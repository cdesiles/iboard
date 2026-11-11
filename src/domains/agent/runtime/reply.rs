//! Asking the service and reading what it answers: one request, its reply read
//! from a file, the last step and answer of a session, and the wait for a run
//! to finish.

use std::path::{Path, PathBuf};
use std::sync::PoisonError;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::poller::project_poller;
use super::service::{Finished, POLL, backoff, interrupt};
use super::{Progress, describe_tool};
use crate::domains::agent::artifacts::build_log;
use crate::platform::job::{Cancel, Job};
use crate::platform::process;

/// How long one call to the service may take.
const CALL: Duration = Duration::from_secs(20);

/// How many of a session's newest messages its step and answer are read from.
const PAGE: usize = 20;

/// How many times a read is retried when its reply comes back cut short.
const RETRIES: usize = 3;

/// How long a session may go without writing anything before its run is
/// given up as dead.
const STALE: Duration = Duration::from_secs(30 * 60);

/// One request to the service, from `project`, so it reads that project's
/// configuration.
pub(super) fn call(project: &Path, args: &[&str], body: Option<&str>) -> Result<String, String> {
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

/// The `opencode` to call: the tests' fake, or the real one outside them.
fn binary() -> Option<PathBuf> {
    std::env::var_os("IBOARD_OPENCODE")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let path = super::Tool::OpenCode.program();
            path.is_file().then_some(path)
        })
}

pub(super) fn parse(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|err| format!("opencode api: {err}"))
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
pub(super) fn await_answer(
    project: &Path,
    session: &str,
    cancel: &Cancel,
    timeout: Option<Duration>,
    progress: Option<&Progress>,
    log: Option<&Path>,
) -> Result<Finished, String> {
    let started = Instant::now();
    let poller = project_poller(project);
    let _subscription = poller.subscribe(false);
    let mut log = log.and_then(|path| build_log::open(path).ok());
    let mut last = String::new();
    let mut seen = 0;
    let mut misses = 0;
    let mut quiet = 0;
    let mut observed = None;
    loop {
        if cancel.is_cancelled() {
            return Err("stopped watching the agent".to_string());
        }
        if let Some(timeout) = timeout.filter(|limit| started.elapsed() >= *limit) {
            interrupt(project, session);
            return Err(format!(
                "the agent gave no answer within {}",
                minutes(timeout)
            ));
        }
        let (polled, at) = match poller.status(session, started) {
            Some(pair) => {
                misses = 0;
                pair
            }
            None => {
                misses += 1;
                quiet = 0;
                observed = None;
                rest(cancel, backoff(misses));
                continue;
            }
        };
        let (status, usage) = (polled.status, polled.usage);
        if !status.running {
            match read(project, session) {
                Ok((_, Some(answer))) => return Ok(Finished { answer, usage }),
                Ok((_, None)) => {
                    if observed == Some(at) {
                        rest(cancel, POLL);
                        continue;
                    }
                    observed = Some(at);
                    quiet += 1;
                    if quiet >= 2 {
                        return Err("the agent returned no answer".to_string());
                    }
                    rest(cancel, POLL);
                    continue;
                }
                Err(_) => {
                    misses += 1;
                    quiet = 0;
                    observed = None;
                    rest(cancel, backoff(misses));
                    continue;
                }
            }
        }
        quiet = 0;
        observed = None;
        if now().saturating_sub(status.updated) > STALE.as_millis() as u64 {
            interrupt(project, session);
            return Err("the agent stopped writing".to_string());
        }
        if status.updated != seen {
            seen = status.updated;
            if let Ok((step, _)) = read(project, session)
                && let Some(step) = step.filter(|step| *step != last)
            {
                last = step.clone();
                if let Some(progress) = progress {
                    *progress.lock().unwrap_or_else(PoisonError::into_inner) = Some(step.clone());
                }
                if let Some(log) = log.as_mut() {
                    let _ = build_log::write_line(log, &build_log::step(step));
                }
            }
        }
        rest(cancel, POLL);
    }
}

/// Watches `session` on a thread of its own, for a run adopted after a
/// restart: nothing to start, only an answer to wait for.
pub fn watch(project: PathBuf, session: String, log: PathBuf) -> Job<Result<String, String>> {
    Job::spawn(move |cancel| {
        let (result, usage) = match await_answer(&project, &session, cancel, None, None, Some(&log))
        {
            Ok(finished) => (Ok(finished.answer), finished.usage),
            Err(error) => (Err(error), None),
        };
        let _ = build_log::append(
            &log,
            &build_log::outcome(&result, Duration::ZERO, usage.as_ref()),
        );
        result
    })
}

pub(super) fn now() -> u64 {
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

/// Sleeps for `duration`, waking to notice a cancel.
fn rest(cancel: &Cancel, duration: Duration) {
    let until = Instant::now() + duration;
    while !cancel.is_cancelled() {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(POLL));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
    fn tests_use_the_fake_opencode() {
        let path = binary().expect("a fake opencode");
        assert!(
            path.starts_with(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fakes")),
            "{path:?}"
        );
    }

    /// The only test that may reach the real service. `make third-party` runs
    /// it, with `IBOARD_OPENCODE` pointing at the real opencode.
    #[test]
    #[ignore = "third-party: run `make third-party` with the real opencode service"]
    fn third_party_the_real_service_answers() {
        let real = std::env::var_os("IBOARD_OPENCODE")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .expect("run `make third-party`, not `cargo test`");
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        let reply = request(&real, project, &["get", "/api/session"], None, CALL)
            .expect("the real opencode api answers");
        let value = parse(&reply).expect("its reply is JSON");
        assert!(value["data"].is_array(), "{value}");
    }
}
