//! What the blocks of the stats screen read from, each on its own job: every
//! part of a page is read at most once however many blocks ask, and a job
//! that waits takes over from one that stopped.
//!
//! [`Measures`] outlives a [`Source`], so a refresh or another page never
//! measures a commit twice, even while the job it replaced is still stopping.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use super::{
    Blobs, Cycles, FileStat, PAGE, Progress, Releases, STOPPED, Snapshot, Stats, cycle, files_at,
    release,
};
use crate::agenda::{self, AgendaDays, Repo};
use crate::files;
use crate::git::{self, Batch};
use crate::job::Cancel;

/// Bumped whenever the measures change, so older caches are read again.
const CACHE_VERSION: u32 = 1;

/// A part of the page a block needs in its [`Stats`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// The page's commits, without their measures, and whether older ones
    /// exist: as cheap as a `git log`.
    Commits,
    /// Each commit's measures, read only where the cache lacks them.
    Measures,
    Cycles,
    Files,
    Agenda,
    Releases,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A value computed once, by the first job that gets to it; the others wait
/// for it. A failure is not kept, so the next job tries again.
struct Memo<T>(Mutex<Option<Arc<T>>>);

impl<T> Default for Memo<T> {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

impl<T> Memo<T> {
    fn get(
        &self,
        cancel: &Cancel,
        make: impl FnOnce() -> Result<T, String>,
    ) -> Result<Arc<T>, String> {
        let mut slot = lock(&self.0);
        if let Some(value) = &*slot {
            return Ok(Arc::clone(value));
        }
        if cancel.is_cancelled() {
            return Err(STOPPED.to_string());
        }
        let value = Arc::new(make()?);
        *slot = Some(Arc::clone(&value));
        Ok(value)
    }
}

/// The commit cache of one repository, in memory and in
/// `.git/iboard/stats.json`. One job at a time measures.
pub struct Measures {
    dir: PathBuf,
    kept: Mutex<Option<Kept>>,
}

struct Kept {
    file: Option<PathBuf>,
    cache: Cache,
}

impl Measures {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            kept: Mutex::new(None),
        }
    }

    /// `commits`' measures, oldest first, reading only those the cache lacks.
    /// Stops early, with an error, once `cancel` is set; what was read by then
    /// stays cached.
    fn snapshots(
        &self,
        commits: &[Listed],
        progress: &Progress,
        cancel: &Cancel,
    ) -> Result<Vec<Snapshot>, String> {
        let mut kept = lock(&self.kept);
        let Kept { file, cache } = kept.get_or_insert_with(|| {
            let file = cache_file(&self.dir);
            let cache = file.as_deref().map(Cache::read).unwrap_or_default();
            Kept { file, cache }
        });
        let missing: Vec<&Listed> = commits
            .iter()
            .filter(|c| !cache.snapshots.contains_key(&c.hash))
            .collect();
        progress.done.store(0, Ordering::Relaxed);
        progress.total.store(missing.len(), Ordering::Relaxed);
        if !missing.is_empty() {
            let mut batch = Batch::open(&self.dir)?;
            let mut blobs = Blobs::new();
            for (at, commit) in missing.iter().enumerate() {
                if cancel.is_cancelled() {
                    break;
                }
                let snapshot = super::measure_commit(&self.dir, &mut batch, &mut blobs, commit)?;
                cache.snapshots.insert(commit.hash.clone(), snapshot);
                progress.done.store(at + 1, Ordering::Relaxed);
                if (at + 1).is_multiple_of(10)
                    && let Some(file) = &file
                {
                    cache.write(file);
                }
            }
            if let Some(file) = &file {
                cache.write(file);
            }
        }
        if cancel.is_cancelled() {
            return Err(STOPPED.to_string());
        }
        Ok(commits
            .iter()
            .rev()
            .filter_map(|c| cache.snapshots.get(&c.hash).cloned())
            .collect())
    }
}

/// One page of history as the blocks read it, shared by their jobs.
pub struct Source {
    dir: PathBuf,
    root: PathBuf,
    page: usize,
    release_tags: String,
    measures: Arc<Measures>,
    /// The projects the Agenda reads, the open one first.
    supervised: Vec<Repo>,
    /// How far the measuring got, for the UI.
    pub progress: Progress,
    listing: Memo<Listing>,
    snapshots: Memo<Vec<Snapshot>>,
    cycles: Memo<Cycles>,
    files: Memo<Vec<FileStat>>,
    agenda: Memo<AgendaDays>,
    releases: Memo<Releases>,
}

impl Source {
    /// Page `page` of the history of the repository holding `dir`, whose
    /// issues are kept under `root`. Reads nothing yet.
    pub fn new(dir: &Path, root: &Path, page: usize, measures: Arc<Measures>) -> Self {
        Self {
            dir: dir.to_path_buf(),
            root: root.to_path_buf(),
            page,
            release_tags: release::ReleasePrefs::default().tags,
            measures,
            supervised: vec![Repo::new(dir)],
            progress: Progress::default(),
            listing: Memo::default(),
            snapshots: Memo::default(),
            cycles: Memo::default(),
            files: Memo::default(),
            agenda: Memo::default(),
            releases: Memo::default(),
        }
    }

