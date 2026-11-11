//! The project's spend history: one JSON line per finished service session in
//! `$XDG_DATA_HOME/iboard/usage.jsonl`, `~/.local/share/iboard/usage.jsonl`.
//!
//! OpenCode prunes its sessions and a cache is made to be cleared, so this is
//! user data. A per-repo `.git/iboard/` file would go with a clone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domains::agent::runtime::{Billing, Tokens, Usage, status_of};
use crate::platform::files;

/// How long a session stays in the log.
const KEEP_DAYS: i64 = 400;

/// The engine the service sessions come from.
const ENGINE: &str = "opencode";

/// One finished session, as the log keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub session: String,
    /// The canonical path of the project the session ran in.
    pub project: PathBuf,
    pub engine: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub billing: Billing,
    pub cost: Option<f64>,
    pub tokens: Tokens,
    /// RFC 3339 in UTC, as the engine's own session times say.
    pub started: String,
    pub ended: String,
}

/// The log file, when the user has a data folder.
pub fn path() -> Option<PathBuf> {
    files::data_dir().map(|dir| dir.join("usage.jsonl"))
}

/// Every entry, oldest first, with a truncated last line left out.
pub fn read() -> Vec<Entry> {
    path().map_or_else(Vec::new, |path| read_at(&path))
}

/// The entries of `project`, oldest first.
pub fn read_for(project: &Path) -> Vec<Entry> {
    let project = canonical(project);
    read()
        .into_iter()
        .filter(|entry| entry.project == project)
        .collect()
}

/// Adds every finished session in `sessions` to the log of `project`.
pub(crate) fn record(sessions: &[Value], project: &Path) -> Result<(), String> {
    let entries = finished(sessions, project);
    if entries.is_empty() {
        return Ok(());
    }
    append(&entries)
}

/// The finished sessions of `project` in a service session list.
pub(crate) fn finished(sessions: &[Value], project: &Path) -> Vec<Entry> {
    let project = canonical(project);
    sessions
        .iter()
        .filter_map(|session| Entry::from_session(session, &project))
        .collect()
}

/// Adds `entries`, replacing an earlier line for the same session and dropping
/// what is older than [`KEEP_DAYS`].
pub(crate) fn append(entries: &[Entry]) -> Result<(), String> {
    let path = path().ok_or_else(|| "Couldn't find the data folder.".to_string())?;
    append_at(&path, entries)
}

impl Entry {
    fn from_session(session: &Value, project: &Path) -> Option<Self> {
        let status = status_of(session);
        if status.running {
            return None;
        }
        let id = session["id"].as_str().filter(|id| !id.is_empty())?;
        let directory = session["location"]["directory"].as_str()?;
        let directory = canonical(Path::new(directory));
        if directory != project {
            return None;
        }
        let usage = Usage::from_session(session).unwrap_or_default();
        let ended = ended_of(session, status.created);
        Some(Self {
            session: id.to_string(),
            project: directory,
            engine: ENGINE.to_string(),
            provider: usage.provider,
            model: usage.model,
            billing: usage.billing,
            cost: usage.cost,
            tokens: usage.tokens,
            started: stamp(status.created),
            ended: stamp(ended),
        })
    }
}

/// The last time a finished session wrote anything, in the engine's
/// milliseconds.
fn ended_of(session: &Value, created: u64) -> u64 {
    ["updated", "idle", "busy"]
        .iter()
        .filter_map(|key| session["time"][key].as_u64())
        .max()
        .unwrap_or(created)
        .max(created)
}

