//! opencode's background service: the sessions it owns, spoken to through
//! `opencode api`. A build started here outlives the window, because the
//! service runs the turn and iboard only watches it: quitting stops the
//! watching, not the work.
//!
//! Set `IBOARD_OPENCODE` to another binary to test this without one.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::{Progress, describe_tool};
use crate::job::{Cancel, Job};
use crate::{build_log, process, reviews};

/// How long one call to the service may take.
const CALL: Duration = Duration::from_secs(20);

/// How long the board waits for the service when it opens: short enough to
/// notice, because this one runs on the UI thread.
const LIST: Duration = Duration::from_secs(4);

/// How many of a session's newest messages its step and answer are read from.
const PAGE: usize = 20;

/// How often a watched session is asked how it is doing.
const POLL: Duration = Duration::from_millis(700);

/// How many times a read is retried when its reply comes back cut short.
const RETRIES: usize = 3;

/// How long a session may go without writing anything before its run is
/// given up as dead.
const STALE: Duration = Duration::from_secs(30 * 60);

/// How many calls in a row may fail before the run is given up.
const TRIES: usize = 5;

/// The session a run lives in, once the service has made it. Stop interrupts
/// it; quitting only drops this, and the service carries on.
pub type Handle = Arc<Mutex<Option<String>>>;

/// The session in `handle`, if the service made it yet.
pub fn session_of(handle: &Handle) -> Option<String> {
    handle
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Notes that `handle`'s run lives in `session`.
pub fn remember(handle: &Handle, session: &str) {
    *handle.lock().unwrap_or_else(PoisonError::into_inner) = Some(session.to_string());
}

/// A form or permission request a build is waiting on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub session: String,
    pub id: String,
    pub title: String,
    pub kind: QuestionKind,
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
    fn key(self) -> &'static str {
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
}

/// Polls `project`'s pending forms and permission requests in the background
/// until the job is dropped.
pub fn follow_questions(project: PathBuf) -> (Job<()>, Feed) {
    let feed = Feed::default();
    let writing = feed.clone();
    let job = Job::spawn(move |cancel| {
        let mut seen: Option<Vec<Question>> = None;
        while !cancel.is_cancelled() {
            if let Ok(mut questions) = pending(&project) {
                let settled = writing
                    .settled
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                questions.retain(|question| {
                    !settled.contains(&(question.session.clone(), question.id.clone()))
                });
                if seen.as_ref() != Some(&questions) {
                    *writing
                        .questions
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = questions.clone();
                    seen = Some(questions);
                }
            }
            std::thread::sleep(POLL);
        }
    });
    (job, feed)
}

/// The forms and permission requests `project` waits on.
pub fn pending(project: &Path) -> Result<Vec<Question>, String> {
    let forms = list(project, "form.list");
    let permissions = list(project, "permission.request.list");
    match (forms, permissions) {
        (Ok(forms), Ok(permissions)) => Ok(questions(forms, permissions)),
        (Ok(forms), Err(_)) => Ok(questions(forms, Vec::new())),
        (Err(_), Ok(permissions)) => Ok(questions(Vec::new(), permissions)),
        (Err(forms), Err(permissions)) => Err(format!("{forms}; {permissions}")),
    }
}

fn questions(forms: Vec<Value>, permissions: Vec<Value>) -> Vec<Question> {
    let forms = forms.iter().filter_map(question_from_form);
    let permissions = permissions.iter().filter_map(question_from_permission);
    forms.chain(permissions).collect()
}

/// The pending items of one opencode list operation. The operation ID, not a
/// raw method and path: only an operation binds `--param`, and without the
/// location the service answers for its default folder.
fn list(project: &Path, operation: &str) -> Result<Vec<Value>, String> {
    let args = list_args(project, operation);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let reply = call(project, &args, None)?;
    let value = parse(&reply)?;
    Ok(value["data"].as_array().cloned().unwrap_or_default())
}

fn list_args(project: &Path, operation: &str) -> Vec<String> {
    let directory = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    vec![
        operation.to_string(),
        "--param".to_string(),
        format!("location[directory]={}", directory.display()),
    ]
}

