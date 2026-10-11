//! When an issue was done: the last commit that touched its file, or the
//! file's own times when git knows nothing about it.
//!
//! `git log` over a folder walks the whole history, so its answer is cached
//! per folder and only recomputed when `HEAD` moves. The board refreshes every
//! two seconds, and asking git for `HEAD` is cheap.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::SystemTime;

use chrono::{DateTime, Local};

type Dates = HashMap<PathBuf, DateTime<Local>>;

/// Per folder: the `HEAD` the dates were read at, and the dates.
static CACHE: Mutex<Option<HashMap<PathBuf, (String, Dates)>>> = Mutex::new(None);

/// The done date of each file in `files`, all directly inside `dir`.
pub fn done_dates(dir: &Path, files: &[PathBuf]) -> Dates {
    let committed = committed_dates(dir);
    files
        .iter()
        .filter_map(|path| {
            let date = committed.get(path).copied().or_else(|| file_time(path))?;
            Some((path.clone(), date))
        })
        .collect()
}

/// The date of the last commit touching each file directly inside `dir`.
/// Empty outside a git repository, or when git is missing.
fn committed_dates(dir: &Path) -> Dates {
    let Some(head) = git(dir, &["rev-parse", "HEAD"]) else {
        return Dates::new();
    };
    {
        let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached_head, dates)) = cache.as_ref().and_then(|cache| cache.get(dir))
            && *cached_head == head
        {
            return dates.clone();
        }
    }
    // Not under the lock: a scan in the background must not hold up one on
    // the UI thread while git walks the history.
    let dates = git(
        dir,
        &[
            "-c",
            "core.quotePath=false",
            "log",
            "--format=%x00%cI",
            "--name-only",
            "--relative",
            "--",
            ".",
        ],
    )
    .map(|log| parse_log(dir, &log))
    .unwrap_or_default();
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(dir.to_path_buf(), (head, dates.clone()));
    dates
}

/// Reads `git log --format=%x00%cI --name-only` output, newest commit first:
/// the first date seen for a file is its latest.
fn parse_log(dir: &Path, log: &str) -> Dates {
    let mut dates = Dates::new();
    let mut current = None;
    for line in log.lines() {
        if let Some(date) = line.strip_prefix('\0') {
            current = DateTime::parse_from_rfc3339(date.trim())
                .ok()
                .map(|d| d.with_timezone(&Local));
        } else if !line.is_empty()
            && !line.contains('/')
            && let Some(date) = current
        {
            dates.entry(dir.join(line)).or_insert(date);
        }
    }
    dates
}

/// The later of the file's modified and status-change times: moving a file
/// into a folder (a rename) only touches the latter.
fn file_time(path: &Path) -> Option<DateTime<Local>> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok();
    #[cfg(unix)]
    let changed = {
        use std::os::unix::fs::MetadataExt;
        u64::try_from(meta.ctime())
            .ok()
            .map(|secs| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs))
    };
    #[cfg(not(unix))]
    let changed: Option<SystemTime> = None;
    modified.max(changed).map(DateTime::<Local>::from)
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_commit_wins_and_subfolders_are_skipped() {
        let dir = Path::new("/r/done");
        let log = "\u{0}2026-10-02T18:00:00+02:00\n\nb.md\n\n\
                   \u{0}2026-09-01T10:00:00+00:00\n\na.md\nb.md\nsub/c.md\n";
        let dates = parse_log(dir, log);
        assert_eq!(
            dates[&dir.join("b.md")],
            DateTime::parse_from_rfc3339("2026-10-02T18:00:00+02:00").unwrap()
        );
        assert_eq!(
            dates[&dir.join("a.md")],
            DateTime::parse_from_rfc3339("2026-09-01T10:00:00+00:00").unwrap()
        );
        assert!(!dates.contains_key(&dir.join("sub/c.md")));
        assert_eq!(dates.len(), 2);
    }
}
