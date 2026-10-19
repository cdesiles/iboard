//! A project's health across its history: code size, complexity, tests and
//! TODOs at each of the last commits, the files that change most, and when
//! work happened.
//!
//! Commits are read a page of [`PAGE`] at a time along the first-parent line
//! of `HEAD`. A commit's measures never change, so each is kept in
//! `.git/iboard/stats.json` and only the commits never seen before are read,
//! through one `git cat-file --batch`; files unchanged between commits are
//! measured once per run.

mod measure;

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use crate::files;
use crate::job::Cancel;
pub use measure::{FileMetrics, Language, language_of, measure};

/// How many commits one page of history holds.
pub const PAGE: usize = 100;

/// A file this long, in code lines, counts as large.
pub const LARGE_FILE: usize = 500;

/// How many days the agenda covers, up to the page's newest commit.
pub const AGENDA_DAYS: i64 = 26 * 7;

/// Bumped whenever the measures change, so older caches are read again.
const CACHE_VERSION: u32 = 1;

/// Files bigger than this are generated or data, not code to measure.
const MAX_BLOB: u64 = 1 << 20;

/// How many files the top lists keep.
const TOP: usize = 5;

/// How many hot spots are shown.
const HOTSPOTS: usize = 8;

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
    pub hash: String,
    pub short: String,
    /// The committer date, RFC 3339.
    pub date: String,
    pub author: String,
    pub subject: String,
    pub files: usize,
    pub code: usize,
    pub comments: usize,
    pub blank: usize,
    pub test_code: usize,
    pub tests: usize,
    /// McCabe's number summed over every function.
    pub complexity: usize,
    /// The functions (or files, where none is declared) it is spread over.
    pub units: usize,
    pub todos: usize,
    /// Files over [`LARGE_FILE`] code lines.
    pub large_files: usize,
    /// Code lines per language, most first.
    pub languages: Vec<(String, usize)>,
    /// The files with the most code lines.
    pub largest: Vec<(String, usize)>,
    /// The files with the highest complexity.
    pub complex: Vec<(String, usize)>,
}

impl Snapshot {
    /// The `YYYY-MM-DD` of the commit.
    pub fn day(&self) -> &str {
        self.date.get(..10).unwrap_or(&self.date)
    }

    /// Average complexity per function.
    pub fn mean_complexity(&self) -> f32 {
        ratio(self.complexity, self.units)
    }

    /// Share of the code lines that are tests.
    pub fn test_share(&self) -> f32 {
        ratio(self.test_code, self.code)
    }

    /// Comment lines per code line.
    pub fn comment_ratio(&self) -> f32 {
        ratio(self.comments, self.code)
    }

    /// TODO marks per thousand code lines.
    pub fn todo_density(&self) -> f32 {
        ratio(self.todos * 1000, self.code)
    }
}

fn ratio(part: usize, whole: usize) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}

/// A file that changes often, and how complex it is now: where debt costs most.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotspot {
    pub path: String,
    /// Commits of the page that changed it.
    pub changes: usize,
    pub code: usize,
    pub complexity: usize,
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
    /// Commits per `YYYY-MM-DD`, over the [`AGENDA_DAYS`] up to the newest.
    pub agenda: BTreeMap<String, usize>,
    pub hotspots: Vec<Hotspot>,
    /// Where the measures are kept.
    pub cache: Option<PathBuf>,
}

