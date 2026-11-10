//! A project's health across its history: code size, complexity and tests
//! at each of the last commits, its files and those that change most,
//! when work happened, how long the issues closed took, and whether a
//! refactor is due.
//!
//! Commits are read a page of [`PAGE`] at a time along the first-parent line
//! of `HEAD`, each block of the screen on its own job through a [`Source`]. A
//! commit's measures never change, so each is kept in
//! `.git/iboard/stats.json` and only the commits never seen before are read,
//! through one `git cat-file --batch`; files unchanged between commits are
//! measured once per run.

mod cycle;
mod measure;
mod release;
mod source;
mod watch;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use crate::agenda::AgendaDays;
use crate::git::{self, Batch};
use crate::job::Cancel;
pub use cycle::{Cycle, Cycles, span};
pub use measure::{FileMetrics, Language, language_of, measure};
pub use release::{Release, ReleasePrefs, ReleasePrefsSection, Releases};
use source::Listed;
pub use source::{Measures, Need, Source};
pub use watch::{Limits, Signal, culprits, cycle_drift, refactor_draft, refactor_due, signals};

/// How many commits one page of history holds.
pub const PAGE: usize = 100;

/// A file at least this complex, and changed at least [`HOTSPOT_CHANGES`]
/// times on the page, is a hot spot.
pub const HOTSPOT_COMPLEXITY: usize = 60;
pub const HOTSPOT_CHANGES: usize = 8;

/// What a stopped job answers.
const STOPPED: &str = "stopped";

/// Files bigger than this are generated or data, not code to measure.
const MAX_BLOB: u64 = 1 << 20;

/// Commits read so far out of those to read, shared with the UI.
#[derive(Debug, Default)]
pub struct Progress {
    done: AtomicUsize,
    total: AtomicUsize,
}

impl Progress {
    /// `(read, to read)`; `to read` is 0 until the commits are listed.
    pub fn get(&self) -> (usize, usize) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

/// The measures of the whole tree at one commit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub short: String,
    /// The committer date, RFC 3339.
    pub date: String,
    pub files: usize,
    pub code: usize,
    pub test_code: usize,
    pub tests: usize,
    /// McCabe's number summed over every function.
    pub complexity: usize,
    /// Code lines per language, most first.
    pub languages: Vec<(String, usize)>,
}

impl Snapshot {
    /// The `YYYY-MM-DD` of the commit.
    pub fn day(&self) -> &str {
        self.date.get(..10).unwrap_or(&self.date)
    }

    /// Share of the code lines that are tests.
    pub fn test_share(&self) -> f32 {
        ratio(self.test_code, self.code)
    }

    /// Complexity per thousand code lines, so size alone doesn't raise it.
    pub fn complexity_per_kloc(&self) -> f32 {
        ratio(self.complexity * 1000, self.code)
    }
}

fn ratio(part: usize, whole: usize) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}

/// A source file at the page's newest commit, and how often the page
/// changed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    pub path: String,
    /// Commits of the page that changed it.
    pub changes: usize,
    pub code: usize,
    pub complexity: usize,
}

impl FileStat {
    /// Complex and often changed: where debt costs most.
    pub fn is_hotspot(&self) -> bool {
        self.complexity >= HOTSPOT_COMPLEXITY && self.changes >= HOTSPOT_CHANGES
    }
}

/// One page of history, measured.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    /// Which page: 0 is the newest [`PAGE`] commits, 1 the ones before…
    pub page: usize,
    /// There are commits older than this page.
    pub older: bool,
    /// Oldest first.
    pub snapshots: Vec<Snapshot>,
    /// The commits of each local day, newest first, over the
    /// [`agenda::AGENDA_DAYS`] up to today, across the supervised projects.
    pub agenda: AgendaDays,
    /// The source files at the page's newest commit, most code first.
    pub files: Vec<FileStat>,
    /// The issues the page's commits closed, and how long each took.
    pub cycles: Cycles,
    /// The release tags the page shows.
    pub releases: Releases,
    /// Where the measures are kept.
    pub cache: Option<PathBuf>,
}

/// What a blob measured, per blob, file kind and language.
type Blobs = HashMap<(String, bool, &'static str), Option<FileMetrics>>;

/// One tree entry worth measuring: a source file of the project's own.
struct Entry<'a> {
    sha: &'a str,
    path: &'a str,
    language: &'static Language,
}

/// The source files of `tree`, as `git ls-tree -r -z -l` prints them.
fn entries(tree: &str) -> Vec<Entry<'_>> {
    tree.split('\0')
        .filter_map(|record| {
            let (meta, path) = record.split_once('\t')?;
            let mut meta = meta.split_whitespace();
            let (mode, kind, sha, size) = (meta.next()?, meta.next()?, meta.next()?, meta.next()?);
            let size: u64 = size.parse().ok()?;
            let language = language_of(path)?;
            (kind == "blob" && mode != "120000" && size <= MAX_BLOB && measure::is_own(path))
                .then_some(Entry {
                    sha,
                    path,
                    language,
                })
        })
        .collect()
}