/// The engine's milliseconds as an RFC 3339 timestamp.
fn stamp(millis: u64) -> String {
    let at = DateTime::<Utc>::from_timestamp_millis(millis as i64).unwrap_or_default();
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn instant(stamp: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn retained(entry: &Entry, cutoff: DateTime<Utc>) -> bool {
    instant(&entry.ended).is_none_or(|ended| ended >= cutoff)
}

fn read_at(path: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut entries: HashMap<String, Entry> = HashMap::new();
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Entry>(line) else {
            continue;
        };
        entries.insert(entry.session.clone(), entry);
    }
    sorted(entries)
}

fn append_at(path: &Path, entries: &[Entry]) -> Result<(), String> {
    let now = Utc::now();
    let cutoff = now - Duration::days(KEEP_DAYS);
    let mut merged: HashMap<String, Entry> = read_at(path)
        .into_iter()
        .map(|entry| (entry.session.clone(), entry))
        .collect();
    merged.retain(|_, entry| retained(entry, cutoff));
    for entry in entries.iter().filter(|entry| retained(entry, cutoff)) {
        merged.insert(entry.session.clone(), entry.clone());
    }
    let mut text = String::new();
    for entry in sorted(merged) {
        text.push_str(
            &serde_json::to_string(&entry)
                .map_err(|err| format!("Couldn't encode a usage entry: {err}"))?,
        );
        text.push('\n');
    }
    files::write_creating(path, text).map_err(|err| {
        let err = std::io::Error::from(err);
        format!("Couldn't write {}: {err}", path.display())
    })
}

fn sorted(entries: HashMap<String, Entry>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = entries.into_values().collect();
    entries.sort_by(|a, b| {
        (&a.started, &a.ended, &a.session).cmp(&(&b.started, &b.ended, &b.session))
    });
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project(dir: &Path) -> PathBuf {
        std::fs::canonicalize(dir).unwrap()
    }

    fn base() -> u64 {
        Utc::now().timestamp_millis() as u64 - 3_600_000
    }

    fn session(id: &str, project: &Path, cost: f64, updated: u64) -> Value {
        let at = base();
        json!({
            "id": id,
            "location": {"directory": project},
            "model": {"id": "deepseek-flash", "providerID": "deepseek"},
            "cost": cost,
            "tokens": {"input": 1234, "output": 56, "reasoning": 7,
                       "cache": {"read": 890, "write": 12}},
            "time": {"created": at, "busy": at + 1_000, "idle": at + 2_000,
                     "updated": at + updated},
        })
    }

    fn entry(id: &str, project: &Path, ended: String) -> Entry {
        Entry {
            session: id.to_string(),
            project: project.to_path_buf(),
            engine: ENGINE.to_string(),
            provider: Some("deepseek".to_string()),
            model: Some("deepseek-flash".to_string()),
            billing: Billing::Metered,
            cost: Some(0.42),
            tokens: Tokens::default(),
            started: ended.clone(),
            ended,
        }
    }

    #[test]
    fn a_finished_session_is_written_with_its_numbers() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");
        let sessions = [session("ses_1", &project, 0.42, 4_000)];

        append_at(&log, &finished(&sessions, &project)).unwrap();

        let entries = read_at(&log);
        assert_eq!(entries.len(), 1);
        let seen = &entries[0];
        assert_eq!(seen.session, "ses_1");
        assert_eq!(seen.project, project);
        assert_eq!(seen.engine, "opencode");
        assert_eq!(seen.provider.as_deref(), Some("deepseek"));
        assert_eq!(seen.model.as_deref(), Some("deepseek-flash"));
        assert_eq!(seen.billing, Billing::Metered);
        assert_eq!(seen.cost, Some(0.42));
        assert_eq!(seen.tokens.input, 1234);
        assert_eq!(seen.tokens.cache_read, 890);
        assert!(seen.started < seen.ended);
    }

    #[test]
    fn a_running_session_is_not_written_yet() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let running = json!({
            "id": "ses_live",
            "location": {"directory": project},
            "time": {"created": 1_000, "busy": 2_000, "idle": 1_500},
        });
        assert!(finished(&[running], &project).is_empty());
    }

    #[test]
    fn a_session_is_deduped_by_its_id() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");

        append_at(
            &log,
            &finished(&[session("ses_1", &project, 0.1, 4_000)], &project),
        )
        .unwrap();
        append_at(
            &log,
            &finished(&[session("ses_1", &project, 0.3, 5_000)], &project),
        )
        .unwrap();

        let entries = read_at(&log);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].cost, Some(0.3), "the newer line wins");
    }

    #[test]
    fn a_truncated_last_line_is_ignored() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");
        let whole = entry("ses_1", &project, stamp(4_000));
        std::fs::write(
            &log,
            format!(
                "{}\n{{\"session\":\"ses_2\",",
                serde_json::to_string(&whole).unwrap()
            ),
        )
        .unwrap();

        let entries = read_at(&log);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session, "ses_1");
    }

    #[test]
    fn entries_read_back_oldest_first() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");
        let old = entry("ses_old", &project, stamp(2_000));
        let new = entry("ses_new", &project, stamp(4_000));
        std::fs::write(
            &log,
            format!(
                "{}\n{}\n",
                serde_json::to_string(&new).unwrap(),
                serde_json::to_string(&old).unwrap()
            ),
        )
        .unwrap();

        let entries = read_at(&log);
        let ids: Vec<&str> = entries.iter().map(|entry| entry.session.as_str()).collect();
        assert_eq!(ids, ["ses_old", "ses_new"]);
    }

    #[test]
    fn a_line_older_than_400_days_is_dropped() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");
        let now = Utc::now();
        let stamp = |at: DateTime<Utc>| at.to_rfc3339_opts(SecondsFormat::Millis, true);
        let old = entry(
            "ses_old",
            &project,
            stamp(now - Duration::days(KEEP_DAYS + 1)),
        );
        let recent = entry("ses_recent", &project, stamp(now - Duration::days(1)));
        std::fs::write(
            &log,
            format!(
                "{}\n{}\n",
                serde_json::to_string(&old).unwrap(),
                serde_json::to_string(&recent).unwrap()
            ),
        )
        .unwrap();

        let fresh = entry("ses_fresh", &project, stamp(now));
        append_at(&log, &[fresh]).unwrap();

        let entries = read_at(&log);
        let ids: Vec<&str> = entries.iter().map(|entry| entry.session.as_str()).collect();
        assert_eq!(ids, ["ses_recent", "ses_fresh"]);
    }
}
