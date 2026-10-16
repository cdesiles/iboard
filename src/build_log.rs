//! What the build agent did on an issue, kept in `issues/.builds/<slug>.log`:
//! one readable line per step or message, each run under its own heading.
//! The slug drops the `001-` priority, so the log follows the issue from
//! Doing to Done.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{agent, reviews};

/// The folder under the issues root; discovery skips dot-folders.
pub const DIR: &str = ".builds";

/// Where the log of `issue` goes, under the issues `root`.
pub fn path(root: &Path, issue: &Path) -> PathBuf {
    root.join(DIR).join(format!("{}.log", reviews::slug(issue)))
}

/// The log at `path`; `None` when no agent ever built the issue.
pub fn load(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// The log at `path`, open for appending, created with its folder.
pub fn open(path: &Path) -> Result<File, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Couldn't create {}: {err}", dir.display()))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("Couldn't open {}: {err}", path.display()))
}

/// Adds `text` as its own lines to the log at `path`.
pub fn append(path: &Path, text: &str) -> Result<(), String> {
    writeln!(open(path)?, "{text}")
        .map_err(|err| format!("Couldn't write {}: {err}", path.display()))
}

/// The heading a new run starts under: when, and which agent.
pub fn heading(at: &str, agent: &str) -> String {
    format!("── {at} · {agent}")
}

/// The line a run ends on.
pub fn outcome(result: &Result<String, String>) -> String {
    match result {
        Ok(_) => "✓ Finished".to_string(),
        Err(err) => format!("✗ {err}"),
    }
}

/// What one line of the agent's output adds to the log: its messages and
/// steps from a stream of JSON events, a plain line as it is.
pub fn entries(line: &str) -> Vec<String> {
    let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
        return match line.trim_end() {
            text if text.trim().is_empty() => Vec::new(),
            text => vec![text.to_string()],
        };
    };
    match event["type"].as_str() {
        Some("text") => message(&event["part"]["text"]).into_iter().collect(),
        Some("tool_use") => agent::step_of(line).map(step).into_iter().collect(),
        Some("assistant") => event["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|part| match part["type"].as_str()? {
                "text" => message(&part["text"]),
                "tool_use" => {
                    agent::describe_tool(part["name"].as_str()?, &part["input"]).map(step)
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn message(text: &serde_json::Value) -> Option<String> {
    let text = text.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn step(step: String) -> String {
    format!("→ {step}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_become_messages_and_steps() {
        let opencode = [
            r#"{"type":"step_start","part":{"type":"step-start"}}"#,
            r#"{"type":"text","part":{"type":"text","text":"Let me look."}}"#,
            r#"{"type":"tool_use","part":{"tool":"read","state":{"input":{"path":"/p/src/app.rs"}}}}"#,
        ];
        let claude = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Now grep."},{"type":"tool_use","name":"Grep","input":{"pattern":"fn x"}}]}}"#;
        let log: Vec<String> = opencode
            .iter()
            .chain([&claude])
            .chain([&r#"{"type":"result","result":"done"}"#])
            .flat_map(|line| entries(line))
            .collect();
        assert_eq!(
            log,
            [
                "Let me look.",
                "→ Reading src/app.rs…",
                "Now grep.",
                "→ Searching fn x…"
            ]
        );
        assert_eq!(entries("plain output  "), ["plain output"]);
        assert!(entries("   ").is_empty());
    }

    #[test]
    fn the_log_follows_the_issue_and_keeps_every_run() {
        let dir = tempfile::tempdir().unwrap();
        let log = path(dir.path(), Path::new("doing/003-fix.md"));
        assert_eq!(log, path(dir.path(), Path::new("done/001-fix.md")));
        assert_eq!(load(&log), None);
        append(&log, "one").unwrap();
        append(&log, &outcome(&Err("stopped".into()))).unwrap();
        append(&log, &outcome(&Ok(String::new()))).unwrap();
        assert_eq!(load(&log).unwrap(), "one\n✗ stopped\n✓ Finished\n");
    }
}