fn measure_commit(
    dir: &Path,
    batch: &mut Batch,
    blobs: &mut Blobs,
    commit: &Listed,
) -> Result<Snapshot, String> {
    let tree = git::output(
        dir,
        &["ls-tree", "-r", "-z", "-l", "--full-tree", &commit.hash],
    )?;
    let mut snapshot = Snapshot {
        short: commit.short.clone(),
        date: commit.date.clone(),
        ..Snapshot::default()
    };
    let mut languages: HashMap<&str, usize> = HashMap::new();
    for entry in entries(&tree) {
        let key = (
            entry.sha.to_string(),
            measure::is_test_file(entry.path),
            entry.language.name,
        );
        let metrics = match blobs.get(&key) {
            Some(metrics) => *metrics,
            None => {
                let metrics = batch.read(entry.sha)?.and_then(|bytes| {
                    let text = std::str::from_utf8(&bytes).ok()?;
                    Some(measure(entry.path, entry.language, text))
                });
                blobs.insert(key, metrics);
                metrics
            }
        };
        let Some(m) = metrics else {
            continue;
        };
        add(&mut snapshot, &m);
        *languages.entry(entry.language.name).or_default() += m.code;
    }
    snapshot.languages = top(languages.into_iter().map(|(k, v)| (k.to_string(), v)));
    Ok(snapshot)
}

fn add(snapshot: &mut Snapshot, m: &FileMetrics) {
    snapshot.files += 1;
    snapshot.code += m.code;
    snapshot.test_code += m.test_code;
    snapshot.tests += m.tests;
    snapshot.complexity += m.complexity();
}

/// Highest first, ties by name.
fn top(items: impl Iterator<Item = (String, usize)>) -> Vec<(String, usize)> {
    let mut items: Vec<_> = items.filter(|(_, n)| *n > 0).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items
}

