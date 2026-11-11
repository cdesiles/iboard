//! The background builds' shared loop: the service a tool runs them in, the
//! session a run lives in, the questions it waits on, and the builds a restart
//! finds again. What a service itself does lives in [`opencode`](super::opencode)
//! and [`claude`](super::claude); a custom command keeps its child process.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use super::poller::project_poller;
use super::reply::{Wait, await_answer};
use super::transcript::Transcript;
use super::{Progress, Usage};
use crate::domains::agent::artifacts::usage_log;
use crate::platform::job::{Cancel, Job};

/// How often a watched session is asked how it is doing.
pub(super) const POLL: Duration = Duration::from_millis(700);

/// The longest wait between two polls while the service does not answer.
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// How many failed polls it takes for the wait to stop doubling.
const TRIES: u32 = 7;

/// How often the loop asks the service how a run is doing. The caller sets
/// it, so a test plays the whole loop in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cadence {
    /// The wait between two polls while the service answers.
    pub(super) every: Duration,
    /// The longest wait between two polls while it does not.
    pub(super) cap: Duration,
}

impl Cadence {
    /// The wait after `misses` polls in a row without an answer.
    pub(super) fn after(&self, misses: u32) -> Duration {
        self.every
            .saturating_mul(1u32 << misses.min(TRIES))
            .min(self.cap)
    }
}

impl Default for Cadence {
    fn default() -> Self {
        Cadence {
            every: POLL,
            cap: BACKOFF_MAX,
        }
    }
}

/// A tool's background service: everything a build does that differs from one
/// tool to the next. The waits, the stale and no-answer rules and the poller
/// are shared.
pub trait Service: Send + Sync {
    /// The engine's name, as the usage log keeps it.
    fn key(&self) -> &'static str;

    /// Starts a session for `issue` in `project`, titled after it, and hands
    /// it `prompt`.
    fn start(
        &self,
        issue: &Path,
        project: &Path,
        model: &str,
        prompt: &str,
    ) -> Result<String, String>;

    /// The sessions `project` has, running or finished.
    fn sessions(&self, project: &Path) -> Result<Vec<Session>, String>;

    /// The assistant messages of `session`, oldest first.
    fn read(&self, project: &Path, session: &str) -> Result<Vec<Message>, String>;

    /// Queues `text` after the turn `session` runs now.
    fn say(&self, project: &Path, session: &str, text: &str) -> Result<(), String>;

    /// Stops `session`'s turn.
    fn interrupt(&self, project: &Path, session: &str);

    /// The forms and permission requests `project` waits on.
    fn questions(&self, project: &Path) -> Result<Vec<Question>, String>;

    /// Answers `session`'s question `id`.
    fn answer(
        &self,
        project: &Path,
        session: &str,
        id: &str,
        answer: &Answer,
    ) -> Result<(), String>;
}

impl PartialEq for dyn Service {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for dyn Service {}

impl fmt::Debug for dyn Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Service").field(&self.key()).finish()
    }
}

/// The service a test's question belongs to, with no daemon behind it.
#[cfg(test)]
pub fn test_service() -> &'static dyn Service {
    super::opencode::service()
}

/// How a session stands, as its service keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub running: bool,
    /// Wall-clock milliseconds, as the service counts them.
    pub created: u64,
    pub updated: u64,
}

/// One session a service lists for a project: how it stands, what it spent,
/// and the issue it was started for.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: String,
    /// The issue's slug, when iboard started it.
    pub slug: Option<String>,
    pub status: Status,
    /// When it last wrote anything, in the service's milliseconds.
    pub ended: u64,
    pub usage: Option<Usage>,
}

/// One assistant message of a session's transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: String,
    /// Whether it is done streaming: its last text part is complete.
    pub closed: bool,
    pub parts: Vec<Part>,
}

/// What one part of a message adds.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    /// A tool call, with the input that says what it is doing.
    Tool {
        name: String,
        input: Value,
        streaming: bool,
    },
    /// A part the log leaves out.
    Other,
}

/// What a question is answered with.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Form(Map<String, Value>),
    Permission(PermissionReply),
}

/// The session a run lives in, once the service has made it. Stop interrupts
/// it; quitting only drops this, and the service carries on.
#[derive(Clone)]
pub struct Handle {
    service: Option<&'static dyn Service>,
    session: Arc<Mutex<Option<String>>>,
}

