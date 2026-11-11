//! Waiting for a background run to finish, reading its steps and answer as
//! they come while its transcript goes to the build log.

use std::path::PathBuf;
use std::sync::PoisonError;
use std::time::{Duration, Instant};

use super::poller::project_poller;
use super::service::{Finished, Message, POLL, Part, Service, backoff, now};
use super::transcript::Transcript;
use super::{Progress, describe_tool};
use crate::domains::agent::artifacts::build_log;
use crate::platform::job::{Cancel, Job};

/// How long a session may go without writing anything before its run is
/// given up as dead.
const STALE: Duration = Duration::from_secs(30 * 60);

/// The last step and the last answer in `messages`.
fn scan(messages: &[Message]) -> (Option<String>, Option<String>) {
    let mut step = None;
    let mut answer = None;
    for message in messages {
        let mut text = String::new();
        for part in &message.parts {
            match part {
                Part::Tool { name, input, .. } => step = describe_tool(name, input),
                Part::Text(said) => text.push_str(said),
                Part::Other => {}
            }
        }
        if !text.trim().is_empty() {
            answer = Some(text);
        }
    }
    (step, answer)
}

/// Waits for `session` to finish, reporting its steps and writing its
/// `transcript` as they come. The job that runs this is dropped when the
/// window closes; the service keeps working, so stopping only stops the
/// watching.
pub(super) fn await_answer(
    service: &'static dyn Service,
    project: &std::path::Path,
    session: &str,
    cancel: &Cancel,
    timeout: Option<Duration>,
    progress: Option<&Progress>,
    mut transcript: Transcript,
) -> Result<Finished, String> {
    let started = Instant::now();
    let poller = project_poller(service, project);
    let _subscription = poller.subscribe(false);
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
            service.interrupt(project, session);
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
            let answer = service.read(project, session).map(|messages| {
                transcript.write(&messages);
                scan(&messages).1
            });
            match answer {
                Ok(Some(answer)) => return Ok(Finished { answer, usage }),
                Ok(None) => {
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
            service.interrupt(project, session);
            return Err("the agent stopped writing".to_string());
        }
        if status.updated != seen || transcript.waiting() {
            seen = status.updated;
            if let Ok(messages) = service.read(project, session) {
                transcript.write(&messages);
                if let Some(step) = scan(&messages).0.filter(|step| *step != last)
                    && let Some(progress) = progress
                {
                    last.clone_from(&step);
                    *progress.lock().unwrap_or_else(PoisonError::into_inner) = Some(step);
                }
            }
        }
        rest(cancel, POLL);
    }
}

/// Watches `session` on a thread of its own, for a run adopted after a
/// restart: nothing to start, only an answer to wait for.
pub fn watch(
    service: &'static dyn Service,
    project: PathBuf,
    session: String,
    log: PathBuf,
) -> Job<Result<String, String>> {
    Job::spawn(move |cancel| {
        let transcript = Transcript::resumed(&log);
        let (result, usage) =
            match await_answer(service, &project, &session, cancel, None, None, transcript) {
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
    use super::*;

    fn text(id: &str, said: &str) -> Message {
        Message {
            id: id.to_string(),
            closed: true,
            parts: vec![Part::Text(said.to_string())],
        }
    }

    fn tool(id: &str, name: &str, input: serde_json::Value) -> Message {
        Message {
            id: id.to_string(),
            closed: true,
            parts: vec![Part::Tool {
                name: name.to_string(),
                input,
                streaming: false,
            }],
        }
    }

    #[test]
    fn the_step_and_the_answer_come_from_the_last_messages() {
        let messages = vec![
            tool(
                "msg_1",
                "read",
                serde_json::json!({"path": "/code/app/src/app.rs"}),
            ),
            text("msg_2", "Done."),
        ];
        let (step, answer) = scan(&messages);
        assert_eq!(step.as_deref(), Some("Reading src/app.rs…"));
        assert_eq!(answer.as_deref(), Some("Done."));
    }

    #[test]
    fn a_step_that_is_not_finished_leaves_no_answer() {
        let messages = vec![Message {
            id: "msg_1".into(),
            closed: false,
            parts: vec![Part::Tool {
                name: "question".into(),
                input: serde_json::json!({}),
                streaming: false,
            }],
        }];
        let (step, answer) = scan(&messages);
        assert!(step.is_some(), "asking is a step");
        assert_eq!(answer, None, "nothing is answered yet");
    }
}
