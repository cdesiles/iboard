//! One poller per service and project: it asks the service once for every
//! session's state and for the project's questions, and the runs read what it
//! last saw.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use super::Usage;
use super::service::{Feed, POLL, Service, Session, Status, backoff};
use crate::domains::agent::artifacts::usage_log;

/// The live pollers, so every run in a service and project shares one.
static POLLERS: LazyLock<Mutex<Pollers>> = LazyLock::new(|| Mutex::new(HashMap::new()));

type Pollers = HashMap<(&'static str, PathBuf), Weak<Poller>>;

/// The longest the shared poller waits before trying a struggling service again.
const WAIT_MAX: Duration = Duration::from_secs(5);

/// What one poll found for a session: how it stands, and what it spent.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Polled {
    pub(super) status: Status,
    pub(super) usage: Option<Usage>,
}

/// The watchers of one project's runs and questions.
pub(super) struct Poller {
    service: &'static dyn Service,
    project: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    sessions: usize,
    questions: usize,
    statuses: HashMap<String, Polled>,
    logged: HashSet<String>,
    observed: Option<Instant>,
    feed: Feed,
    running: bool,
}

/// Keeps its poller alive while it is held.
pub(super) struct Subscription {
    poller: Arc<Poller>,
    questions: bool,
}

/// The poller for `project` in `service`, started if no live one exists.
pub(super) fn project_poller(service: &'static dyn Service, project: &Path) -> Arc<Poller> {
    let project = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let key = (service.key(), project.clone());
    let mut pollers = POLLERS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(poller) = pollers.get(&key).and_then(Weak::upgrade) {
        return poller;
    }
    let poller = Arc::new(Poller::new(service, project));
    pollers.insert(key, Arc::downgrade(&poller));
    drop(pollers);
    poller.start();
    poller
}

impl Poller {
    fn new(service: &'static dyn Service, project: PathBuf) -> Self {
        Self {
            service,
            project,
            inner: Mutex::new(Inner {
                sessions: 0,
                questions: 0,
                statuses: HashMap::new(),
                logged: HashSet::new(),
                observed: None,
                feed: Feed::default(),
                running: false,
            }),
        }
    }

    pub(super) fn subscribe(self: &Arc<Self>, questions: bool) -> Subscription {
        let start = {
            let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            if questions {
                inner.questions += 1;
            } else {
                inner.sessions += 1;
            }
            !inner.running
        };
        if start {
            self.start();
        }
        Subscription {
            poller: Arc::clone(self),
            questions,
        }
    }

    pub(super) fn status(&self, session: &str, since: Instant) -> Option<(Polled, Instant)> {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let observed = inner.observed?;
        if observed < since {
            return None;
        }
        inner
            .statuses
            .get(session)
            .cloned()
            .map(|polled| (polled, observed))
    }

    pub(super) fn feed(&self) -> Feed {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .feed
            .clone()
    }

    fn alive(&self) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let alive = inner.sessions + inner.questions > 0;
        if !alive {
            inner.running = false;
        }
        alive
    }

    /// Asks for what the watchers need; false when a call failed.
    fn poll(&self) -> bool {
        let (sessions, questions) = {
            let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            (inner.sessions > 0, inner.questions > 0)
        };
        let mut ok = true;
        if sessions {
            match self.service.sessions(&self.project) {
                Ok(sessions) => {
                    self.log_finished(&sessions);
                    let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
                    inner.statuses = statuses_from(&sessions);
                    inner.observed = Some(Instant::now());
                }
                Err(_) => {
                    let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
                    inner.statuses.clear();
                    inner.observed = None;
                    ok = false;
                }
            }
        }
        if questions {
            match self.service.questions(&self.project) {
                Ok(questions) => self.feed().publish(questions),
                Err(_) => ok = false,
            }
        }
        ok
    }

    /// Adds the finished sessions this poller has not seen finish yet.
    fn log_finished(&self, sessions: &[Session]) {
        let finished = usage_log::finished(sessions, &self.project, self.service.key());
        let new: Vec<_> = {
            let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            finished
                .into_iter()
                .filter(|entry| !inner.logged.contains(&entry.session))
                .collect()
        };
        if new.is_empty() || usage_log::append(&new).is_err() {
            return;
        }
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        inner
            .logged
            .extend(new.into_iter().map(|entry| entry.session));
    }

    fn start(self: &Arc<Self>) {
        {
            let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            if inner.running {
                return;
            }
            inner.running = true;
        }
        let weak = Arc::downgrade(self);
        std::thread::spawn(move || {
            let mut misses = 0;
            loop {
                let Some(poller) = weak.upgrade() else {
                    break;
                };
                if !poller.alive() {
                    break;
                }
                let ok = poller.poll();
                misses = if ok { 0 } else { misses + 1 };
                let wait = if ok {
                    POLL
                } else {
                    backoff(misses).min(WAIT_MAX)
                };
                drop(poller);
                if !waiting(&weak, wait) {
                    break;
                }
            }
        });
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut inner = self
            .poller
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.questions {
            inner.questions = inner.questions.saturating_sub(1);
        } else {
            inner.sessions = inner.sessions.saturating_sub(1);
        }
    }
}

/// Sleeps for `wait`, returning false as soon as no watcher is left.
fn waiting(poller: &Weak<Poller>, wait: Duration) -> bool {
    let until = Instant::now() + wait;
    loop {
        let Some(poller) = poller.upgrade() else {
            return false;
        };
        if !poller.alive() {
            return false;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        std::thread::sleep(left.min(POLL));
    }
}

/// How every session stands, from the ones a service just listed.
fn statuses_from(sessions: &[Session]) -> HashMap<String, Polled> {
    sessions
        .iter()
        .map(|session| {
            (
                session.id.clone(),
                Polled {
                    status: session.status,
                    usage: session.usage.clone(),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::Billing;
    use super::*;
    use crate::domains::agent::runtime::{Tokens, Usage};

    fn session(id: &str, usage: Option<Usage>) -> Session {
        Session {
            id: id.to_string(),
            slug: None,
            status: Status {
                running: true,
                created: 1_000,
                updated: 2_000,
            },
            ended: 2_000,
            usage,
        }
    }

    #[test]
    fn a_polled_session_keeps_what_it_spent() {
        let usage = Usage {
            cost: Some(0.0),
            tokens: Tokens {
                input: 159,
                output: 14_001,
                cache_read: 1_902_598,
                cache_write: 383_223,
                ..Tokens::default()
            },
            provider: Some("github-copilot".into()),
            model: Some("claude-opus-4.6".into()),
            billing: Billing::Subscription,
        };
        let polled = statuses_from(&[session("ses_copilot", Some(usage))]);
        let usage = polled["ses_copilot"].usage.as_ref().expect("usage");
        assert_eq!(usage.cost, Some(0.0));
        assert_eq!(usage.billing, Billing::Subscription);
        assert_eq!(usage.tokens.cache_read, 1_902_598);
    }

    #[test]
    fn every_listed_session_is_polled() {
        let statuses = statuses_from(&[session("ses_here", None), session("ses_other", None)]);
        assert_eq!(statuses.len(), 2);
    }
}