fn question_from_form(value: &Value) -> Option<Question> {
    let fields = value["fields"]
        .as_array()?
        .iter()
        .filter_map(field_from)
        .collect();
    Some(Question {
        session: value["sessionID"].as_str()?.to_string(),
        id: value["id"].as_str()?.to_string(),
        title: value["title"].as_str()?.to_string(),
        kind: QuestionKind::Form(fields),
    })
}

fn field_from(value: &Value) -> Option<Field> {
    let key = value["key"].as_str()?.to_string();
    let kind = match value["type"].as_str()? {
        "string" => FieldKind::Text {
            options: choices(value),
            placeholder: value["placeholder"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            default: value["default"].as_str().unwrap_or_default().to_string(),
            custom: value["custom"].as_bool().unwrap_or(false),
        },
        "number" | "integer" => FieldKind::Number {
            default: number(value["default"].as_f64()),
            integer: value["type"] == "integer",
        },
        "boolean" => FieldKind::Boolean {
            default: value["default"].as_bool().unwrap_or(false),
        },
        "multiselect" => FieldKind::Multi {
            options: choices(value),
            default: value["default"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect(),
            custom: value["custom"].as_bool().unwrap_or(false),
        },
        "external" => FieldKind::External {
            url: value["url"].as_str().unwrap_or_default().to_string(),
        },
        _ => return None,
    };
    Some(Field {
        title: value["title"].as_str().unwrap_or(&key).to_string(),
        description: value["description"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        required: value["required"].as_bool().unwrap_or(false),
        hidden: value["hidden"].as_bool().unwrap_or(false),
        key,
        kind,
    })
}

fn choices(value: &Value) -> Vec<Choice> {
    value["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|option| {
            let value = option["value"].as_str()?.to_string();
            Some(Choice {
                label: option["label"].as_str().unwrap_or(&value).to_string(),
                description: option["description"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                value,
            })
        })
        .collect()
}

fn number(value: Option<f64>) -> String {
    value.map(|value| format!("{value}")).unwrap_or_default()
}

fn question_from_permission(value: &Value) -> Option<Question> {
    let action = value["action"].as_str()?.to_string();
    Some(Question {
        session: value["sessionID"].as_str()?.to_string(),
        id: value["id"].as_str()?.to_string(),
        title: format!("Allow {action}?"),
        kind: QuestionKind::Permission(Permission {
            action,
            resources: value["resources"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|resource| resource.as_str().map(str::to_string))
                .collect(),
            message: value["message"].as_str().map(str::to_string),
        }),
    })
}

/// Answers a form `question`, whose keys are its fields' keys.
pub fn answer_form(
    project: &Path,
    session: &str,
    id: &str,
    answer: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    let path = format!("/api/session/{session}/form/{id}/reply");
    let body = json!({ "answer": answer }).to_string();
    call(project, &["post", &path], Some(&body)).map(|_| ())
}

/// Answers a tool's permission request.
pub fn answer_permission(
    project: &Path,
    session: &str,
    id: &str,
    reply: PermissionReply,
) -> Result<(), String> {
    let path = format!("/api/session/{session}/permission/{id}/reply");
    let body = json!({ "decision": reply.key() }).to_string();
    call(project, &["post", &path], Some(&body)).map(|_| ())
}

/// A build the service is still running, found again after a restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub session: String,
    /// The issue's slug, as its session's metadata has it.
    pub slug: String,
    /// Since when the service has been on it.
    pub elapsed: Duration,
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
) -> Result<String, String> {
    let session = create(build.project, build.issue, build.model)?;
    remember(handle, &session);
    if cancel.is_cancelled() {
        return Err("the agent was not started".to_string());
    }
    prompt(build.project, &session, build.prompt)?;
    await_answer(
        build.project,
        &session,
        cancel,
        build.timeout,
        progress,
        Some(build.log),
    )
}

/// Creates a session for `issue` in `project`, titled after it.
fn create(project: &Path, issue: &Path, model: &str) -> Result<String, String> {
    let title = crate::files::file_name(issue);
    let directory = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let mut body = json!({
        "title": title,
        "location": { "directory": directory },
        "metadata": { "iboard": { "slug": reviews::slug(issue) } },
    });
    if let Some(model) = model_ref(model) {
        body["model"] = model;
    }
    let reply = call(project, &["post", "/api/session"], Some(&body.to_string()))?;
    let value = parse(&reply)?;
    value["data"]["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "opencode made no session".to_string())
}

/// `provider/model#variant`, as the service wants it, or `None` when the
/// model is the service's own choice.
fn model_ref(model: &str) -> Option<Value> {
    let model = model.split('#').next()?.trim();
    let (provider, id) = model.split_once('/')?;
    Some(json!({ "providerID": provider, "id": id }))
}

/// Hands `text` to `session`; the service runs it whether or not anyone
/// watches.
fn prompt(project: &Path, session: &str, text: &str) -> Result<(), String> {
    let path = format!("/api/session/{session}/prompt");
    let body = json!({ "text": text, "delivery": "queue" }).to_string();
    call(project, &["post", &path], Some(&body)).map(|_| ())
}

/// Queues `text` after the turn `session` runs now, for an instruction typed
/// while the agent works.
pub fn say(project: &Path, session: &str, text: &str) -> Result<(), String> {
    prompt(project, session, text)
}

/// Stops `session`'s turn, for Stop on the card.
pub fn interrupt(project: &Path, session: &str) {
    let path = format!("/api/session/{session}/interrupt");
    let _ = call(project, &["post", &path], None);
}

/// How a session stands, as the service keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Status {
    running: bool,
    /// Wall-clock milliseconds, as the service counts them.
    created: u64,
    updated: u64,
}

fn status(project: &Path, session: &str) -> Result<Status, String> {
    let path = format!("/api/session/{session}");
    let reply = call(project, &["get", &path], None)?;
    let value = parse(&reply)?;
    let data = &value["data"];
    let millis = |key: &str| data["time"][key].as_u64().unwrap_or(0);
    let (busy, idle) = (millis("busy"), millis("idle"));
    Ok(Status {
        running: busy > 0 && idle < busy,
        created: millis("created"),
        updated: millis("updated").max(busy),
    })
}

/// The last step `session` took, and the answer of its last message. Only its
/// newest messages: a transcript grows without bound, and the service's reply
/// loses its tail once large enough (see [`call_with`]).
fn read(project: &Path, session: &str) -> Result<(Option<String>, Option<String>), String> {
    let path = format!("/api/session/{session}/message?order=desc&limit={PAGE}");
    let reply = call(project, &["get", &path], None)?;
    let value = parse(&reply)?;
    let mut messages = value["data"].as_array().cloned().unwrap_or_default();
    messages.reverse();
    Ok(scan(&messages))
}

/// The last step and the last answer in `messages`.
fn scan(messages: &[Value]) -> (Option<String>, Option<String>) {
    let mut step = None;
    let mut answer = None;
    for message in messages.iter().filter(|m| m["type"] == "assistant") {
        let Some(parts) = message["content"].as_array() else {
            continue;
        };
        let mut text = String::new();
        for part in parts {
            match part["type"].as_str() {
                Some("tool" | "question") => {
                    let tool = part["name"].as_str().unwrap_or("tool");
                    step = describe_tool(tool, &part["state"]["input"]);
                }
                Some("text") => text.push_str(part["text"].as_str().unwrap_or_default()),
                _ => {}
            }
        }
        if !text.trim().is_empty() {
            answer = Some(text);
        }
    }
    (step, answer)
}

/// Waits for `session` to finish, reporting its steps as they come. The job
/// that runs this is dropped when the window closes; the service keeps
/// working, so stopping only stops the watching.
pub fn await_answer(
    project: &Path,
    session: &str,
    cancel: &Cancel,
    timeout: Option<Duration>,
    progress: Option<&Progress>,
    log: Option<&Path>,
) -> Result<String, String> {
    let started = Instant::now();
    let mut log = log.and_then(|path| build_log::open(path).ok());
    let mut last = String::new();
    let mut seen = 0;
    let mut misses = 0;
    loop {
        if cancel.is_cancelled() {
            return Err("stopped watching the agent".to_string());
        }
        let status = match status(project, session) {
            Ok(status) => {
                misses = 0;
                status
            }
            Err(err) => {
                misses += 1;
                if misses >= TRIES {
                    return Err(err);
                }
                std::thread::sleep(POLL);
                continue;
            }
        };
        if !status.running {
            return read(project, session)?
                .1
                .ok_or_else(|| "the agent returned no answer".to_string());
        }
        if let Some(timeout) = timeout.filter(|limit| started.elapsed() >= *limit) {
            interrupt(project, session);
            return Err(format!(
                "the agent gave no answer within {}",
                minutes(timeout)
            ));
        }
        if now().saturating_sub(status.updated) > STALE.as_millis() as u64 {
            interrupt(project, session);
            return Err("the agent stopped writing".to_string());
        }
        if status.updated != seen {
            seen = status.updated;
            let (step, _) = read(project, session)?;
            if let Some(step) = step.filter(|step| *step != last) {
                last = step.clone();
                if let Some(progress) = progress {
                    *progress.lock().unwrap_or_else(PoisonError::into_inner) = Some(step.clone());
                }
                if let Some(log) = log.as_mut() {
                    let _ = build_log::write_line(log, &step);
                }
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Watches `session` on a thread of its own, for a run adopted after a
/// restart: nothing to start, only an answer to wait for.
pub fn watch(project: PathBuf, session: String, log: PathBuf) -> Job<Result<String, String>> {
    Job::spawn(move |cancel| {
        let result = await_answer(&project, &session, cancel, None, None, Some(&log));
        let _ = build_log::append(&log, &build_log::outcome(&result, Duration::ZERO));
        result
    })
}

/// The builds the service is still running for `project`, as a restart finds
/// them.
pub fn running(project: &Path) -> Result<Vec<Found>, String> {
    let reply = call_with(project, &["get", "/api/session"], None, LIST)?;
    let value = parse(&reply)?;
    let sessions = value["data"].as_array().cloned().unwrap_or_default();
    let wanted = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    Ok(sessions
        .iter()
        .filter_map(|session| found(session, &wanted))
        .collect())
}

/// One running iboard session, if that is what `session` is.
fn found(session: &Value, project: &Path) -> Option<Found> {
    let slug = session["metadata"]["iboard"]["slug"].as_str()?.to_string();
    let directory = session["location"]["directory"].as_str()?;
    if Path::new(directory) != project {
        return None;
    }
    let millis = |key: &str| session["time"][key].as_u64().unwrap_or(0);
    let (busy, idle) = (millis("busy"), millis("idle"));
    if busy == 0 || idle >= busy {
        return None;
    }
    let elapsed = Duration::from_millis(now().saturating_sub(millis("created")));
    Some(Found {
        session: session["id"].as_str()?.to_string(),
        slug,
        elapsed,
    })
}

/// When a run that has been going for `elapsed` started.
pub fn started(elapsed: Duration) -> Instant {
    Instant::now()
        .checked_sub(elapsed)
        .unwrap_or_else(Instant::now)
}

/// One request to the service, from `project`, so it reads that project's
/// configuration.
fn call(project: &Path, args: &[&str], body: Option<&str>) -> Result<String, String> {
    call_with(project, args, body, CALL)
}

fn call_with(
    project: &Path,
    args: &[&str],
    body: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    let binary = binary().ok_or_else(|| "opencode is not installed".to_string())?;
    request(&binary, project, args, body, timeout)
}

/// Runs `binary` for one request, its reply read from a file: the service's
/// CLI drops the tail of a reply too large for a pipe, at a write boundary,
/// and retrying only gets the same cut text back. A read whose reply is still
/// not valid JSON is tried again, in case the service was busy.
fn request(
    binary: &Path,
    project: &Path,
    args: &[&str],
    body: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    let reads = args.first().is_some_and(|method| *method == "get");
    let attempts = if reads { RETRIES } else { 1 };
    let deadline = Instant::now() + timeout;
    let mut text = String::new();
    for attempt in 1..=attempts {
        let left = deadline.saturating_duration_since(Instant::now());
        let reply = tempfile::NamedTempFile::new()
            .map_err(|err| format!("Couldn't open a reply file: {err}"))?;
        let mut command = std::process::Command::new(binary);
        command.arg("api").args(args).current_dir(project);
        if let Some(body) = body {
            command.args(["-d", body]);
        }
        text = process::run_to_file(
            command,
            "opencode api",
            &Cancel::default(),
            left,
            reply.path(),
        )?
        .into_result("opencode api")?;
        if attempt == attempts || serde_json::from_str::<Value>(&text).is_ok() {
            break;
        }
    }
    Ok(text)
}

fn binary() -> Option<PathBuf> {
    std::env::var_os("IBOARD_OPENCODE")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| super::find_binary("opencode"))
}

fn parse(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|err| format!("opencode api: {err}"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// `timeout` in minutes, for an error.
fn minutes(timeout: Duration) -> String {
    match timeout.as_secs() {
        seconds if seconds < 60 => format!("{seconds}s"),
        seconds => format!("{} min", seconds / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_titled_and_tagged_with_its_issue() {
        let body = json!({
            "title": crate::files::file_name(Path::new("doing/002-build.md")),
            "metadata": { "iboard": { "slug": reviews::slug(Path::new("doing/002-build.md")) } },
        });
        assert_eq!(body["title"], "002-build.md");
        assert_eq!(body["metadata"]["iboard"]["slug"], "build");
    }

    #[test]
    fn a_model_becomes_the_services_own_reference() {
        assert_eq!(
            model_ref("deepseek/deepseek-flash"),
            Some(json!({ "providerID": "deepseek", "id": "deepseek-flash" }))
        );
        assert_eq!(
            model_ref("anthropic/claude-sonnet#thinking")
                .as_ref()
                .unwrap()["id"],
            "claude-sonnet"
        );
        assert_eq!(model_ref(""), None, "the service picks its own");
        assert_eq!(model_ref("no-provider"), None);
    }

    #[test]
    fn a_running_session_is_not_idle_since_its_last_turn() {
        let running = json!({"id": "ses_1", "location": {"directory": "/code/app"},
            "metadata": {"iboard": {"slug": "build"}},
            "time": {"created": 1_000, "busy": 2_000}});
        assert_eq!(
            found(&running, Path::new("/code/app")),
            Some(Found {
                session: "ses_1".into(),
                slug: "build".into(),
                elapsed: Duration::from_millis(now().saturating_sub(1_000)),
            })
        );
        let done = json!({"id": "ses_1", "location": {"directory": "/code/app"},
            "metadata": {"iboard": {"slug": "build"}},
            "time": {"created": 1_000, "busy": 2_000, "idle": 3_000}});
        assert_eq!(found(&done, Path::new("/code/app")), None, "it finished");
    }

    #[test]
    fn only_this_projects_iboard_sessions_are_adopted() {
        let elsewhere = json!({"id": "ses_2", "location": {"directory": "/code/other"},
            "metadata": {"iboard": {"slug": "build"}}, "time": {"busy": 2_000}});
        assert_eq!(found(&elsewhere, Path::new("/code/app")), None);
        let stranger = json!({"id": "ses_3", "location": {"directory": "/code/app"},
            "time": {"busy": 2_000}});
        assert_eq!(found(&stranger, Path::new("/code/app")), None, "not ours");
    }

    #[test]
    fn the_step_and_the_answer_come_from_the_last_messages() {
        let messages = vec![
            json!({"type": "user", "content": [{"type": "text", "text": "hello"}]}),
            json!({"type": "assistant", "content": [
                {"type": "reasoning", "text": "thinking"},
                {"type": "tool", "name": "read",
                 "state": {"input": {"path": "/code/app/src/app.rs"}}},
            ]}),
            json!({"type": "assistant", "content": [{"type": "text", "text": "Done."}]}),
        ];
        let (step, answer) = scan(&messages);
        assert_eq!(step.as_deref(), Some("Reading src/app.rs…"));
        assert_eq!(answer.as_deref(), Some("Done."));
    }

    #[test]
    fn a_step_that_is_not_finished_leaves_no_answer() {
        let messages = vec![json!({"type": "assistant", "content": [
            {"type": "question", "state": {"input": {}}},
        ]})];
        let (step, answer) = scan(&messages);
        assert!(step.is_some(), "asking is a step");
        assert_eq!(answer, None, "nothing is answered yet");
    }

    #[test]
    fn a_handle_remembers_the_session_for_stop() {
        let handle = Handle::default();
        assert_eq!(session_of(&handle), None);
        remember(&handle, "ses_42");
        assert_eq!(session_of(&handle).as_deref(), Some("ses_42"));
    }

    /// The service's CLI drops the tail of a reply too large for a pipe. A
    /// reply written to a file arrives whole.
    #[test]
    fn a_large_reply_is_read_whole_from_a_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("opencode");
        std::fs::write(
            &binary,
            "#!/bin/sh\n\
             if [ -p /dev/stdout ]; then printf '{\"data\":[';\n\
             else printf '{\"data\":[{\"id\":\"ses_1\"}]}';\n\
             fi\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let reply = request(
            &binary,
            dir.path(),
            &["get", "/api/session/ses_1/message"],
            None,
            CALL,
        )
        .unwrap();
        assert_eq!(parse(&reply).unwrap()["data"][0]["id"], "ses_1");
    }

    #[test]
    fn a_form_becomes_a_question_with_its_typed_fields() {
        let value = json!({
            "id": "frm_1", "sessionID": "ses_1", "title": "Which approach?",
            "fields": [
                {"key": "approach", "type": "string", "title": "Approach",
                 "required": true, "options": [
                    {"value": "fast", "label": "Fast", "description": "Ship it"},
                    {"value": "careful", "label": "Careful"}
                 ]},
                {"key": "notes", "type": "string", "placeholder": "Anything else?"},
                {"key": "count", "type": "integer", "default": 3},
                {"key": "deploy", "type": "boolean", "default": true},
                {"key": "areas", "type": "multiselect", "default": ["api"],
                 "options": [{"value": "api", "label": "API"}]},
            ]
        });
        let question = question_from_form(&value).expect("a form");
        assert_eq!(question.session, "ses_1");
        assert_eq!(question.title, "Which approach?");
        let QuestionKind::Form(fields) = question.kind else {
            panic!("not a form");
        };
        assert_eq!(fields.len(), 5);
        assert!(fields[0].required);
        assert!(matches!(
            &fields[0].kind,
            FieldKind::Text { options, .. } if options.len() == 2
        ));
        assert!(matches!(
            &fields[2].kind,
            FieldKind::Number { integer: true, default } if default == "3"
        ));
        assert!(matches!(
            &fields[4].kind,
            FieldKind::Multi { default, .. } if default == &["api"]
        ));
    }

    #[test]
    fn a_permission_request_becomes_a_question() {
        let value = json!({
            "id": "per_1", "sessionID": "ses_1", "action": "bash",
            "resources": ["git push origin main"],
            "message": "Push the branch?"
        });
        let question = question_from_permission(&value).expect("a request");
        assert_eq!(question.title, "Allow bash?");
        let QuestionKind::Permission(permission) = question.kind else {
            panic!("not a permission");
        };
        assert_eq!(permission.resources, ["git push origin main"]);
        assert_eq!(permission.message.as_deref(), Some("Push the branch?"));
        assert_eq!(PermissionReply::Once.key(), "once");
        assert_eq!(PermissionReply::Always.key(), "always");
        assert_eq!(PermissionReply::Reject.key(), "reject");
    }

    #[test]
    fn a_list_names_its_operation_and_the_projects_location() {
        let scratch = tempfile::tempdir().unwrap();
        let project = std::fs::canonicalize(scratch.path()).unwrap();
        let args = list_args(&project, "form.list");
        assert_eq!(args[0], "form.list", "an operation binds --param");
        assert_eq!(args[1], "--param");
        assert_eq!(
            args[2],
            format!("location[directory]={}", project.display())
        );
    }

    #[test]
    fn a_feed_forgets_a_question_once_answered() {
        let question = Question {
            session: "ses_1".into(),
            id: "frm_1".into(),
            title: "Which approach?".into(),
            kind: QuestionKind::Form(Vec::new()),
        };
        let feed = Feed::new(vec![question.clone()]);
        assert_eq!(feed.questions(), [question]);
        feed.remove("ses_1", "frm_1");
        assert!(feed.questions().is_empty());
    }
}