/// The source files at `newest`, with how many of the page's commits
/// changed each.
fn files_at(dir: &Path, newest: &str, cancel: &Cancel) -> Result<Vec<FileStat>, String> {
    let out = git::output(
        dir,
        &[
            "log",
            "--first-parent",
            &format!("--max-count={PAGE}"),
            "--format=",
            "--name-only",
            newest,
        ],
    )?;
    let mut changes: HashMap<&str, usize> = HashMap::new();
    for path in out.lines().filter(|line| !line.is_empty()) {
        *changes.entry(path).or_default() += 1;
    }
    let tree = git::output(dir, &["ls-tree", "-r", "-z", "-l", "--full-tree", newest])?;
    let mut batch = Batch::open(dir)?;
    let mut files = Vec::new();
    for entry in entries(&tree) {
        if cancel.is_cancelled() {
            return Err(STOPPED.to_string());
        }
        let Some(bytes) = batch.read(entry.sha)? else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let m = measure(entry.path, entry.language, text);
        if m.code == 0 {
            continue;
        }
        files.push(FileStat {
            path: entry.path.to_string(),
            changes: changes.get(entry.path).copied().unwrap_or(0),
            code: m.code,
            complexity: m.complexity(),
        });
    }
    files.sort_by(|a, b| b.code.cmp(&a.code).then_with(|| a.path.cmp(&b.path)));
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch_git::{git, git_env, git_stdin};
    use std::sync::Arc;

    const ALL: [Need; 5] = [
        Need::Commits,
        Need::Measures,
        Need::Cycles,
        Need::Files,
        Need::Agenda,
    ];

    fn source(dir: &Path, root: &Path, page: usize) -> Source {
        Source::new(dir, root, page, Arc::new(Measures::new(dir)))
    }

    fn collect(dir: &Path, root: &Path, page: usize) -> Result<Stats, String> {
        source(dir, root, page).read(&ALL, &Cancel::default())
    }

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        dir
    }

    fn commit(dir: &Path, file: &str, text: &str, message: &str) {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", message]);
    }

    fn import(dir: &Path, commits: &[(&str, Option<(&str, &str)>)]) {
        let mut stream = String::new();
        for (hour, (message, file)) in commits.iter().enumerate() {
            let time = 1_790_000_000 + hour * 3600;
            stream += &format!("commit refs/heads/main\ncommitter t <t@t> {time} +0000\n");
            stream += &format!("data {}\n{message}\n", message.len());
            if let Some((path, text)) = file {
                stream += &format!("M 644 inline {path}\ndata {}\n{text}\n", text.len());
            }
        }
        git_stdin(dir, &stream, &["fast-import", "--quiet"]);
        git(dir, &["reset", "-q", "--hard", "main"]);
    }

    #[test]
    fn each_commit_is_measured_oldest_first_and_cached() {
        let dir = repo();
        commit(dir.path(), "src/lib.rs", "fn a() {}\n", "one");
        commit(
            dir.path(),
            "src/lib.rs",
            "fn a() {\n    if x {}\n}\n// TODO\n",
            "two",
        );
        commit(dir.path(), "tests/t.rs", "#[test]\nfn t() {}\n", "three");
        commit(dir.path(), "README.md", "# Doc\n", "four");
        let first = source(dir.path(), dir.path(), 0);
        let stats = first.read(&ALL, &Cancel::default()).unwrap();
        assert_eq!(first.progress.get(), (4, 4));
        let code: Vec<usize> = stats.snapshots.iter().map(|s| s.code).collect();
        assert_eq!(code, [1, 3, 5, 5]);
        let last = stats.snapshots.last().unwrap();
        assert_eq!((last.files, last.tests, last.test_code), (2, 1, 2));
        assert_eq!(last.complexity, 3);
        assert!(!stats.older);
        assert_eq!(stats.agenda.days.values().map(Vec::len).sum::<usize>(), 4);
        let lib = stats.files.iter().find(|f| f.path == "src/lib.rs").unwrap();
        assert_eq!((lib.changes, lib.code), (2, 3));
        assert_eq!(stats.files[0].path, "src/lib.rs", "most code first");
        assert!(!lib.is_hotspot());
        assert!(stats.cache.as_ref().unwrap().is_file());

        commit(dir.path(), "src/b.rs", "fn b() {}\n", "five");
        let next = source(dir.path(), dir.path(), 0);
        let again = next.read(&ALL, &Cancel::default()).unwrap();
        assert_eq!(next.progress.get(), (1, 1), "only the new commit is read");
        assert_eq!(again.snapshots.len(), 5);
    }

    #[test]
    fn a_closed_issue_takes_from_its_first_commit_to_its_closing_one() {
        let dir = repo();
        let at = |date: &str, args: &[&str]| {
            git_env(dir.path(), &[("GIT_COMMITTER_DATE", date)], args);
        };
        let issues = dir.path().join("issues");
        std::fs::create_dir_all(issues.join("todo")).unwrap();
        std::fs::create_dir_all(issues.join("done")).unwrap();
        std::fs::write(issues.join("todo/001-a.md"), "# A\n").unwrap();
        at("2026-10-01T10:00:00+00:00", &["add", "."]);
        at("2026-10-01T10:00:00+00:00", &["commit", "-q", "-m", "plan"]);
        std::fs::rename(issues.join("todo/001-a.md"), issues.join("done/a.md")).unwrap();
        std::fs::write(issues.join("done/b.md"), "# B\n").unwrap();
        at("2026-10-03T12:30:00+00:00", &["add", "-A"]);
        let message = "Do it\n\ncloses a.md\ncloses b.md";
        at(
            "2026-10-03T12:30:00+00:00",
            &["commit", "-q", "-m", message],
        );
        let stats = collect(dir.path(), &issues, 0).unwrap();
        let cycles: Vec<(&str, i64)> = stats
            .cycles
            .cycles
            .iter()
            .map(|c| (c.issue.as_str(), c.seconds))
            .collect();
        assert_eq!(cycles, [("a.md", 2 * 86_400 + 2 * 3600 + 30 * 60)]);
        assert_eq!(stats.cycles.unknown, 1, "b.md was never seen before");
    }

    #[test]
    fn a_cycle_runs_from_entering_doing_to_entering_done() {
        let dir = repo();
        let issues = dir.path().join("issues");
        let at = |date: &str, args: &[&str]| {
            git_env(dir.path(), &[("GIT_COMMITTER_DATE", date)], args);
        };
        let step = |date: &str, from: Option<&str>, to: &str, message: &str| {
            std::fs::create_dir_all(issues.join(to).parent().unwrap()).unwrap();
            match from {
                Some(from) => std::fs::rename(issues.join(from), issues.join(to)).unwrap(),
                None => std::fs::write(issues.join(to), "# A\n").unwrap(),
            }
            at(date, &["add", "-A"]);
            at(date, &["commit", "-q", "-m", message]);
        };
        step("2026-09-01T10:00:00+00:00", None, "todo/a.md", "plan");
        step(
            "2026-10-01T10:00:00+00:00",
            Some("todo/a.md"),
            "doing/a.md",
            "start",
        );
        step(
            "2026-10-03T10:00:00+00:00",
            Some("doing/a.md"),
            "done/a.md",
            "finish, without naming it",
        );
        let stats = collect(dir.path(), &issues, 0).unwrap();
        let cycles: Vec<(&str, i64)> = stats
            .cycles
            .cycles
            .iter()
            .map(|c| (c.issue.as_str(), c.seconds))
            .collect();
        assert_eq!(cycles, [("a.md", 2 * 86_400)]);
    }

    #[test]
    fn an_older_page_measures_its_own_commits() {
        let dir = repo();
        let mut commits = vec![
            ("plan", Some(("issues/todo/a.md", "# A\n"))),
            ("Do it\n\ncloses a.md", Some(("src/lib.rs", "fn a() {}\n"))),
        ];
        commits.extend([("more", None); PAGE]);
        import(dir.path(), &commits);
        let issues = dir.path().join("issues");
        let stats = collect(dir.path(), &issues, 1).unwrap();
        let code: Vec<usize> = stats.snapshots.iter().map(|s| s.code).collect();
        assert_eq!(code, [0, 1], "the two oldest commits");
        let lib = stats.files.iter().find(|f| f.path == "src/lib.rs").unwrap();
        assert_eq!(lib.changes, 1);
        let cycles: Vec<(&str, i64)> = stats
            .cycles
            .cycles
            .iter()
            .map(|c| (c.issue.as_str(), c.seconds))
            .collect();
        assert_eq!(cycles, [("a.md", 3600)]);
    }

    #[test]
    fn a_file_with_a_non_ascii_name_counts_its_changes() {
        let dir = repo();
        let file = "src/caf\u{e9}.rs";
        import(
            dir.path(),
            &[
                ("one", Some((file, "fn a() {}\n"))),
                ("two", Some((file, "fn a() {}\nfn b() {}\n"))),
            ],
        );
        let stats = collect(dir.path(), dir.path(), 0).unwrap();
        let cafe = stats.files.iter().find(|f| f.path == file).unwrap();
        assert_eq!(cafe.changes, 2);
    }

    #[test]
    fn a_project_without_commits_says_so() {
        let dir = repo();
        let err = collect(dir.path(), dir.path(), 0).unwrap_err();
        assert!(err.contains("no commits"), "{err}");
    }

    #[test]
    fn a_cancelled_run_stops_and_the_next_job_finishes_it() {
        let dir = repo();
        commit(dir.path(), "a.rs", "fn a() {}\n", "one");
        let page = source(dir.path(), dir.path(), 0);
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(page.read(&ALL, &cancel).is_err());
        let stats = page.read(&ALL, &Cancel::default()).unwrap();
        assert_eq!(stats.snapshots.len(), 1);
    }

    /// Every block's job asks at once; the commits are measured once, and a
    /// refresh sharing the cache measures none.
    #[test]
    fn jobs_share_the_measures() {
        let dir = repo();
        commit(dir.path(), "a.rs", "fn a() {}\n", "one");
        commit(dir.path(), "b.rs", "fn b() {}\n", "two");
        let page = Arc::new(source(dir.path(), dir.path(), 0));
        let jobs: Vec<_> = (0..8)
            .map(|_| {
                let page = Arc::clone(&page);
                std::thread::spawn(move || page.read(&[Need::Measures], &Cancel::default()))
            })
            .collect();
        for job in jobs {
            assert_eq!(job.join().unwrap().unwrap().snapshots.len(), 2);
        }
        assert_eq!(page.progress.get(), (2, 2));
        let refreshed = page.at_page(0);
        let stats = refreshed.read(&[Need::Measures], &Cancel::default());
        assert_eq!(stats.unwrap().snapshots.len(), 2);
        assert_eq!(refreshed.progress.get(), (0, 0));
    }

    #[test]
    fn the_listed_commits_come_without_their_measures() {
        let dir = repo();
        commit(dir.path(), "a.rs", "fn a() {}\n", "one");
        let page = source(dir.path(), dir.path(), 0);
        let stats = page.read(&[Need::Commits], &Cancel::default()).unwrap();
        assert_eq!(stats.snapshots.len(), 1);
        assert_eq!(stats.snapshots[0].code, 0);
        assert_eq!(page.progress.get(), (0, 0));

        commit(dir.path(), "b.rs", "fn b() {}\n", "two");
        let page = source(dir.path(), dir.path(), 0);
        let stats = page.read(&[Need::Commits], &Cancel::default()).unwrap();
        let shorts: Vec<&str> = stats.snapshots.iter().map(|s| s.short.as_str()).collect();
        let log = git(dir.path(), &["log", "--reverse", "--format=%h"]);
        assert_eq!(
            shorts,
            log.lines().collect::<Vec<_>>(),
            "oldest first, as measured ones are"
        );
    }
}
