//! The commits that closed an issue.
//!
//! The relation is not stored in the ticket: it is read from the history's
//! commit messages, where AGENTS.md asks for a `closes <file>.md` line. Grepping
//! the message is robust to file moves and needs no rename detection, and the
//! `[DISCOVERY]` ticket chose it over walking `git mv`s.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One commit whose message names an issue's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// The abbreviated hash, as `git log --format=%h`.
    pub hash: String,
    /// The committer date, RFC 3339.
    pub date: String,
    /// The subject line.
    pub subject: String,
}

impl Commit {
    /// The `YYYY-MM-DD` of the commit, for a compact label.
    pub fn day(&self) -> &str {
        self.date.get(..10).unwrap_or(&self.date)
    }
}

/// Per `(folder, file)`: its commits, and when they were read.
type Cached = HashMap<(PathBuf, String), (Vec<Commit>, Instant)>;
static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

/// How long a lookup is trusted. The header asks every frame, so without this
/// each frame would spawn `git`; the board refreshes well within it.
const TTL: Duration = Duration::from_secs(2);

/// The commits whose message names `file_name`, newest first. Runs in `dir`,
/// so pass the issue's column folder (git finds the repository above it). An
/// empty list when git is missing, `dir` is not a repository, or no commit
/// mentions the file.
pub fn for_file(dir: &Path, file_name: &str) -> Vec<Commit> {
    let key = (dir.to_path_buf(), file_name.to_string());
    {
        let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((list, at)) = cache.as_ref().and_then(|cache| cache.get(&key))
            && at.elapsed() < TTL
        {
            return list.clone();
        }
    }
    let list = run(dir, file_name);
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key, (list.clone(), Instant::now()));
    list
}

fn run(dir: &Path, file_name: &str) -> Vec<Commit> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "log",
            "--all",
            "--fixed-strings",
            "--grep",
            file_name,
            "--format=%h%x09%cI%x09%s",
        ])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_line)
        .collect()
}

fn parse_line(line: &str) -> Option<Commit> {
    let mut parts = line.splitn(3, '\t');
    let hash = parts.next()?.trim().to_string();
    let date = parts.next()?.to_string();
    let subject = parts.next().unwrap_or("").to_string();
    (!hash.is_empty()).then_some(Commit {
        hash,
        date,
        subject,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_splits_into_hash_date_and_subject() {
        let commit = parse_line("1a2b3c4\t2026-10-08T12:00:00+02:00\tDo the thing").unwrap();
        assert_eq!(commit.hash, "1a2b3c4");
        assert_eq!(commit.day(), "2026-10-08");
        assert_eq!(commit.subject, "Do the thing");
    }

    #[test]
    fn a_blank_line_is_dropped() {
        assert!(parse_line("").is_none());
        assert!(parse_line("\t2026-10-08\tx").is_none());
    }

    #[test]
    fn a_missing_subject_is_empty_not_a_loss() {
        let commit = parse_line("abc\t2026-10-08T00:00:00Z").unwrap();
        assert_eq!(commit.subject, "");
    }
}
