//! One poller per project: it asks the service once for every session's state
//! and for the project's questions, and the runs read what it last saw.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::Usage;
use super::reply::{call, parse};
use super::service::{Feed, POLL, backoff, pending};
use crate::domains::agent::artifacts::usage_log;

/// The live pollers, so every run in a project shares one.
static POLLERS: LazyLock<Mutex<HashMap<PathBuf, Weak<Poller>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The longest the shared poller waits before trying a struggling service again.
const WAIT_MAX: Duration = Duration::from_secs(5);

/// How a session stands, as the service keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Status {
    pub(crate) running: bool,
    /// Wall-clock milliseconds, as the service counts them.
    pub(crate) created: u64,
    pub(crate) updated: u64,
}

/// What one poll found for a session: how it stands, and what it spent.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Polled {
    pub(super) status: Status,
    pub(super) usage: Option<Usage>,
}

/// The state of one service session.
pub(crate) fn status_of(session: &Value) -> Status {
    let millis = |key: &str| session["time"][key].as_u64().unwrap_or(0);
    let (busy, idle) = (millis("busy"), millis("idle"));
    Status {
        running: busy > 0 && idle < busy,
        created: millis("created"),
        updated: millis("updated").max(busy),
    }
}

/// The watchers of one project's runs and questions.
pub(super) struct Poller {
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

/// The poller for `project`, started if no live one exists.
pub(super) fn project_poller(project: &Path) -> Arc<Poller> {
    let project = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let mut pollers = POLLERS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(poller) = pollers.get(&project).and_then(Weak::upgrade) {
        return poller;
    }
    let poller = Arc::new(Poller::new(project.clone()));
    pollers.insert(project, Arc::downgrade(&poller));
    drop(pollers);
    poller.start();
    poller
}

impl Poller {
    fn new(project: PathBuf) -> Self {
        Self {
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
            match statuses(&self.project) {
                Ok((statuses, listed)) => {
                    self.log_finished(&listed);
                    let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
                    inner.statuses = statuses;
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
            match pending(&self.project) {
                Ok(questions) => self.feed().publish(questions),
                Err(_) => ok = false,
            }
        }
        ok
    }

    /// Adds the finished sessions this poller has not seen finish yet.
    fn log_finished(&self, sessions: &[Value]) {
        let finished = usage_log::finished(sessions, &self.project);
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

/// How every session of `project` stands, from one request, with the sessions
/// themselves for the usage log.
fn statuses(project: &Path) -> Result<(HashMap<String, Polled>, Vec<Value>), String> {
    let reply = call(project, &["get", "/api/session"], None)?;
    let value = parse(&reply)?;
    let sessions = value["data"].as_array().cloned().unwrap_or_default();
    Ok((statuses_from(&value, project), sessions))
}

/// The sessions of `project` in a session list.
fn statuses_from(value: &Value, project: &Path) -> HashMap<String, Polled> {
    let sessions = value["data"].as_array().cloned().unwrap_or_default();
    sessions
        .iter()
        .filter_map(|session| {
            let id = session["id"].as_str()?;
            let directory = session["location"]["directory"].as_str()?;
            (Path::new(directory) == project).then(|| {
                (
                    id.to_string(),
                    Polled {
                        status: status_of(session),
                        usage: Usage::from_session(session),
                    },
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::Billing;
    use super::*;

    #[test]
    fn a_polled_session_keeps_what_it_spent() {
        let value = serde_json::json!({"data": [{
            "id": "ses_copilot", "location": {"directory": "/code/app"},
            "model": {"id": "claude-opus-4.6", "providerID": "github-copilot"},
            "cost": 0,
            "tokens": {"input": 159, "output": 14001, "reasoning": 0,
                       "cache": {"read": 1902598, "write": 383223}},
            "time": {"busy": 2, "idle": 1},
        }]});
        let polled = statuses_from(&value, Path::new("/code/app"));
        let usage = polled["ses_copilot"].usage.as_ref().expect("usage");
        assert_eq!(usage.cost, Some(0.0));
        assert_eq!(usage.billing, Billing::Subscription);
        assert_eq!(usage.tokens.cache_read, 1_902_598);
    }

    #[test]
    fn only_this_projects_sessions_are_polled() {
        let value = serde_json::json!({"data": [
            {"id": "ses_here", "location": {"directory": "/code/app"},
             "time": {"busy": 2, "idle": 1}},
            {"id": "ses_there", "location": {"directory": "/code/other"},
             "time": {"busy": 2}},
        ]});
        let statuses = statuses_from(&value, Path::new("/code/app"));
        assert!(statuses.contains_key("ses_here"));
        assert_eq!(statuses.len(), 1);
    }
}
