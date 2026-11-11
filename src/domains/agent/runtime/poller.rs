//! One poller per project: it asks the service once for every session's state
//! and for the project's questions, and the runs read what it last saw.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::service::{Feed, POLL, Status, backoff, call, parse, pending, status_of};

/// The live pollers, so every run in a project shares one.
static POLLERS: LazyLock<Mutex<HashMap<PathBuf, Weak<Poller>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The longest the shared poller waits before trying a struggling service again.
const WAIT_MAX: Duration = Duration::from_secs(5);

/// The watchers of one project's runs and questions.
pub(super) struct Poller {
    project: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    sessions: usize,
    questions: usize,
    statuses: HashMap<String, Status>,
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

    pub(super) fn status(&self, session: &str, since: Instant) -> Option<(Status, Instant)> {
        let inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let observed = inner.observed?;
        if observed < since {
            return None;
        }
        inner
            .statuses
            .get(session)
            .copied()
            .map(|status| (status, observed))
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
                Ok(statuses) => {
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

/// How every session of `project` stands, from one request.
fn statuses(project: &Path) -> Result<HashMap<String, Status>, String> {
    let reply = call(project, &["get", "/api/session"], None)?;
    let value = parse(&reply)?;
    Ok(statuses_from(&value, project))
}

/// The sessions of `project` in a session list.
fn statuses_from(value: &Value, project: &Path) -> HashMap<String, Status> {
    let sessions = value["data"].as_array().cloned().unwrap_or_default();
    sessions
        .iter()
        .filter_map(|session| {
            let id = session["id"].as_str()?;
            let directory = session["location"]["directory"].as_str()?;
            (Path::new(directory) == project).then_some((id.to_string(), status_of(session)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
