//! The build loop from end to end, against a service kept in memory: a build
//! that finishes, polls that struggle, a service that goes quiet, a cancel,
//! and a run found again after a restart.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use super::reply::STALE;
use super::service::{
    self, Answer, Build, Cadence, Finished, Handle, Message, Part, Question, Service, Session,
    Status, now,
};
use super::{Billing, Tokens, Usage};
use crate::platform::job::{Cancel, eventually};

/// How a scripted service answers one poll of its sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reply {
    /// The call fails.
    Fail,
    /// The session works.
    Running,
    /// The session works but wrote nothing for longer than [`STALE`].
    Stale,
    /// The session is done, with nothing to answer.
    Idle,
    /// The session is done and answered.
    Finished,
}

#[derive(Debug)]
struct State {
    replies: VecDeque<Reply>,
    last: Reply,
    polls: usize,
    running: bool,
    session: String,
    slug: Option<String>,
    created: u64,
    running_messages: Vec<Message>,
    done_messages: Vec<Message>,
    usage: Option<Usage>,
    started: Vec<String>,
    said: Vec<String>,
    interrupted: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            replies: VecDeque::new(),
            last: Reply::Finished,
            polls: 0,
            running: false,
            session: "ses_1".into(),
            slug: Some("ticket".into()),
            created: now(),
            running_messages: Vec::new(),
            done_messages: Vec::new(),
            usage: None,
            started: Vec::new(),
            said: Vec::new(),
            interrupted: Vec::new(),
        }
    }
}

impl State {
    /// The next scripted reply; the last one repeats once the script is read.
    fn next(&mut self) -> Reply {
        let reply = self.replies.pop_front().unwrap_or(self.last);
        self.last = reply;
        reply
    }
}

struct Fake {
    key: &'static str,
    state: Mutex<State>,
}

impl Service for Fake {
    fn key(&self) -> &'static str {
        self.key
    }

    fn start(&self, issue: &Path, _project: &Path, model: &str) -> Result<String, String> {
        let mut state = self.lock();
        state.started.push(format!("{} · {model}", issue.display()));
        Ok(state.session.clone())
    }

    fn sessions(&self, _project: &Path) -> Result<Vec<Session>, String> {
        let mut state = self.lock();
        state.polls += 1;
        let reply = state.next();
        let status = match reply {
            Reply::Fail => return Err("the service did not answer".to_string()),
            Reply::Running => Status {
                running: true,
                created: state.created,
                updated: now(),
            },
            Reply::Stale => Status {
                running: true,
                created: state.created,
                updated: now().saturating_sub(STALE.as_millis() as u64 + 1),
            },
            Reply::Idle | Reply::Finished => Status {
                running: false,
                created: state.created,
                updated: now(),
            },
        };
        state.running = status.running;
        let usage = if reply == Reply::Finished {
            state.usage.clone()
        } else {
            None
        };
        Ok(vec![Session {
            id: state.session.clone(),
            slug: state.slug.clone(),
            status,
            ended: status.updated,
            usage,
        }])
    }

    fn read(&self, _project: &Path, _session: &str) -> Result<Vec<Message>, String> {
        let state = self.lock();
        Ok(if state.running {
            state.running_messages.clone()
        } else {
            state.done_messages.clone()
        })
    }

    fn say(&self, _project: &Path, _session: &str, text: &str) -> Result<(), String> {
        self.lock().said.push(text.to_string());
        Ok(())
    }

    fn interrupt(&self, _project: &Path, session: &str) {
        self.lock().interrupted.push(session.to_string());
    }

    fn questions(&self, _project: &Path) -> Result<Vec<Question>, String> {
        Ok(Vec::new())
    }

    fn answer(
        &self,
        _project: &Path,
        _session: &str,
        _id: &str,
        _answer: &Answer,
    ) -> Result<(), String> {
        Ok(())
    }
}

impl Fake {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn with(&self, edit: impl FnOnce(&mut State)) {
        edit(&mut self.lock());
    }

    fn polls(&self) -> usize {
        self.lock().polls
    }

    fn running(&self) -> bool {
        self.lock().running
    }

    fn started(&self) -> Vec<String> {
        self.lock().started.clone()
    }

    fn said(&self) -> Vec<String> {
        self.lock().said.clone()
    }

    fn interrupted(&self) -> Vec<String> {
        self.lock().interrupted.clone()
    }
}

/// A project, the issue a build works on, and the log it writes to.
struct Case {
    _dir: tempfile::TempDir,
    issue: PathBuf,
    project: PathBuf,
    log: PathBuf,
}

fn case() -> Case {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(dir.path().join("issues/doing")).unwrap();
    Case {
        issue: dir.path().join("issues/doing/ticket.md"),
        project,
        log: dir.path().join("issues/.builds/ticket.log"),
        _dir: dir,
    }
}

/// A service that plays `replies`, repeating the last one; it lives for the
/// rest of the test process, as adoption needs a `'static` service.
fn scripted(replies: impl IntoIterator<Item = Reply>) -> &'static Fake {
    Box::leak(Box::new(Fake {
        key: "fake",
        state: Mutex::new(State {
            replies: replies.into_iter().collect(),
            ..State::default()
        }),
    }))
}

/// The loop in milliseconds: it doubles 2, 4, 8, 16 and stays there.
fn fast() -> Cadence {
    Cadence {
        every: Duration::from_millis(2),
        cap: Duration::from_millis(16),
    }
}

fn start(
    service: &'static Fake,
    case: &Case,
    cancel: &Cancel,
    timeout: Option<Duration>,
) -> Result<Finished, String> {
    let build = Build {
        model: "test/model",
        prompt: "Build the ticket.",
        issue: &case.issue,
        project: &case.project,
        log: &case.log,
        timeout,
    };
    let handle = Handle::new(Some(service));
    service::start_with(&build, cancel, None, &handle, fast())
}