impl Default for Handle {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Handle {
    /// A handle for a run in `service`; `None` when the tool keeps its child
    /// process.
    pub fn new(service: Option<&'static dyn Service>) -> Self {
        Handle {
            service,
            session: Arc::default(),
        }
    }

    /// The service the run lives in, when it runs in one.
    pub fn service(&self) -> Option<&'static dyn Service> {
        self.service
    }
}

/// The session in `handle`, if the service made it yet.
pub fn session_of(handle: &Handle) -> Option<String> {
    handle
        .session
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Notes that `handle`'s run lives in `session`.
pub fn remember(handle: &Handle, session: &str) {
    *handle
        .session
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(session.to_string());
}

/// A form or permission request a build is waiting on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub service: &'static dyn Service,
    pub session: String,
    pub id: String,
    pub title: String,
    pub kind: QuestionKind,
}

impl Question {
    pub fn new(
        service: &'static dyn Service,
        session: impl Into<String>,
        id: impl Into<String>,
        title: impl Into<String>,
        kind: QuestionKind,
    ) -> Self {
        Question {
            service,
            session: session.into(),
            id: id.into(),
            title: title.into(),
            kind,
        }
    }

    /// Answers it in `project`, where its build runs.
    pub fn answer(&self, project: &Path, answer: Answer) -> Result<(), String> {
        self.service
            .answer(project, &self.session, &self.id, &answer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionKind {
    Form(Vec<Field>),
    Permission(Permission),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    pub action: String,
    pub resources: Vec<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub title: String,
    pub description: String,
    pub required: bool,
    pub hidden: bool,
    pub kind: FieldKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    Text {
        options: Vec<Choice>,
        placeholder: String,
        default: String,
        custom: bool,
    },
    Number {
        default: String,
        integer: bool,
    },
    Boolean {
        default: bool,
    },
    Multi {
        options: Vec<Choice>,
        default: Vec<String>,
        custom: bool,
    },
    External {
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub description: String,
}

/// How a tool approval is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionReply {
    Once,
    Always,
    Reject,
}

impl PermissionReply {
    pub(super) fn key(self) -> &'static str {
        match self {
            PermissionReply::Once => "once",
            PermissionReply::Always => "always",
            PermissionReply::Reject => "reject",
        }
    }
}

/// The pending questions of a project, refreshed by [`follow_questions`].
#[derive(Clone, Default)]
pub struct Feed {
    questions: Arc<Mutex<Vec<Question>>>,
    settled: Arc<Mutex<HashSet<(String, String)>>>,
}

impl Feed {
    /// A feed already holding `questions`, for a board built without a service.
    pub fn new(questions: Vec<Question>) -> Self {
        Self {
            questions: Arc::new(Mutex::new(questions)),
            settled: Arc::default(),
        }
    }

    pub fn questions(&self) -> Vec<Question> {
        self.questions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Drops a question iboard just answered, before the next poll catches up.
    pub fn remove(&self, session: &str, id: &str) {
        self.settled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((session.to_string(), id.to_string()));
        self.questions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|question| question.session != session || question.id != id);
    }

    /// Replaces the pending questions with what the service just said.
    pub(super) fn publish(&self, mut questions: Vec<Question>) {
        let settled = self.settled.lock().unwrap_or_else(PoisonError::into_inner);
        questions
            .retain(|question| !settled.contains(&(question.session.clone(), question.id.clone())));
        let mut current = self
            .questions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if *current != questions {
            *current = questions;
        }
    }
}

/// Polls `project`'s pending forms and permission requests in the background
/// until the job is dropped.
pub fn follow_questions(service: &'static dyn Service, project: PathBuf) -> (Job<()>, Feed) {
    let poller = project_poller(service, &project, Cadence::default());
    let feed = poller.feed();
    let subscription = poller.subscribe(true);
    let job = Job::spawn(move |cancel| {
        let _subscription = subscription;
        while !cancel.is_cancelled() {
            std::thread::sleep(POLL);
        }
    });
    (job, feed)
}

/// A build the service is still running, found again after a restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub service: &'static dyn Service,
    pub session: String,
    /// The issue's slug, as its session's metadata has it.
    pub slug: String,
    /// Since when the service has been on it.
    pub elapsed: Duration,
}

/// A run the service finished: its answer, and what its session spent.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    pub answer: String,
    pub usage: Option<Usage>,
}

