//! When an issue was done: the last commit that touched its file, or the
//! file's own times when git knows nothing about it.
//!
//! `git log` over a folder walks the whole history, so its answer is cached
//! per folder and only recomputed when the git files that move on a commit
//! or a branch change do: `HEAD` and the ref it names.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::platform::files::Stamps;
use crate::platform::git;

type Dates = HashMap<PathBuf, DateTime<Local>>;

/// Per folder: the git files the dates were read at, and the dates.
static CACHE: Mutex<Option<HashMap<PathBuf, (Stamps, Dates)>>> = Mutex::new(None);

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

/// The done date of each issue in `issues`, read from the history of the
/// folder holding it: a merged column gathers several projects' folders.
pub fn dates_for(issues: &[&Path]) -> Dates {
    let mut dirs: Vec<&Path> = issues.iter().filter_map(|path| path.parent()).collect();
    dirs.sort();
    dirs.dedup();
    let mut dates = Dates::new();
    for dir in dirs {
        let files: Vec<PathBuf> = issues
            .iter()
            .filter(|path| path.parent() == Some(dir))
            .map(|path| path.to_path_buf())
            .collect();
        dates.extend(done_dates(dir, &files));
    }
    dates
}

/// The date of the last commit touching each file directly inside `dir`.
/// Empty outside a git repository, or when git is missing.
fn committed_dates(dir: &Path) -> Dates {
    let head = git::head_files(dir);
    if head.is_empty() {
        return Dates::new();
    }
    let stamps = Stamps::of(head);
    {
        let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached, dates)) = cache.as_ref().and_then(|cache| cache.get(dir))
            && *cached == stamps
        {
            return dates.clone();
        }
    }
    // Not under the lock: a scan in the background must not hold up one on
    // the UI thread while git walks the history.
    let dates = git::output(
        dir,
        &[
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
        .insert(dir.to_path_buf(), (stamps, dates.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch_git::git;

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

    #[test]
    fn a_commit_moves_the_dates_a_folder_reports() {
        let scratch = tempfile::tempdir().unwrap();
        let project = scratch.path();
        let done = project.join("issues/done");
        fs::create_dir_all(&done).unwrap();
        let issue = done.join("a.md");
        fs::write(&issue, "# A\n").unwrap();
        git(project, &["init", "-q"]);
        let commit = |date: &str, text: &str| {
            fs::write(&issue, text).unwrap();
            git(project, &["add", "-A"]);
            crate::scratch_git::git_env(
                project,
                &[("GIT_COMMITTER_DATE", date), ("GIT_AUTHOR_DATE", date)],
                &["commit", "-q", "-m", date],
            );
        };
        let date = |text: &str| {
            DateTime::parse_from_rfc3339(text)
                .unwrap()
                .with_timezone(&Local)
        };
        let files = [issue.clone()];

        commit("2026-01-01T10:00:00+00:00", "# A\n");
        assert_eq!(
            done_dates(&done, &files)[&issue],
            date("2026-01-01T10:00:00+00:00")
        );

        commit("2026-02-01T10:00:00+00:00", "# A, twice\n");
        assert_eq!(
            done_dates(&done, &files)[&issue],
            date("2026-02-01T10:00:00+00:00")
        );
    }

    #[test]
    fn a_folder_outside_a_repository_has_no_committed_dates() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let issue = dir.join("a.md");
        fs::write(&issue, "# A\n").unwrap();
        assert!(committed_dates(dir).is_empty());
        assert_eq!(
            done_dates(dir, std::slice::from_ref(&issue))[&issue],
            file_time(&issue).unwrap()
        );
    }
}
