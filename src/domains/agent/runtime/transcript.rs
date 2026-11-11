//! A service session's transcript on its way into the build log: the agent's
//! messages and every tool call, each written once and in the order they came.

use std::fs::File;
use std::path::Path;

use serde_json::Value;

use super::describe_tool;
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
    pub(super) fn write(&mut self, messages: &[Value]) {
        let lines = self.unread(messages);
        if let Some(log) = self.log.as_mut() {
            for line in lines {
                let _ = build_log::write_line(log, &line);
            }
        }
    }

    /// The log lines of the parts not written yet, oldest first. It stops at
    /// a part still being written, so nothing is logged out of order.
    fn unread(&mut self, messages: &[Value]) -> Vec<String> {
        let assistant: Vec<&Value> = messages
            .iter()
            .filter(|message| message["type"] == "assistant")
            .collect();
        let known = assistant
            .iter()
            .position(|message| message["id"].as_str() == Some(self.message.as_str()));
        let mut lines = Vec::new();
        self.waiting = false;
        for (at, message) in assistant.iter().enumerate().skip(known.unwrap_or(0)) {
            let parts = message["content"].as_array().map_or(&[][..], Vec::as_slice);
            let streamed = ["streamed", "completed"]
                .iter()
                .any(|key| !message["time"][*key].is_null());
            let mut done = if known == Some(at) { self.parts } else { 0 };
            while let Some(part) = parts.get(done) {
                match line(part, streamed || done + 1 < parts.len()) {
                    Part::Open => break,
                    Part::Unlogged => {}
                    Part::Line(line) => lines.push(line),
                }
                done += 1;
            }
            self.message = message["id"].as_str().unwrap_or_default().to_string();
            self.parts = done;
            if done < parts.len() {
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
enum Part {
    /// A tool's input or a text still streams: read it again later.
    Open,
    /// A part the log leaves out.
    Unlogged,
    Line(String),
}

/// What `part` adds to the log; `closed` when nothing more can be added to it.
fn line(part: &Value, closed: bool) -> Part {
    match part["type"].as_str() {
        Some("tool" | "question") if part["state"]["status"] == "streaming" => Part::Open,
        Some("tool" | "question") => {
            let tool = part["name"].as_str().unwrap_or("tool");
            describe_tool(tool, &part["state"]["input"])
                .map_or(Part::Unlogged, |step| Part::Line(build_log::step(step)))
        }
        Some("text") if !closed => Part::Open,
        Some("text") => match part["text"].as_str().unwrap_or_default().trim() {
            "" => Part::Unlogged,
            text => Part::Line(text.to_string()),
        },
        _ => Part::Unlogged,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn read(path: &str) -> Value {
        json!({"type": "tool", "id": path, "name": "read",
               "state": {"status": "completed", "input": {"path": path}}})
    }

    #[test]
    fn messages_and_tools_are_logged_once_in_order() {
        let mut transcript = Transcript::default();
        let first = json!({"id": "msg_1", "type": "assistant", "time": {"streamed": 2},
        "content": [
            {"type": "reasoning", "text": "thinking"},
            {"type": "text", "text": " Let me look.\n"},
            read("/code/app/src/a.rs"),
            read("/code/app/src/b.rs"),
        ]});
        let messages = vec![
            json!({"id": "msg_0", "type": "user", "text": "Build it"}),
            first.clone(),
        ];
        assert_eq!(
            transcript.unread(&messages),
            ["Let me look.", "→ Reading src/a.rs…", "→ Reading src/b.rs…"]
        );
        assert!(transcript.unread(&messages).is_empty(), "nothing is new");

        let second = json!({"id": "msg_2", "type": "assistant", "time": {"completed": 9},
            "content": [{"type": "text", "text": "Done."}]});
        assert_eq!(transcript.unread(&[first, second]), ["Done."]);
        assert!(!transcript.waiting());
    }

    #[test]
    fn a_tool_waits_for_its_whole_input() {
        let mut transcript = Transcript::default();
        let streaming = json!({"id": "msg_1", "type": "assistant", "time": {"created": 1},
        "content": [
            read("/code/app/src/a.rs"),
            {"type": "tool", "id": "call_2", "name": "bash",
             "state": {"status": "streaming", "input": "{\"comm"}},
        ]});
        assert_eq!(transcript.unread(&[streaming]), ["→ Reading src/a.rs…"]);
        assert!(transcript.waiting(), "the next read does not wait");

        let running = json!({"id": "msg_1", "type": "assistant", "time": {"streamed": 2},
        "content": [
            read("/code/app/src/a.rs"),
            {"type": "tool", "id": "call_2", "name": "bash",
             "state": {"status": "running", "input": {"command": "make check"}}},
        ]});
        assert_eq!(transcript.unread(&[running]), ["→ Running make check…"]);
        assert!(!transcript.waiting());
    }

    #[test]
    fn a_text_waits_for_its_message_to_stream() {
        let mut transcript = Transcript::default();
        let open = json!({"id": "msg_1", "type": "assistant", "time": {"created": 1},
            "content": [{"type": "text", "text": "Let me"}]});
        assert!(transcript.unread(&[open]).is_empty());
        assert!(transcript.waiting());

        let followed = json!({"id": "msg_1", "type": "assistant", "time": {"created": 1},
            "content": [{"type": "text", "text": "Let me look."}, read("/code/app/src/a.rs")]});
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
        let old = json!({"id": "msg_1", "type": "assistant", "time": {"completed": 2},
            "content": [{"type": "text", "text": "Before the restart."}]});
        assert!(transcript.unread(std::slice::from_ref(&old)).is_empty());

        let new = json!({"id": "msg_2", "type": "assistant", "time": {"completed": 4},
            "content": [{"type": "text", "text": "After it."}]});
        assert_eq!(transcript.unread(&[old, new]), ["After it."]);
    }

    #[test]
    fn a_page_past_the_last_message_read_is_logged_whole() {
        let mut transcript = Transcript::default();
        let message = |id: &str, text: &str| {
            json!({"id": id, "type": "assistant", "time": {"completed": 2},
                   "content": [{"type": "text", "text": text}]})
        };
        assert_eq!(transcript.unread(&[message("msg_1", "One.")]), ["One."]);
        let page = [message("msg_8", "Eight."), message("msg_9", "Nine.")];
        assert_eq!(transcript.unread(&page), ["Eight.", "Nine."]);
    }

    #[test]
    fn the_lines_reach_the_log() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("fix.log");
        let mut transcript = Transcript::new(&log);
        transcript.write(&[
            json!({"id": "msg_1", "type": "assistant", "time": {"completed": 2},
                   "content": [{"type": "text", "text": "Reading it."}, read("/p/src/a.rs")]}),
        ]);
        assert_eq!(
            build_log::load(&log).as_deref(),
            Some("Reading it.\n→ Reading src/a.rs…\n")
        );
    }
}
