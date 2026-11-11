//! The AI build's queue: the Ready issues still to take, the agents working
//! on them, and the Ready issues that arrive while Play is pressed.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::service;
use crate::platform::job::Job;

/// The most agents an AI build runs at once.
pub const MAX_AGENTS: usize = super::MAX_AGENTS;

/// The label that keeps an issue away from the build agent: work that is not
/// code, such as a quote or an email.
const MANUAL: &str = "manual";

/// Whether Play hands an issue with `labels` to the build agent.
pub fn buildable(labels: &[String]) -> bool {
    !labels.iter().any(|label| label == MANUAL)
}

/// The issues an AI build goes through, and the ones its agents are on.
pub struct Build {
    /// The issues still to build, top first.
    queue: VecDeque<PathBuf>,
    /// The issues the agents work on, in their new column.
    pub runs: Vec<Run>,
    /// How many issues left the queue so far.
    taken: usize,
    total: usize,
    /// An agent failed: the rest of the queue was dropped.
    failed: bool,
}

impl Build {
    pub fn new(queue: Vec<PathBuf>) -> Self {
        Self {
            total: queue.len(),
            queue: queue.into(),
            runs: Vec::new(),
            taken: 0,
            failed: false,
        }
    }

    /// Adopts runs that outlived the window: they are the whole build, with
    /// no queue left.
    pub fn adopted(runs: Vec<Run>) -> Self {
        let total = runs.len();
        Self {
            queue: VecDeque::new(),
            runs,
            taken: total,
            total,
            failed: false,
        }
    }

    /// Follows an issue renamed from `old` to `new`, queued or at work.
    pub fn rename(&mut self, old: &Path, new: &Path) {
        let runs = self.runs.iter_mut().map(|run| &mut run.issue);
        for issue in self.queue.iter_mut().chain(runs) {
            if issue == old {
                *issue = new.to_path_buf();
            }
        }
    }

    /// Lets go of an issue that left the board: it is no longer queued, its
    /// agent stops, and the run no longer counts it.
    pub fn forget(&mut self, path: &Path) {
        let queued = self.queue.len();
        self.queue.retain(|issue| issue != path);
        self.total -= queued - self.queue.len();
        if let Some(index) = self.runs.iter().position(|run| run.issue == path) {
            let run = self.runs.remove(index);
            run.stop();
            let at = run.at;
            self.taken = self.taken.saturating_sub(1);
            self.total = self.total.saturating_sub(1);
            for run in self.runs.iter_mut().filter(|run| run.at > at) {
                run.at -= 1;
            }
        }
    }

    /// Adds an issue that arrived in Ready while Play is pressed, once.
    pub fn enqueue(&mut self, path: PathBuf) {
        if !self.queue.contains(&path) {
            self.total += 1;
            self.queue.push_back(path);
        }
    }

    /// The next issue to build, top first.
    pub fn take(&mut self) -> Option<PathBuf> {
        self.queue.pop_front()
    }

    /// An issue left Ready since the build started: it no longer counts.
    pub fn skip(&mut self) {
        self.total -= 1;
    }

    /// Where the next issue to take stands in the run, from 1.
    pub fn place(&mut self) -> usize {
        self.taken += 1;
        self.taken
    }

    /// The place of the run an issue already has, or the next one; a retry
    /// keeps the issue's place in the run.
    pub fn retry(&mut self, path: &Path) -> usize {
        match self.runs.iter().position(|run| run.issue == path) {
            Some(index) => self.runs.remove(index).at,
            None => {
                self.taken += 1;
                self.total += 1;
                self.taken
            }
        }
    }

    /// An agent failed: the rest of the queue is dropped.
    pub fn fail(&mut self) {
        self.queue.clear();
        self.failed = true;
    }

    /// The number of issues built when nothing is left to build, unless a
    /// failure already ended the run.
    pub fn done(&self) -> Option<usize> {
        (self.runs.is_empty() && self.total > 0 && !self.failed).then_some(self.total)
    }

    pub fn total(&self) -> usize {
        self.total
    }
}

/// Play stays pressed until Stop: an issue that arrives in Ready, dropped
/// there or written by another process, joins the build.
pub struct Watch {
    /// Ready's issues when the board was last looked at.
    ready: HashSet<PathBuf>,
    /// The board generation `ready` was read from.
    generation: u64,
}

impl Watch {
    pub fn new(ready: HashSet<PathBuf>, generation: u64) -> Self {
        Self { ready, generation }
    }

    /// Follows an issue renamed from `old` to `new`: a Ready issue
    /// renumbered does not arrive again.
    pub fn rename(&mut self, old: &Path, new: &Path) {
        if self.ready.remove(old) {
            self.ready.insert(new.to_path_buf());
        }
    }

