//! What the build agent did on an issue, kept in `issues/.builds/<slug>.log`:
//! one readable line per step or message, each run under its own heading.
//! The slug drops the `001-` priority, so the log follows the issue from
//! Doing to Done.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{agent, format, reviews};

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

/// The line a run ends on, with how long a finished one `took`.
pub fn outcome(result: &Result<String, String>, took: Duration) -> String {
    match result {
        Ok(_) => format!("{FINISHED}{}", format::stopwatch(took)),
        Err(err) => format!("✗ {err}"),
    }
}

const FINISHED: &str = "✓ Finished in ";

/// How long the agent took on the issue: its last run's, when it finished.
pub fn took(text: &str) -> Option<Duration> {
    let last = text
        .lines()
        .rev()
        .find(|line| line.starts_with("✓ ") || line.starts_with("✗ ") || line.starts_with("── "))?;
    let (minutes, seconds) = last.strip_prefix(FINISHED)?.trim().split_once(':')?;
    let secs = minutes.parse::<u64>().ok()? * 60 + seconds.parse::<u64>().ok()?;
    Some(Duration::from_secs(secs))
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
    format!("→ {}", step.lines().next().unwrap_or_default().trim_end())
}

/// One piece of a log, as the agent log panel shows it.
#[derive(Debug, PartialEq, Eq)]
pub enum Entry<'a> {
    /// A run's heading: when, and which agent.
    Run(&'a str),
    /// A tool the agent used.
    Step(&'a str),
    /// How a run ended, and whether it went well.
    Outcome { ok: bool, text: &'a str },
    /// What the agent said, in markdown, over one or more lines.
    Message(String),
}

/// The log's `text` cut into its runs, steps, outcomes and messages; the
/// lines between two marked ones make one message.
pub fn parse(text: &str) -> Vec<Entry<'_>> {
    let mut entries = Vec::new();
    let mut message: Vec<&str> = Vec::new();
    let flush = |message: &mut Vec<&str>, entries: &mut Vec<Entry>| {
        let text = message.join("\n");
        if !text.trim().is_empty() {
            entries.push(Entry::Message(text.trim().to_string()));
        }
        message.clear();
    };
    for line in text.lines() {
        let marked = if let Some(rest) = line.strip_prefix("── ") {
            Some(Entry::Run(rest.trim()))
        } else if let Some(rest) = line.strip_prefix("→ ") {
            Some(Entry::Step(rest.trim()))
        } else if let Some(rest) = line.strip_prefix("✓ ") {
            Some(Entry::Outcome {
                ok: true,
                text: rest.trim(),
            })
        } else {
            line.strip_prefix("✗ ").map(|rest| Entry::Outcome {
                ok: false,
                text: rest.trim(),
            })
        };
        match marked {
            Some(entry) => {
                flush(&mut message, &mut entries);
                entries.push(entry);
            }
            None => message.push(line),
        }
    }
    flush(&mut message, &mut entries);
    entries
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
    fn a_log_parses_into_runs_steps_messages_and_outcomes() {
        let log = "── 2026-10-17 · opencode\nLet me **look**.\n\n- one\n- two\n→ Reading app.rs…\nDone.\n✓ Finished\n── later · claude\n✗ stopped\n";
        assert_eq!(
            parse(log),
            [
                Entry::Run("2026-10-17 · opencode"),
                Entry::Message("Let me **look**.\n\n- one\n- two".into()),
                Entry::Step("Reading app.rs…"),
                Entry::Message("Done.".into()),
                Entry::Outcome {
                    ok: true,
                    text: "Finished"
                },
                Entry::Run("later · claude"),
                Entry::Outcome {
                    ok: false,
                    text: "stopped"
                },
            ]
        );
        assert!(parse("").is_empty());
    }

    #[test]
    fn a_step_takes_one_line() {
        assert_eq!(step("Using shell a\nb".into()), "→ Using shell a");
    }

    #[test]
    fn the_log_follows_the_issue_and_keeps_every_run() {
        let dir = tempfile::tempdir().unwrap();
        let log = path(dir.path(), Path::new("doing/003-fix.md"));
        assert_eq!(log, path(dir.path(), Path::new("done/001-fix.md")));
        assert_eq!(load(&log), None);
        append(&log, "one").unwrap();
        let took = Duration::from_secs(125);
        append(&log, &outcome(&Err("stopped".into()), took)).unwrap();
        append(&log, &outcome(&Ok(String::new()), took)).unwrap();
        assert_eq!(load(&log).unwrap(), "one\n✗ stopped\n✓ Finished in 02:05\n");
    }

    #[test]
    fn the_time_taken_is_the_last_runs_when_it_finished() {
        let finished = "── a · opencode\n✓ Finished in 02:05\n";
        assert_eq!(took(finished), Some(Duration::from_secs(125)));
        assert_eq!(
            took(&format!("{finished}── b · opencode\n→ Reading…\n")),
            None
        );
        assert_eq!(
            took(&format!("{finished}── b · opencode\n✗ stopped\n")),
            None
        );
        assert_eq!(took("── a · opencode\n✓ Finished\n"), None);
        assert_eq!(took(""), None);
    }
}