/// Measures page `page` of the history of the repository holding `dir`,
/// reading only the commits the cache lacks. Stops early, with an error,
/// once `cancel` is set; what was read by then stays cached.
pub fn collect(
    dir: &Path,
    page: usize,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<Stats, String> {
    let listed = list(dir, page)?;
    let older = listed.len() > PAGE;
    let commits = &listed[..listed.len().min(PAGE)];
    let cache_file = cache_file(dir);
    let mut cache = cache_file.as_deref().map(Cache::read).unwrap_or_default();
    let missing: Vec<&Listed> = commits
        .iter()
        .filter(|c| !cache.snapshots.contains_key(&c.hash))
        .collect();
    progress.total.store(missing.len(), Ordering::Relaxed);
    if !missing.is_empty() {
        let mut batch = Batch::open(dir)?;
        let mut blobs = HashMap::new();
        for (at, commit) in missing.iter().enumerate() {
            if cancel.is_cancelled() {
                break;
            }
            let snapshot = measure_commit(dir, &mut batch, &mut blobs, commit)?;
            cache.snapshots.insert(commit.hash.clone(), snapshot);
            progress.done.store(at + 1, Ordering::Relaxed);
            if (at + 1).is_multiple_of(10)
                && let Some(file) = &cache_file
            {
                cache.write(file);
            }
        }
        if let Some(file) = &cache_file {
            cache.write(file);
        }
    }
    if cancel.is_cancelled() {
        return Err("stopped".to_string());
    }
    let snapshots: Vec<Snapshot> = commits
        .iter()
        .rev()
        .filter_map(|c| cache.snapshots.get(&c.hash).cloned())
        .collect();
    let hotspots = match commits.first() {
        Some(newest) => hotspots(dir, page, &newest.hash)?,
        None => Vec::new(),
    };
    let agenda = commits
        .first()
        .map(|newest| agenda(dir, &newest.date))
        .transpose()?
        .unwrap_or_default();
    Ok(Stats {
        page,
        older,
        snapshots,
        agenda,
        hotspots,
        cache: cache_file,
    })
}

/// A commit as `git log` lists it.
struct Listed {
    hash: String,
    short: String,
    date: String,
    author: String,
    subject: String,
}

/// The commits of `page`, newest first, plus one more when older ones exist.
fn list(dir: &Path, page: usize) -> Result<Vec<Listed>, String> {
    let head = git(dir, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .map_err(|_| "This project has no commits yet.".to_string())?;
    let out = git(
        dir,
        &[
            "log",
            "--first-parent",
            &format!("--skip={}", page * PAGE),
            &format!("--max-count={}", PAGE + 1),
            "--format=%H%x1f%h%x1f%cI%x1f%an%x1f%s",
            head.trim(),
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(5, '\x1f');
            Some(Listed {
                hash: fields.next()?.to_string(),
                short: fields.next()?.to_string(),
                date: fields.next()?.to_string(),
                author: fields.next()?.to_string(),
                subject: fields.next().unwrap_or_default().to_string(),
            })
        })
        .collect())
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
    let tree = git(
        dir,
        &["ls-tree", "-r", "-z", "-l", "--full-tree", &commit.hash],
    )?;
    let mut snapshot = Snapshot {
        hash: commit.hash.clone(),
        short: commit.short.clone(),
        date: commit.date.clone(),
        author: commit.author.clone(),
        subject: commit.subject.clone(),
        ..Snapshot::default()
    };
    let mut languages: HashMap<&str, usize> = HashMap::new();
    let mut files = Vec::new();
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
        files.push((entry.path, m.code, m.complexity()));
    }
    snapshot.languages = top(
        languages.into_iter().map(|(k, v)| (k.to_string(), v)),
        usize::MAX,
    );
    snapshot.largest = top(files.iter().map(|f| (f.0.to_string(), f.1)), TOP);
    snapshot.complex = top(files.iter().map(|f| (f.0.to_string(), f.2)), TOP);
    Ok(snapshot)
}

fn add(snapshot: &mut Snapshot, m: &FileMetrics) {
    snapshot.files += 1;
    snapshot.code += m.code;
    snapshot.comments += m.comments;
    snapshot.blank += m.blank;
    snapshot.test_code += m.test_code;
    snapshot.tests += m.tests;
    snapshot.complexity += m.complexity();
    snapshot.units += m.units();
    snapshot.todos += m.todos;
    if m.code > LARGE_FILE {
        snapshot.large_files += 1;
    }
}

/// The `count` highest, highest first, ties by name.
fn top(items: impl Iterator<Item = (String, usize)>, count: usize) -> Vec<(String, usize)> {
    let mut items: Vec<_> = items.filter(|(_, n)| *n > 0).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(count);
    items
}

/// The page's most changed source files, weighed by their complexity at
/// `newest`.
fn hotspots(dir: &Path, page: usize, newest: &str) -> Result<Vec<Hotspot>, String> {
    let out = git(
        dir,
        &[
            "log",
            "--first-parent",
            &format!("--skip={}", page * PAGE),
            &format!("--max-count={PAGE}"),
            "--format=",
            "--name-only",
            newest,
        ],
    )?;
    let mut changes: HashMap<&str, usize> = HashMap::new();
    for path in out.lines().filter(|line| !line.is_empty()) {
        if language_of(path).is_some() && measure::is_own(path) {
            *changes.entry(path).or_default() += 1;
        }
    }
    let churned = top(
        changes.into_iter().map(|(k, v)| (k.to_string(), v)),
        HOTSPOTS * 4,
    );
    let mut batch = Batch::open(dir)?;
    let mut spots = Vec::new();
    for (path, changes) in churned {
        let Some(language) = language_of(&path) else {
            continue;
        };
        let Some(bytes) = batch.read(&format!("{newest}:{path}"))? else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let m = measure(&path, language, text);
        spots.push(Hotspot {
            path,
            changes,
            code: m.code,
            complexity: m.complexity(),
        });
    }
    spots.sort_by_key(|s| std::cmp::Reverse(s.changes * s.complexity));
    spots.truncate(HOTSPOTS);
    Ok(spots)
}