    /// The Ready issues that arrived since the board was last read, once per
    /// generation.
    pub fn arrivals(&mut self, generation: u64, ready: Vec<PathBuf>) -> Vec<PathBuf> {
        if self.generation == generation {
            return Vec::new();
        }
        self.generation = generation;
        let arrived = ready
            .iter()
            .filter(|path| !self.ready.contains(*path))
            .cloned()
            .collect();
        self.ready = ready.into_iter().collect();
        arrived
    }
}

/// One agent at work on one issue.
pub struct Run {
    pub issue: PathBuf,
    pub answer: Job<Result<String, String>>,
    /// The project the agent works in, and the service session it lives in
    /// when it runs there rather than in a child process.
    pub project: PathBuf,
    pub session: service::Handle,
    /// When the agent took `issue`.
    pub started: Instant,
    /// Where `issue` stands in the whole run, from 1.
    pub at: usize,
}

impl Run {
    /// Stops the agent, wherever it runs: the service is told to interrupt,
    /// a child process is stopped when its job drops.
    pub fn stop(&self) {
        if let (Some(service), Some(session)) =
            (self.session.service(), service::session_of(&self.session))
        {
            service.interrupt(&self.project, &session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    fn run(issue: &str, at: usize) -> Run {
        Run {
            issue: path(issue),
            answer: Job::done(Ok(String::new())),
            project: path("/project"),
            session: service::Handle::default(),
            started: Instant::now(),
            at,
        }
    }

    #[test]
    fn a_build_takes_its_issues_in_order_and_counts_them() {
        let mut build = Build::new(vec![path("a.md"), path("b.md")]);
        assert_eq!(build.total(), 2);
        assert_eq!(build.take(), Some(path("a.md")));
        assert_eq!(build.place(), 1);
        assert_eq!(build.take(), Some(path("b.md")));
        assert_eq!(build.place(), 2);
        assert_eq!(build.take(), None);
        assert_eq!(build.done(), Some(2));
    }

    #[test]
    fn a_skipped_issue_no_longer_counts() {
        let mut build = Build::new(vec![path("a.md")]);
        assert_eq!(build.take(), Some(path("a.md")));
        build.skip();
        assert_eq!(build.done(), None);
    }

    #[test]
    fn an_arriving_ready_issue_joins_the_queue_once() {
        let mut build = Build::new(Vec::new());
        build.enqueue(path("a.md"));
        build.enqueue(path("a.md"));
        assert_eq!(build.take(), Some(path("a.md")));
        assert_eq!(build.take(), None);
        assert_eq!(build.done(), Some(1));
    }

    #[test]
    fn a_failure_drops_the_rest_of_the_queue() {
        let mut build = Build::new(vec![path("a.md"), path("b.md")]);
        assert_eq!(build.take(), Some(path("a.md")));
        build.fail();
        assert_eq!(build.take(), None);
        assert_eq!(build.done(), None, "a failed run does not report a build");
    }

    #[test]
    fn a_renumbered_issue_keeps_its_place_in_the_queue_and_the_run() {
        let mut build = Build::new(vec![path("old.md")]);
        build.runs.push(run("old.md", 1));
        build.rename(Path::new("old.md"), Path::new("new.md"));
        assert_eq!(build.take(), Some(path("new.md")));
        assert_eq!(build.runs[0].issue, path("new.md"));
    }

    #[test]
    fn forgetting_an_issue_takes_it_out_of_the_queue_and_the_run() {
        let mut build = Build::new(vec![path("queued.md")]);
        build.taken = 2;
        build.total = 3;
        build.runs.push(run("working.md", 1));
        build.runs.push(run("kept.md", 2));
        build.forget(Path::new("queued.md"));
        assert_eq!(build.take(), None);
        assert_eq!(build.total(), 2);
        build.forget(Path::new("working.md"));
        assert_eq!(build.runs.len(), 1);
        assert_eq!(build.runs[0].at, 1);
        assert_eq!(build.total(), 1);
    }

    #[test]
    fn a_retry_keeps_the_place_of_an_issue_already_at_work() {
        let mut build = Build::new(Vec::new());
        build.runs.push(run("a.md", 1));
        assert_eq!(build.retry(Path::new("a.md")), 1);
        assert!(build.runs.is_empty());
        assert_eq!(build.retry(Path::new("b.md")), 1);
        assert_eq!(build.total(), 1);
    }

    #[test]
    fn watching_ready_returns_only_the_issues_that_arrived() {
        let mut watch = Watch::new(HashSet::from([path("a.md")]), 1);
        assert!(watch.arrivals(1, vec![path("a.md")]).is_empty());
        assert_eq!(
            watch.arrivals(2, vec![path("a.md"), path("b.md")]),
            [path("b.md")]
        );
        assert!(
            watch
                .arrivals(2, vec![path("a.md"), path("b.md")])
                .is_empty()
        );
    }

    #[test]
    fn manual_issues_stay_out_of_the_build() {
        assert!(buildable(&["code".to_string()]));
        assert!(!buildable(&["code".to_string(), MANUAL.to_string()]));
    }
}