    /// The same source, reading the tags matching `pattern` as releases.
    pub fn with_release_tags(mut self, pattern: &str) -> Self {
        self.release_tags = pattern.to_string();
        self
    }

    /// The same source, its Agenda reading `repos` instead of its own folder
    /// alone.
    pub fn with_supervised(mut self, repos: Vec<Repo>) -> Self {
        self.supervised = repos;
        self
    }

    /// The projects the Agenda reads.
    pub fn supervised(&self) -> &[Repo] {
        &self.supervised
    }

    /// The pattern of the tags it reads as releases.
    pub fn release_tags(&self) -> &str {
        &self.release_tags
    }

    /// The folder its git commands run in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The same project at `page`, read afresh but for the commit cache.
    pub fn at_page(&self, page: usize) -> Self {
        Self::new(&self.dir, &self.root, page, Arc::clone(&self.measures))
            .with_release_tags(&self.release_tags)
            .with_supervised(self.supervised.clone())
    }

    /// The [`Stats`] with what `needs` asks for filled in and the rest empty.
    pub fn read(&self, needs: &[Need], cancel: &Cancel) -> Result<Stats, String> {
        let mut stats = Stats {
            page: self.page,
            ..Stats::default()
        };
        for need in needs {
            match need {
                Need::Commits => {
                    let listing = self.listing(cancel)?;
                    stats.older = listing.older;
                    stats.cache.clone_from(&listing.cache);
                    stats.snapshots = listing.commits.iter().rev().map(Listed::bare).collect();
                }
                Need::Measures => stats.snapshots.clone_from(&*self.snapshots(cancel)?),
                Need::Cycles => stats.cycles.clone_from(&*self.cycles(cancel)?),
                Need::Files => stats.files.clone_from(&*self.files(cancel)?),
                Need::Agenda => {
                    let agenda = self
                        .agenda
                        .get(cancel, || agenda::collect_all(&self.supervised, cancel))?;
                    stats.agenda.clone_from(&*agenda);
                }
                Need::Releases => stats.releases.clone_from(&*self.releases(cancel)?),
            }
        }
        Ok(stats)
    }

    fn listing(&self, cancel: &Cancel) -> Result<Arc<Listing>, String> {
        self.listing.get(cancel, || {
            let mut commits = list(&self.dir, self.page)?;
            if commits.is_empty() {
                return Err("No commits on this page.".to_string());
            }
            let older = commits.len() > PAGE;
            commits.truncate(PAGE);
            let cache = cache_file(&self.dir);
            Ok(Listing {
                commits,
                older,
                cache,
            })
        })
    }

    fn snapshots(&self, cancel: &Cancel) -> Result<Arc<Vec<Snapshot>>, String> {
        self.snapshots.get(cancel, || {
            let listing = self.listing(cancel)?;
            self.measures
                .snapshots(&listing.commits, &self.progress, cancel)
        })
    }

    fn cycles(&self, cancel: &Cancel) -> Result<Arc<Cycles>, String> {
        self.cycles.get(cancel, || {
            let listing = self.listing(cancel)?;
            cycle::collect(&self.dir, &self.root, listing.newest())
        })
    }

    fn releases(&self, cancel: &Cancel) -> Result<Arc<Releases>, String> {
        self.releases.get(cancel, || {
            let listing = self.listing(cancel)?;
            release::read(&self.dir, &self.release_tags, &listing.commits, cancel)
        })
    }

    fn files(&self, cancel: &Cancel) -> Result<Arc<Vec<FileStat>>, String> {
        self.files.get(cancel, || {
            let listing = self.listing(cancel)?;
            files_at(&self.dir, listing.newest(), cancel)
        })
    }
}

/// The commits of a page, newest first, never empty.
struct Listing {
    commits: Vec<Listed>,
    older: bool,
    cache: Option<PathBuf>,
}

impl Listing {
    fn newest(&self) -> &str {
        self.commits.first().map_or("HEAD", |c| &c.hash)
    }
}

/// A commit as `git log` lists it.
pub(super) struct Listed {
    pub(super) hash: String,
    pub(super) short: String,
    pub(super) date: String,
}

impl Listed {
    /// The commit, unmeasured.
    fn bare(&self) -> Snapshot {
        Snapshot {
            short: self.short.clone(),
            date: self.date.clone(),
            ..Snapshot::default()
        }
    }
}

/// The commits of `page`, newest first, plus one more when older ones exist.
fn list(dir: &Path, page: usize) -> Result<Vec<Listed>, String> {
    let head = git::output(dir, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .map_err(|_| "This project has no commits yet.".to_string())?;
    let out = git::output(
        dir,
        &[
            "log",
            "--first-parent",
            &format!("--skip={}", page * PAGE),
            &format!("--max-count={}", PAGE + 1),
            "--format=%H%x1f%h%x1f%cI",
            head.trim(),
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\x1f');
            Some(Listed {
                hash: fields.next()?.to_string(),
                short: fields.next()?.to_string(),
                date: fields.next()?.to_string(),
            })
        })
        .collect())
}

/// Where the measures of the repository holding `dir` are kept.
fn cache_file(dir: &Path) -> Option<PathBuf> {
    let common = git::output(dir, &["rev-parse", "--git-common-dir"]).ok()?;
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
        if let Ok(json) = serde_json::to_vec(self) {
            let _ = files::write_creating_last(file, json);
        }
    }
}