/// Commits per day over the [`AGENDA_DAYS`] up to `newest` (RFC 3339).
fn agenda(dir: &Path, newest: &str) -> Result<BTreeMap<String, usize>, String> {
    let end = chrono::DateTime::parse_from_rfc3339(newest).map_err(|err| err.to_string())?;
    let start = end - chrono::Duration::days(AGENDA_DAYS);
    let out = git(
        dir,
        &[
            "log",
            &format!("--since={}", start.to_rfc3339()),
            &format!("--until={}", end.to_rfc3339()),
            "--format=%cs",
            "HEAD",
        ],
    )?;
    let mut days = BTreeMap::new();
    for day in out.lines().filter(|line| !line.is_empty()) {
        *days.entry(day.to_string()).or_default() += 1;
    }
    Ok(days)
}

/// Where the measures of the repository holding `dir` are kept.
fn cache_file(dir: &Path) -> Option<PathBuf> {
    let common = git(dir, &["rev-parse", "--git-common-dir"]).ok()?;
    Some(dir.join(common.trim()).join("iboard").join("stats.json"))
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    snapshots: HashMap<String, Snapshot>,
}

impl Cache {
    fn read(file: &Path) -> Self {
        std::fs::read(file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Cache>(&bytes).ok())
            .filter(|cache| cache.version == CACHE_VERSION)
            .unwrap_or_default()
    }

    /// Failures are ignored: the measures are read again next time.
    fn write(&mut self, file: &Path) {
        self.version = CACHE_VERSION;
        let Some(parent) = file.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_ok()
            && let Ok(json) = serde_json::to_vec(self)
        {
            let _ = files::atomic_write(file, json);
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|err| format!("git: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// One `git cat-file --batch`, reading objects by name.
struct Batch {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Batch {
    fn open(dir: &Path) -> Result<Self, String> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("git: {err}"))?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return Err("git cat-file: no pipe".to_string());
        };
        Ok(Self {
            child,
            input,
            output: BufReader::new(output),
        })
    }

    /// The object's bytes, or `None` when it does not exist.
    fn read(&mut self, object: &str) -> Result<Option<Vec<u8>>, String> {
        let broken = |err: std::io::Error| format!("git cat-file: {err}");
        writeln!(self.input, "{object}").map_err(broken)?;
        self.input.flush().map_err(broken)?;
        let mut header = String::new();
        self.output.read_line(&mut header).map_err(broken)?;
        let header = header.trim_end();
        if header.ends_with(" missing") || header.ends_with(" ambiguous") {
            return Ok(None);
        }
        let size: usize = header
            .rsplit(' ')
            .next()
            .and_then(|size| size.parse().ok())
            .ok_or_else(|| format!("git cat-file: unexpected `{header}`"))?;
        let mut bytes = vec![0; size + 1];
        self.output.read_exact(&mut bytes).map_err(broken)?;
        bytes.pop();
        Ok(Some(bytes))
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q"]);
        dir
    }

    fn run(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }

    fn commit(dir: &Path, file: &str, text: &str, message: &str) {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        run(dir, &["add", "."]);
        run(dir, &["commit", "-q", "-m", message]);
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
        let progress = Progress::default();
        let stats = collect(dir.path(), 0, &progress, &Cancel::default()).unwrap();
        assert_eq!(progress.get(), (4, 4));
        let code: Vec<usize> = stats.snapshots.iter().map(|s| s.code).collect();
        assert_eq!(code, [1, 3, 5, 5]);
        let last = stats.snapshots.last().unwrap();
        assert_eq!((last.subject.as_str(), last.files), ("four", 2));
        assert_eq!((last.tests, last.test_code, last.todos), (1, 2, 1));
        assert_eq!(last.complexity, 3);
        assert!(!stats.older);
        assert_eq!(stats.agenda.values().sum::<usize>(), 4);
        assert_eq!(stats.hotspots[0].path, "src/lib.rs");
        assert_eq!(stats.hotspots[0].changes, 2);
        assert!(stats.cache.as_ref().unwrap().is_file());

        commit(dir.path(), "src/b.rs", "fn b() {}\n", "five");
        let progress = Progress::default();
        let again = collect(dir.path(), 0, &progress, &Cancel::default()).unwrap();
        assert_eq!(progress.get(), (1, 1), "only the new commit is read");
        assert_eq!(again.snapshots.len(), 5);
    }

    #[test]
    fn a_project_without_commits_says_so() {
        let dir = repo();
        let err = collect(dir.path(), 0, &Progress::default(), &Cancel::default()).unwrap_err();
        assert!(err.contains("no commits"), "{err}");
    }

    #[test]
    fn a_cancelled_run_stops() {
        let dir = repo();
        commit(dir.path(), "a.rs", "fn a() {}\n", "one");
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(collect(dir.path(), 0, &Progress::default(), &cancel).is_err());
    }
}