fn step() -> Message {
    Message {
        id: "msg_1".into(),
        closed: true,
        parts: vec![Part::Tool {
            name: "read".into(),
            input: serde_json::json!({"path": "/code/app/src/app.rs"}),
            streaming: false,
        }],
    }
}

fn text(message: &str) -> Message {
    Message {
        id: "msg_2".into(),
        closed: true,
        parts: vec![Part::Text(message.into())],
    }
}

fn usage() -> Usage {
    Usage {
        cost: Some(0.42),
        tokens: Tokens {
            output: 7,
            ..Tokens::default()
        },
        provider: Some("test".into()),
        model: Some("test/model".into()),
        billing: Billing::Metered,
    }
}

#[test]
fn a_build_that_finishes_writes_its_steps_and_returns_its_answer() {
    let fake = scripted([Reply::Running, Reply::Finished]);
    let spent = usage();
    fake.with(|state| {
        state.running_messages = vec![step()];
        state.done_messages = vec![step(), text("Done.")];
        state.usage = Some(spent.clone());
    });
    let case = case();
    let finished = start(
        fake,
        &case,
        &Cancel::default(),
        Some(Duration::from_secs(5)),
    )
    .expect("the build answers");

    assert_eq!(finished.answer, "Done.");
    assert_eq!(finished.usage, Some(spent));
    assert_eq!(fake.said(), ["Build the ticket."]);
    assert_eq!(
        fake.started(),
        [format!("{} · test/model", case.issue.display())]
    );
    assert!(fake.interrupted().is_empty());
    let log = std::fs::read_to_string(&case.log).unwrap();
    assert!(log.contains("→ Reading src/app.rs…"), "{log}");
    assert!(log.contains("Done."), "{log}");
}

#[test]
fn failed_polls_recover_and_the_run_finishes() {
    let fake = scripted([
        Reply::Fail,
        Reply::Fail,
        Reply::Fail,
        Reply::Running,
        Reply::Finished,
    ]);
    fake.with(|state| state.done_messages = vec![text("Back.")]);
    let case = case();
    let finished = start(
        fake,
        &case,
        &Cancel::default(),
        Some(Duration::from_secs(5)),
    )
    .expect("the loop waited out the failures");

    assert_eq!(finished.answer, "Back.");
    assert!(fake.polls() >= 5, "it retried: {} polls", fake.polls());
}

#[test]
fn the_waits_double_up_to_the_cap_and_start_over() {
    let cadence = Cadence {
        every: Duration::from_millis(1),
        cap: Duration::from_millis(8),
    };
    let mut misses = 0;
    let mut waits = Vec::new();
    for polled in [false, false, false, false, true, false, false] {
        if polled {
            misses = 0;
            waits.push(cadence.every.as_millis());
        } else {
            misses += 1;
            waits.push(cadence.after(misses).as_millis());
        }
    }
    assert_eq!(waits, [2, 4, 8, 8, 1, 2, 4]);
}

#[test]
fn a_service_that_never_answers_is_stopped_at_the_timeout() {
    let fake = scripted([Reply::Running]);
    let case = case();
    let error = start(
        fake,
        &case,
        &Cancel::default(),
        Some(Duration::from_millis(60)),
    )
    .expect_err("it never answers");

    assert!(error.contains("gave no answer"), "{error}");
    assert_eq!(fake.interrupted(), ["ses_1"]);
}

#[test]
fn a_session_silent_past_stale_is_stopped() {
    let fake = scripted([Reply::Stale]);
    let case = case();
    let error = start(
        fake,
        &case,
        &Cancel::default(),
        Some(Duration::from_secs(5)),
    )
    .expect_err("it stopped writing");

    assert_eq!(error, "the agent stopped writing");
    assert_eq!(fake.interrupted(), ["ses_1"]);
}

#[test]
fn an_idle_session_with_no_answer_ends_the_run() {
    let fake = scripted([Reply::Idle]);
    fake.with(|state| state.done_messages = vec![step()]);
    let case = case();
    let error = start(
        fake,
        &case,
        &Cancel::default(),
        Some(Duration::from_secs(5)),
    )
    .expect_err("there is no answer");

    assert_eq!(error, "the agent returned no answer");
    assert!(fake.interrupted().is_empty(), "nothing to stop");
}

#[test]
fn a_cancel_stops_watching_and_leaves_the_session_running() {
    let fake = scripted([Reply::Running]);
    let case = case();
    let cancel = Cancel::default();
    std::thread::scope(|scope| {
        let stopped = cancel.clone();
        let watching =
            scope.spawn(move || start(fake, &case, &stopped, Some(Duration::from_secs(30))));
        assert!(eventually(|| fake.said().len() == 1), "the build started");
        cancel.cancel();
        let error = watching.join().unwrap().expect_err("the watching stopped");
        assert_eq!(error, "stopped watching the agent");
    });

    assert!(fake.running(), "the service keeps the session");
    assert!(
        fake.interrupted().is_empty(),
        "stopping only stops the watching"
    );
}

#[test]
fn a_running_build_is_found_again_by_its_slug() {
    let fake = scripted([Reply::Running]);
    fake.with(|state| state.created = now().saturating_sub(5_000));
    let case = case();
    let found = service::running(fake, &case.project).expect("the sessions are listed");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].session, "ses_1");
    assert_eq!(found[0].slug, "ticket");
    assert_eq!(found[0].service.key(), "fake");
    assert!(
        found[0].elapsed >= Duration::from_secs(5),
        "{:?}",
        found[0].elapsed
    );
}
