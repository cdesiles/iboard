//! The AI build: Ready's issues handed to the build agents top to bottom,
//! each moved to the next column while an agent works on it. Doing's header
//! sets how many agents work at once.

use super::*;
use crate::build_log;
use std::collections::HashSet;

/// The most agents an AI build runs at once.
pub(super) const MAX_AGENTS: usize = 16;

/// The issues an AI build goes through, and the ones its agents are on.
pub(super) struct Build {
    /// The issues still to build, top first.
    queue: VecDeque<PathBuf>,
    /// The issues the agents work on, in their new column.
    runs: Vec<Run>,
    /// How many issues left the queue so far.
    taken: usize,
    total: usize,
    /// An agent failed: the rest of the queue was dropped.
    failed: bool,
}

impl Build {
    fn new(queue: Vec<PathBuf>) -> Self {
        Self {
            total: queue.len(),
            queue: queue.into(),
            runs: Vec::new(),
            taken: 0,
            failed: false,
        }
    }
}

/// Play stays pressed until Stop: an issue that arrives in Ready, dropped
/// there or written by another process, joins the build.
pub(super) struct Watch {
    /// Ready's issues when the board was last looked at.
    ready: HashSet<PathBuf>,
    /// The board generation `ready` was read from.
    generation: u64,
}

/// One agent at work on one issue.
struct Run {
    issue: PathBuf,
    answer: Job<Result<String, String>>,
    /// When the agent took `issue`.
    started: Instant,
    /// Where `issue` stands in the whole run, from 1.
    at: usize,
}

/// The AI build as the board shows it: the issues at work and since when,
/// and how many agents may work at once.
#[derive(Clone, Copy)]
pub(super) struct Builds<'a> {
    pub(super) running: &'a [(PathBuf, Instant)],
    pub(super) agents: usize,
    /// Play is pressed: Ready shows Stop even while no agent works.
    pub(super) watching: bool,
}

impl Builds<'_> {
    /// Play is pressed, or an agent is at work: Ready offers Stop.
    pub(super) fn active(&self) -> bool {
        self.watching || !self.running.is_empty()
    }

    /// An agent is free for a Doing card's retry.
    fn has_room(&self) -> bool {
        self.running.len() < self.agents
    }
}

/// Where `issue`, in `column`, stands with the AI build: at work, or a Doing
/// issue to retry while an agent is free.
pub(super) fn card_build(column: &Column, issue: &Issue, builds: Builds) -> Option<CardBuild> {
    if let Some((_, since)) = builds.running.iter().find(|(path, _)| *path == issue.path) {
        return Some(CardBuild::Running(*since));
    }
    (builds.has_room() && discovery::is_doing_column(&column.dir)).then_some(CardBuild::Retry)
}

/// The label that keeps an issue away from the build agent: work that is not
/// code, such as a quote or an email.
pub(super) const MANUAL: &str = "manual";

/// Whether Play hands `issue` to the build agent.
pub(super) fn buildable(issue: &Issue) -> bool {
    !issue.labels.iter().any(|label| label == MANUAL)
}

impl IboardApp {
    /// Play, on Ready's header: builds `paths` in order, as many at once as
    /// there are agents (one when none were left), leaving out the manual
    /// ones. Without a build agent set up, Settings › AI build opens instead.
    pub(super) fn start_build(&mut self, mut paths: Vec<PathBuf>) {
        if !self.build_agent.configured() {
            self.open_settings(settings_ui::Tab::Build);
            return;
        }
        paths.retain(|path| self.board.find(path).is_some_and(buildable));
        self.session.agents = self.session.agents.max(1);
        self.session.watch = Some(Watch {
            ready: self.ready_issues().into_iter().collect(),
            generation: self.board_generation,
        });
        self.session.build = Some(Build::new(paths));
        self.fill_agents();
    }

    fn ready_issues(&self) -> Vec<PathBuf> {
        self.board
            .columns
            .iter()
            .filter(|column| discovery::is_ready_column(&column.dir))
            .flat_map(|column| column.issues.iter().map(|issue| issue.path.clone()))
            .collect()
    }

    /// While Play is pressed, queues the issues that arrived in Ready since
    /// the board was last read, and hands them to the free agents.
    fn watch_ready(&mut self) {
        let generation = self.board_generation;
        let ready = self.ready_issues();
        let Some(watch) = self.session.watch.as_mut() else {
            return;
        };
        if watch.generation == generation {
            return;
        }
        watch.generation = generation;
        let arrived: Vec<PathBuf> = ready
            .iter()
            .filter(|path| !watch.ready.contains(*path))
            .filter(|path| self.board.find(path).is_some_and(buildable))
            .cloned()
            .collect();
        watch.ready = ready.into_iter().collect();
        if arrived.is_empty() {
            return;
        }
        let build = self
            .session
            .build
            .get_or_insert_with(|| Build::new(Vec::new()));
        for path in arrived {
            if !build.queue.contains(&path) {
                build.total += 1;
                build.queue.push_back(path);
            }
        }
        self.fill_agents();
    }

    /// Play is pressed: new Ready issues join the build.
    pub(super) fn watching_ready(&self) -> bool {
        self.session.watch.is_some()
    }

    /// Retry, on a Doing card: an agent starts `path` over where it is. The
    /// issue at work keeps its place in the run.
    pub(super) fn retry_build(&mut self, path: PathBuf) {
        if !self.build_agent.configured() {
            self.open_settings(settings_ui::Tab::Build);
            return;
        }
        let mut build = self
            .session
            .build
            .take()
            .unwrap_or_else(|| Build::new(Vec::new()));
        let at = match build.runs.iter().position(|run| run.issue == path) {
            Some(index) => build.runs.remove(index).at,
            None => {
                build.taken += 1;
                build.total += 1;
                build.taken
            }
        };
        let name = file_name(&path);
        match self.start_agent(path, at) {
            Some(run) => build.runs.push(run),
            None => {
                self.error = Some(format!(
                    "Couldn't build {name}: it is no longer on the board"
                ));
            }
        }
        if !build.runs.is_empty() {
            self.session.build = Some(build);
        }
    }

