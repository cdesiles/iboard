//! A service session's transcript on its way into the build log: the agent's
//! messages and every tool call, each written once and in the order they came.

use std::fs::File;
use std::path::Path;

use super::describe_tool;
use super::service::{Message, Part};
use crate::domains::agent::artifacts::build_log;

/// The build log of a run, and how far into its session's transcript it has
/// been written.
#[derive(Debug, Default)]
pub(super) struct Transcript {
    log: Option<File>,
    /// The last assistant message read, and how many of its parts are logged.
    message: String,
    parts: usize,
    /// What the session said before this run was adopted is not written again.
    resumed: bool,
    waiting: bool,
}

impl Transcript {
    /// The transcript of a run just started: all of it goes to `log`.
    pub(super) fn new(log: &Path) -> Self {
        Self {
            log: build_log::open(log).ok(),
            ..Self::default()
        }
    }

    /// The transcript of a run adopted after a restart: `log` already holds
    /// what the session said so far.
    pub(super) fn resumed(log: &Path) -> Self {
        Self {
            resumed: true,
            ..Self::new(log)
        }
    }

    /// Whether a part was still being written at the last read, so the next
    /// one should not wait for the session to change.
    pub(super) fn waiting(&self) -> bool {
        self.waiting
    }

    /// Adds what `messages` say that the log does not hold yet.
    pub(super) fn write(&mut self, messages: &[Message]) {
        let lines = self.unread(messages);
        if let Some(log) = self.log.as_mut() {
            for line in lines {
                let _ = build_log::write_line(log, &line);
            }
        }
    }

    /// The log lines of the parts not written yet, oldest first. It stops at
    /// a part still being written, so nothing is logged out of order.
    fn unread(&mut self, messages: &[Message]) -> Vec<String> {
        let known = messages
            .iter()
            .position(|message| message.id == self.message);
        let mut lines = Vec::new();
        self.waiting = false;
        for (at, message) in messages.iter().enumerate().skip(known.unwrap_or(0)) {
            let mut done = if known == Some(at) { self.parts } else { 0 };
            while let Some(part) = message.parts.get(done) {
                match line(part, message.closed || done + 1 < message.parts.len()) {
                    Logged::Open => break,
                    Logged::Unlogged => {}
                    Logged::Line(line) => lines.push(line),
                }
                done += 1;
            }
            self.message.clone_from(&message.id);
            self.parts = done;
            if done < message.parts.len() {
                self.waiting = true;
                break;
            }
        }
        if std::mem::take(&mut self.resumed) {
            lines.clear();
        }
        lines
    }
}

/// What a part of a message adds to the log.
enum Logged {
    /// A tool's input or a text still streams: read it again later.
    Open,
    /// A part the log leaves out.
    Unlogged,
    Line(String),
}

/// What `part` adds to the log; `closed` when nothing more can be added to it.
fn line(part: &Part, closed: bool) -> Logged {
    match part {
        Part::Tool {
            streaming: true, ..
        } => Logged::Open,
        Part::Tool { name, input, .. } => describe_tool(name, input)
            .map_or(Logged::Unlogged, |step| Logged::Line(build_log::step(step))),
        Part::Text(_) if !closed => Logged::Open,
        Part::Text(text) => match text.trim() {
            "" => Logged::Unlogged,
            text => Logged::Line(text.to_string()),
        },
        Part::Other => Logged::Unlogged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &str) -> Part {
        Part::Tool {
            name: "read".into(),
            input: serde_json::json!({"path": path}),
            streaming: false,
        }
    }

    fn message(id: &str, closed: bool, parts: Vec<Part>) -> Message {
        Message {
            id: id.to_string(),
            closed,
            parts,
        }
    }

    #[test]
    fn messages_and_tools_are_logged_once_in_order() {
        let mut transcript = Transcript::default();
        let first = message(
            "msg_1",
            true,
            vec![
                Part::Other,
                Part::Text(" Let me look.\n".into()),
                read("/code/app/src/a.rs"),
                read("/code/app/src/b.rs"),
            ],
        );
        let messages = vec![first];
        assert_eq!(
            transcript.unread(&messages),
            ["Let me look.", "→ Reading src/a.rs…", "→ Reading src/b.rs…"]
        );
        assert!(transcript.unread(&messages).is_empty(), "nothing is new");

        let second = message("msg_2", true, vec![Part::Text("Done.".into())]);
        assert_eq!(transcript.unread(&[messages[0].clone(), second]), ["Done."]);
        assert!(!transcript.waiting());
    }

    #[test]
    fn a_tool_waits_for_its_whole_input() {
        let mut transcript = Transcript::default();
        let streaming = message(
            "msg_1",
            false,
            vec![
                read("/code/app/src/a.rs"),
                Part::Tool {
                    name: "bash".into(),
                    input: serde_json::json!("{\"comm"),
                    streaming: true,
                },
            ],
        );
        assert_eq!(transcript.unread(&[streaming]), ["→ Reading src/a.rs…"]);
        assert!(transcript.waiting(), "the next read does not wait");

        let running = message(
            "msg_1",
            true,
            vec![
                read("/code/app/src/a.rs"),
                Part::Tool {
                    name: "bash".into(),
                    input: serde_json::json!({"command": "make check"}),
                    streaming: false,
                },
            ],
        );
        assert_eq!(transcript.unread(&[running]), ["→ Running make check…"]);
        assert!(!transcript.waiting());
    }

    #[test]
    fn a_text_waits_for_its_message_to_stream() {
        let mut transcript = Transcript::default();
        let open = message("msg_1", false, vec![Part::Text("Let me".into())]);
        assert!(transcript.unread(&[open]).is_empty());
        assert!(transcript.waiting());

        let followed = message(
            "msg_1",
            false,
            vec![
                Part::Text("Let me look.".into()),
                read("/code/app/src/a.rs"),
            ],
        );
        assert_eq!(
            transcript.unread(&[followed]),
            ["Let me look.", "→ Reading src/a.rs…"],
            "a part after it closes the text"
        );
    }

    #[test]
    fn an_adopted_run_continues_from_where_the_session_stands() {
        let mut transcript = Transcript {
            resumed: true,
            ..Transcript::default()
        };
        let old = message(
            "msg_1",
            true,
            vec![Part::Text("Before the restart.".into())],
        );
        assert!(transcript.unread(std::slice::from_ref(&old)).is_empty());

        let new = message("msg_2", true, vec![Part::Text("After it.".into())]);
        assert_eq!(transcript.unread(&[old, new]), ["After it."]);
    }

    #[test]
    fn a_page_past_the_last_message_read_is_logged_whole() {
        let mut transcript = Transcript::default();
        let said = |id: &str, text: &str| message(id, true, vec![Part::Text(text.to_string())]);
        assert_eq!(transcript.unread(&[said("msg_1", "One.")]), ["One."]);
        let page = [said("msg_8", "Eight."), said("msg_9", "Nine.")];
        assert_eq!(transcript.unread(&page), ["Eight.", "Nine."]);
    }

    #[test]
    fn the_lines_reach_the_log() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("fix.log");
        let mut transcript = Transcript::new(&log);
        transcript.write(&[message(
            "msg_1",
            true,
            vec![Part::Text("Reading it.".into()), read("/p/src/a.rs")],
        )]);
        assert_eq!(
            build_log::load(&log).as_deref(),
            Some("Reading it.\n→ Reading src/a.rs…\n")
        );
    }
}
