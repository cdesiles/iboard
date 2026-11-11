//! What the build agent did on an issue, kept in `issues/.builds/<slug>.log`:
//! one readable line per step or message, each run under its own heading.
//! The slug drops the `001-` priority, so the log follows the issue from
//! Doing to Done.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::domains::agent::artifacts::reviews;
use crate::domains::agent::runtime as agent;
use crate::platform::{files, format};

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

/// The log at `path`, open for appending, created with its folder; a deleted
/// issues folder stays deleted.
pub fn open(path: &Path) -> Result<File, String> {
    if let Some(dir) = path.parent() {
        files::create_last_dir(dir)
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
    write_line(&mut open(path)?, text)
        .map_err(|err| format!("Couldn't write {}: {err}", path.display()))
}

/// Adds `text` and its line break to `log` in one write, so a run started
/// over while the last one ends never cuts into its lines.
pub fn write_line(log: &mut File, text: &str) -> std::io::Result<()> {
    log.write_all(format!("{text}\n").as_bytes())
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

/// The line an author's instruction adds to the log, on one line.
pub fn author(text: &str) -> String {
    format!("▸ {}", text.trim().replace('\n', " "))
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
    /// The file an edit tool wrote.
    Edit(&'a str),
    /// How a run ended, and whether it went well.
    Outcome { ok: bool, text: &'a str },
    /// What the author told the agent, while it worked or after.
    Author(&'a str),
    /// What the agent said, in markdown, over one or more lines.
    Message(String),
}

/// The edit tools an agent logs, whose step is a change to a file rather
/// than something to fold into an activity row.
const EDIT_TOOLS: &[&str] = &[
    "edit",
    "edit_file",
    "write",
    "multiedit",
    "multi_edit",
    "apply_patch",
    "patch",
    "notebookedit",
    "create_file",
    "str_replace_editor",
];

/// The file a step edited, when the step is an edit tool's.
fn edited(step: &str) -> Option<&str> {
    let step = step.strip_suffix('…').unwrap_or(step);
    if let Some(path) = step.strip_prefix("Editing ") {
        return (!path.trim().is_empty()).then_some(path.trim());
    }
    let (tool, path) = step.strip_prefix("Using ")?.split_once(' ')?;
    EDIT_TOOLS
        .contains(&tool.to_ascii_lowercase().as_str())
        .then(|| path.trim())
        .filter(|path| !path.is_empty())
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
            let step = rest.trim();
            Some(edited(step).map_or(Entry::Step(step), Entry::Edit))
        } else if let Some(rest) = line.strip_prefix("✓ ") {
            Some(Entry::Outcome {
                ok: true,
                text: rest.trim(),
            })
        } else if let Some(rest) = line.strip_prefix("▸ ") {
            Some(Entry::Author(rest.trim()))
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

/// The kind of a step, from the verb its tool logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind<'a> {
    Read,
    Search,
    Command,
    Lookup,
    /// A tool the summary names by itself: `fetch`, `edit`, `task`.
    Tool(&'a str),
}

/// The kind of `step`, from its first word.
pub fn kind_of(step: &str) -> Kind<'_> {
    let step = step.strip_suffix('…').unwrap_or(step);
    let mut words = step.split(' ');
    match words.next().unwrap_or_default() {
        "Reading" => Kind::Read,
        "Searching" => Kind::Search,
        "Running" => Kind::Command,
        "Looking" => Kind::Lookup,
        "Editing" => Kind::Tool("edit"),
        "Using" => match words.next().unwrap_or_default() {
            tool if EDIT_TOOLS.contains(&tool.to_ascii_lowercase().as_str()) => Kind::Tool("edit"),
            tool => Kind::Tool(tool),
        },
        _ => Kind::Tool(step),
    }
}

/// A step's verb and what it touched: `Reading src/app.rs`.
pub fn step_parts(step: &str) -> (&str, &str) {
    let step = step.strip_suffix('…').unwrap_or(step).trim();
    for verb in ["Looking up ", "Using "] {
        if let Some(rest) = step.strip_prefix(verb) {
            return (verb.trim_end(), rest);
        }
    }
    step.split_once(' ').unwrap_or((step, ""))
}

/// The steps between two messages in one line: "Read 5 files, searched 2,
/// ran 1 command". Most used first; ties keep the order they appeared in.
pub fn summary(steps: &[&str]) -> String {
    let mut tallies: Vec<(Kind, usize)> = Vec::new();
    for step in steps {
        let kind = kind_of(step);
        match tallies.iter_mut().find(|(seen, _)| *seen == kind) {
            Some((_, count)) => *count += 1,
            None => tallies.push((kind, 1)),
        }
    }
    tallies.sort_by_key(|left| std::cmp::Reverse(left.1));
    let parts: Vec<String> = tallies
        .into_iter()
        .map(|(kind, count)| phrase(kind, count))
        .collect();
    parts.join(", ")
}

fn phrase(kind: Kind<'_>, count: usize) -> String {
    match kind {
        Kind::Read if count == 1 => "Read 1 file".to_string(),
        Kind::Read => format!("Read {count} files"),
        Kind::Search => format!("searched {count}"),
        Kind::Command if count == 1 => "ran 1 command".to_string(),
        Kind::Command => format!("ran {count} commands"),
        Kind::Lookup => format!("looked up {count}"),
        Kind::Tool(tool) => format!(
            "used {tool} {count} time{}",
            if count == 1 { "" } else { "s" }
        ),
    }
}

/// A run heading's when and agent: `2026-10-17 14:02 · opencode`.
pub fn run_parts(run: &str) -> (&str, &str) {
    run.split_once(" · ").unwrap_or((run, ""))
}

/// A run's start time, short: "today 14:02", or "Nov 7 · 14:02".
pub fn when_label(at: &str) -> String {
    let Ok(when) = chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M") else {
        return at.to_string();
    };
    if when.date() == chrono::Local::now().date_naive() {
        format!("today {}", when.format("%H:%M"))
    } else {
        when.format("%b %-d · %H:%M").to_string()
    }
}

/// What the timeline shows: a run's head, then its messages, its steps
/// folded into bursts, its edits, and how it ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Segment<'a> {
    Run(&'a str),
    Message(String),
    Author(&'a str),
    /// The steps between two messages, when there are two or more.
    Activity(Vec<&'a str>),
    /// One step between two messages: no disclosure onto itself.
    Step(&'a str),
    Edit(&'a str),
    Outcome {
        ok: bool,
        text: &'a str,
    },
}

/// The log's entries folded for the timeline: consecutive steps become one
/// [`Segment::Activity`], a lone one stays a [`Segment::Step`], and an edit
/// leaves the burst it sits in.
pub fn segments(entries: Vec<Entry<'_>>) -> Vec<Segment<'_>> {
    fn flush<'a>(burst: &mut Vec<&'a str>, out: &mut Vec<Segment<'a>>) {
        match burst.len() {
            0 => {}
            1 => out.push(Segment::Step(burst[0])),
            _ => out.push(Segment::Activity(std::mem::take(burst))),
        }
        burst.clear();
    }
    let mut out = Vec::new();
    let mut burst = Vec::new();
    for entry in entries {
        match entry {
            Entry::Step(step) => burst.push(step),
            Entry::Edit(edit) => {
                flush(&mut burst, &mut out);
                out.push(Segment::Edit(edit));
            }
            Entry::Message(message) => {
                flush(&mut burst, &mut out);
                out.push(Segment::Message(message));
            }
            Entry::Author(text) => {
                flush(&mut burst, &mut out);
                out.push(Segment::Author(text));
            }
            Entry::Run(run) => {
                flush(&mut burst, &mut out);
                out.push(Segment::Run(run));
            }
            Entry::Outcome { ok, text } => {
                flush(&mut burst, &mut out);
                out.push(Segment::Outcome { ok, text });
            }
        }
    }
    flush(&mut burst, &mut out);
    out
}

/// A run of the timeline: its heading, what happened in it, the files it
/// edited, and how it ended.
#[derive(Debug, PartialEq, Eq)]
pub struct RunLog<'a> {
    pub head: &'a str,
    pub body: Vec<Segment<'a>>,
    pub edits: Vec<&'a str>,
    pub outcome: Option<(bool, &'a str)>,
}

impl<'a> RunLog<'a> {
    fn new(head: &'a str) -> Self {
        Self {
            head,
            body: Vec::new(),
            edits: Vec::new(),
            outcome: None,
        }
    }
}

/// The log's segments grouped into runs. Content before the first heading
/// makes a run of its own, so an old log still reads.
pub fn runs(segments: Vec<Segment<'_>>) -> Vec<RunLog<'_>> {
    fn current<'b, 'a>(runs: &'b mut Vec<RunLog<'a>>) -> &'b mut RunLog<'a> {
        if runs.is_empty() {
            runs.push(RunLog::new(""));
        }
        let last = runs.len() - 1;
        &mut runs[last]
    }
    let mut runs: Vec<RunLog> = Vec::new();
    for segment in segments {
        match segment {
            Segment::Run(head) => runs.push(RunLog::new(head)),
            Segment::Outcome { ok, text } => current(&mut runs).outcome = Some((ok, text)),
            Segment::Edit(edit) => current(&mut runs).edits.push(edit),
            segment => current(&mut runs).body.push(segment),
        }
    }
    runs
}

/// The log's `text` as runs of messages and folded activity.
pub fn timeline(text: &str) -> Vec<RunLog<'_>> {
    runs(segments(parse(text)))
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
        let log = "── 2026-10-17 · opencode\nLet me **look**.\n\n- one\n- two\n→ Reading app.rs…\n→ Using edit src/app.rs…\nDone.\n▸ go on\n✓ Finished\n── later · claude\n✗ stopped\n";
        assert_eq!(
            parse(log),
            [
                Entry::Run("2026-10-17 · opencode"),
                Entry::Message("Let me **look**.\n\n- one\n- two".into()),
                Entry::Step("Reading app.rs…"),
                Entry::Edit("src/app.rs"),
                Entry::Message("Done.".into()),
                Entry::Author("go on"),
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
    fn a_burst_folds_but_a_lone_step_does_not() {
        let log = "→ Reading a.rs…\n→ Searching fn x…\n→ Running make…\nhi\n→ Reading b.rs…\n";
        let segments = segments(parse(log));
        assert_eq!(
            segments,
            [
                Segment::Activity(vec!["Reading a.rs…", "Searching fn x…", "Running make…"]),
                Segment::Message("hi".into()),
                Segment::Step("Reading b.rs…"),
            ]
        );
        assert_eq!(
            summary(&["Reading a.rs…", "Searching fn x…", "Running make…"]),
            "Read 1 file, searched 1, ran 1 command"
        );
    }

    #[test]
    fn an_edit_leaves_the_burst_and_lands_in_its_run() {
        let log = "── t · opencode\n→ Reading a.rs…\n→ Reading b.rs…\n→ Using edit a.rs…\n→ Reading c.rs…\n✓ Finished\n";
        let runs = timeline(log);
        assert_eq!(
            runs[0].body,
            [
                Segment::Activity(vec!["Reading a.rs…", "Reading b.rs…"]),
                Segment::Step("Reading c.rs…"),
            ],
            "the edit does not join either burst"
        );
        assert_eq!(runs[0].edits, ["a.rs"]);
        assert_eq!(runs[0].outcome, Some((true, "Finished")));
    }

    #[test]
    fn activity_counts_pluralise_and_sort_by_use() {
        let steps = [
            "Reading a.rs…",
            "Searching x…",
            "Searching y…",
            "Reading b.rs…",
            "Reading c.rs…",
        ];
        assert_eq!(summary(&steps), "Read 3 files, searched 2");
        assert_eq!(summary(&["Running make…"]), "ran 1 command");
        assert_eq!(summary(&["Using fetch https://x…"]), "used fetch 1 time");
    }

    #[test]
    fn runs_split_the_timeline() {
        let log = "── 2026-11-07 14:02 · opencode\nhi\n✓ Finished\n── 2026-11-08 09:00 · claude\nbye\n✗ stopped\n";
        let runs = timeline(log);
        assert_eq!(runs.len(), 2);
        assert_eq!(run_parts(runs[1].head), ("2026-11-08 09:00", "claude"));
        assert_eq!(runs[0].body, [Segment::Message("hi".into())]);
        assert_eq!(runs[1].outcome, Some((false, "stopped")));
        assert_eq!(when_label("2020-11-07 14:02"), "Nov 7 · 14:02");
        assert_eq!(timeline("no heading\n").len(), 1, "an old log still reads");
    }

    #[test]
    fn an_instruction_stays_on_one_line() {
        assert_eq!(author("Please fix\nthat too"), "▸ Please fix that too");
        assert_eq!(parse(&author("go on")), [Entry::Author("go on")]);
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
    fn two_runs_at_once_keep_their_lines_whole() {
        let dir = tempfile::tempdir().unwrap();
        let log = path(dir.path(), Path::new("doing/a.md"));
        let runs: Vec<_> = ["── run", "✗ stopped"]
            .map(|line| {
                let log = log.clone();
                std::thread::spawn(move || {
                    for _ in 0..200 {
                        append(&log, line).unwrap();
                    }
                })
            })
            .into_iter()
            .collect();
        runs.into_iter().for_each(|run| run.join().unwrap());
        let text = load(&log).unwrap();
        assert!(
            text.lines()
                .all(|line| line == "── run" || line == "✗ stopped"),
            "{text}"
        );
    }

    #[test]
    fn a_log_never_brings_back_a_deleted_issues_folder() {
        let dir = tempfile::tempdir().unwrap();
        let issues = dir.path().join("issues");
        let log = path(&issues, Path::new("doing/a.md"));
        assert!(append(&log, "late").is_err());
        assert!(!issues.exists());
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