    /// The issues the agents work on, and since when.
    pub(super) fn building_now(&self) -> Vec<(PathBuf, Instant)> {
        self.session
            .build
            .iter()
            .flat_map(|build| &build.runs)
            .map(|run| (run.issue.clone(), run.started))
            .collect()
    }

    /// Since when an agent works on `path`, if one does.
    pub(super) fn building_since(&self, path: &Path) -> Option<Instant> {
        let build = self.session.build.as_ref()?;
        let run = build.runs.iter().find(|run| run.issue == path)?;
        Some(run.started)
    }

    /// The `−` and `+` on Doing's header: from none, which stops the build,
    /// to [`MAX_AGENTS`]. More agents take more of the queue at once; fewer
    /// let the ones at work finish.
    pub(super) fn set_agents(&mut self, agents: usize) {
        self.session.agents = agents.min(MAX_AGENTS);
        if self.session.agents == 0 {
            self.stop_build();
        } else {
            self.fill_agents();
        }
    }

    /// Stop, on Ready's header: every agent is stopped and the rest of the
    /// queue dropped. The issues they were on stay where they are.
    pub(super) fn stop_build(&mut self) {
        self.session.watch = None;
        if let Some(build) = self.session.build.take() {
            let what = match build.runs.as_slice() {
                [run] => file_name(&run.issue),
                runs => format!("{} issues", runs.len()),
            };
            self.session.notice = Some((format!("Stopped building {what}"), Instant::now()));
        }
    }

    /// Stop, on the card at work: its agent is stopped, the issue stays where
    /// it is. The last one stopped ends the build.
    pub(super) fn stop_agent(&mut self, path: &Path) {
        let Some(build) = self.session.build.as_mut() else {
            return;
        };
        build.runs.retain(|run| run.issue != path);
        if build.runs.is_empty() {
            self.session.build = None;
            self.session.watch = None;
        }
        let name = file_name(path);
        self.session.notice = Some((format!("Stopped building {name}"), Instant::now()));
    }

    /// Hands the queue's next issues still in Ready to the free agents, after
    /// moving each to the next column. A build nothing works on is over.
    fn fill_agents(&mut self) {
        let Some(mut build) = self.session.build.take() else {
            return;
        };
        while build.runs.len() < self.session.agents {
            let Some(path) = build.queue.pop_front() else {
                break;
            };
            build.taken += 1;
            let Some(moved) = self.take_for_build(&path) else {
                continue;
            };
            build.runs.extend(self.start_agent(moved, build.taken));
        }
        if !build.runs.is_empty() {
            self.session.build = Some(build);
        } else if build.total > 0 && !build.failed {
            let total = build.total;
            let noun = if total == 1 { "issue" } else { "issues" };
            self.session.notice = Some((format!("Built {total} {noun}"), Instant::now()));
        }
    }

    /// Starts an agent on `issue`, the `at`th of the run; `None` when the
    /// issue is no longer on the board.
    fn start_agent(&mut self, issue: PathBuf, at: usize) -> Option<Run> {
        let content = self.board.find(&issue)?.markdown.clone();
        let prompt = prompts::build_prompt(&self.relative(&issue), &content);
        let project = self.board.project_of(&issue).to_path_buf();
        let log = build_log::path(self.board.root_of(&issue), &issue);
        let (answer, _) = agent::start_build(&self.build_agent, &prompt, &issue, &project, &log);
        Some(Run {
            issue,
            answer,
            started: Instant::now(),
            at,
        })
    }

    /// Moves a Ready issue to the column after Ready, where an agent takes
    /// it; `None` when it left Ready since the build started.
    fn take_for_build(&mut self, path: &Path) -> Option<PathBuf> {
        let index = self.board.columns.iter().position(|column| {
            discovery::is_ready_column(&column.dir)
                && column.issues.iter().any(|issue| issue.path == path)
        })?;
        match self.board.columns.get(index + 1).map(|c| c.dir.clone()) {
            Some(next) => self.move_issue(path, &next),
            None => Some(path.to_path_buf()),
        }
    }

    /// Takes the agents' answers as they come in and hands the free agents
    /// the next issues; a failure drops the rest of the queue. Called every
    /// frame.
    pub(super) fn poll_build(&mut self, ctx: &egui::Context) {
        self.watch_ready();
        let Some(build) = self.session.build.as_mut() else {
            return;
        };
        let mut finished = Vec::new();
        build.runs.retain(|run| match run.answer.answer() {
            Some(result) => {
                finished.push((file_name(&run.issue), result));
                false
            }
            None => true,
        });
        if finished.is_empty() {
            ctx.request_repaint_after(job::POLL_EVERY);
            return;
        }
        for (name, result) in finished {
            if let Err(err) = result {
                build.queue.clear();
                build.failed = true;
                self.session.watch = None;
                self.error = Some(format!("Couldn't build {name}: {err}"));
            }
        }
        self.refresh();
        self.fill_agents();
    }

    /// Where the first issue at work stands in its run: its place, from 1,
    /// and the run's size.
    #[cfg(test)]
    pub(super) fn build_place(&self) -> Option<(usize, usize)> {
        let build = self.session.build.as_ref()?;
        let run = build.runs.first()?;
        Some((run.at, build.total))
    }
}