/// One build to run in the service, and where its log goes.
pub struct Build<'a> {
    pub model: &'a str,
    pub prompt: &'a str,
    pub issue: &'a Path,
    pub project: &'a Path,
    pub log: &'a Path,
    pub timeout: Option<Duration>,
}

/// Runs a build in a new session, then waits for its answer.
pub fn start(
    build: &Build,
    cancel: &Cancel,
    progress: Option<&Progress>,
    handle: &Handle,
) -> Result<Finished, String> {
    start_with(build, cancel, progress, handle, Cadence::default())
}

/// [`start`] with a cadence the caller chose.
pub(super) fn start_with(
    build: &Build,
    cancel: &Cancel,
    progress: Option<&Progress>,
    handle: &Handle,
    cadence: Cadence,
) -> Result<Finished, String> {
    let service = handle
        .service()
        .ok_or_else(|| "no background service runs this build".to_string())?;
    let session = service.start(build.issue, build.project, build.model, build.prompt)?;
    remember(handle, &session);
    if cancel.is_cancelled() {
        return Err("the agent was not started".to_string());
    }
    await_answer(
        service,
        build.project,
        &session,
        cancel,
        Wait {
            timeout: build.timeout,
            cadence,
        },
        progress,
        Transcript::new(build.log),
    )
}

/// The builds the service is still running for `project`, as a restart finds
/// them. The finished sessions in the same list go to the project's usage log.
pub fn running(service: &'static dyn Service, project: &Path) -> Result<Vec<Found>, String> {
    let sessions = service.sessions(project)?;
    let _ = usage_log::record(&sessions, project, service.key());
    Ok(sessions
        .iter()
        .filter_map(|session| found(service, session))
        .collect())
}

/// One running iboard session, if that is what `session` is.
fn found(service: &'static dyn Service, session: &Session) -> Option<Found> {
    if !session.status.running {
        return None;
    }
    Some(Found {
        service,
        session: session.id.clone(),
        slug: session.slug.clone()?,
        elapsed: Duration::from_millis(now().saturating_sub(session.status.created)),
    })
}

/// When a run that has been going for `elapsed` started.
pub fn started(elapsed: Duration) -> Instant {
    Instant::now()
        .checked_sub(elapsed)
        .unwrap_or_else(Instant::now)
}

/// Watches `session` on a thread of its own, for a run adopted after a
/// restart: nothing to start, only an answer to wait for.
pub use super::reply::watch;

/// Wall-clock milliseconds, as the service counts them.
pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(running: bool, slug: Option<&str>) -> Session {
        Session {
            id: "ses_1".into(),
            slug: slug.map(str::to_string),
            status: Status {
                running,
                created: 1_000,
                updated: 2_000,
            },
            ended: 2_000,
            usage: None,
        }
    }

    #[test]
    fn a_handle_remembers_the_session_for_stop() {
        let handle = Handle::default();
        assert_eq!(session_of(&handle), None);
        assert_eq!(handle.service(), None);
        remember(&handle, "ses_42");
        assert_eq!(session_of(&handle).as_deref(), Some("ses_42"));
    }

    #[test]
    fn only_a_running_iboard_session_is_adopted() {
        assert_eq!(found(test_service(), &session(false, Some("build"))), None);
        assert_eq!(found(test_service(), &session(true, None)), None);
        let adopted = found(test_service(), &session(true, Some("build")));
        assert_eq!(
            adopted.map(|found| (found.session, found.slug)),
            Some(("ses_1".to_string(), "build".to_string()))
        );
    }

    #[test]
    fn a_feed_forgets_a_question_once_answered() {
        let question = Question::new(
            test_service(),
            "ses_1",
            "frm_1",
            "Which approach?",
            QuestionKind::Form(Vec::new()),
        );
        let feed = Feed::new(vec![question.clone()]);
        assert_eq!(feed.questions(), [question]);
        feed.remove("ses_1", "frm_1");
        assert!(feed.questions().is_empty());
    }

    #[test]
    fn failed_polls_back_off_up_to_a_cap() {
        let cadence = Cadence::default();
        assert_eq!(cadence.after(0), POLL);
        assert_eq!(cadence.after(1), POLL * 2);
        assert_eq!(cadence.after(2), POLL * 4);
        assert_eq!(cadence.after(TRIES), BACKOFF_MAX);
    }
}
