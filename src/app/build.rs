//! The AI build: Ready's issues handed to the build agent one at a time, top
//! to bottom, each moved to the next column while the agent works on it.

use super::*;
use crate::build_log;

/// The issues an AI build goes through, and the one its agent is on.
pub(super) struct Build {
    /// The issues still to build, top first.
    queue: VecDeque<PathBuf>,
    /// The issue the agent works on, in its new column.
    current: PathBuf,
    answer: Job<Result<String, String>>,
    progress: agent::Progress,
    /// When the agent took `current`.
    started: Instant,
    /// Where `current` stands in the whole run, from 1.
    at: usize,
    total: usize,
}

/// Where `issue`, in `column`, stands with the AI build `build` (its issue
/// and start): the one at work, or a Doing issue to retry while none runs.
pub(super) fn card_build(
    column: &Column,
    issue: &Issue,
    build: Option<(&Path, Instant)>,
) -> Option<CardBuild> {
    match build {
        Some((path, since)) => (path == issue.path).then_some(CardBuild::Running(since)),
        None => discovery::is_doing_column(&column.dir).then_some(CardBuild::Retry),
    }
}

impl IboardApp {
    /// Play, on Ready's header: builds `paths` in order. Without a build
    /// agent set up, Settings › AI build opens instead.
    pub(super) fn start_build(&mut self, paths: Vec<PathBuf>) {
        if !self.build_agent.configured() {
            self.open_settings(settings_ui::Tab::Build);
            return;
        }
        let total = paths.len();
        self.next_build(paths.into(), 0, total);
    }

    /// Retry, on a Doing card: the agent starts `path` over where it is. The
    /// issue the agent was on keeps the rest of its run.
    pub(super) fn retry_build(&mut self, path: PathBuf) {
        if !self.build_agent.configured() {
            self.open_settings(settings_ui::Tab::Build);
            return;
        }
        let (queue, at, total) = match self.session.build.take() {
            Some(build) if build.current == path => (build.queue, build.at, build.total),
            _ => (VecDeque::new(), 1, 1),
        };
        let name = file_name(&path);
        if !self.hand_over(path, queue, at, total) {
            self.error = Some(format!(
                "Couldn't build {name}: it is no longer on the board"
            ));
        }
    }

    /// The issue the agent works on, and since when.
    pub(super) fn building_now(&self) -> Option<(&Path, Instant)> {
        let build = self.session.build.as_ref()?;
        Some((build.current.as_path(), build.started))
    }

    /// Stop, on Ready's header or the card at work: the agent is stopped and the rest of the
    /// queue dropped. The issue it was on stays where it is.
    pub(super) fn stop_build(&mut self) {
        if let Some(build) = self.session.build.take() {
            let name = file_name(&build.current);
            self.session.notice = Some((format!("Stopped building {name}"), Instant::now()));
        }
    }

    /// Hands the first issue of `queue` still in Ready to the agent, after
    /// moving it to the next column; `done` of `total` are behind.
    fn next_build(&mut self, mut queue: VecDeque<PathBuf>, mut done: usize, total: usize) {
        while let Some(path) = queue.pop_front() {
            done += 1;
            let Some(moved) = self.take_for_build(&path) else {
                continue;
            };
            if self.hand_over(moved, queue.clone(), done, total) {
                return;
            }
        }
        if total > 0 {
            let noun = if total == 1 { "issue" } else { "issues" };
            self.session.notice = Some((format!("Built {total} {noun}"), Instant::now()));
        }
    }

    /// Starts the agent on `issue`, the `at`th of `total`, with `queue` to
    /// follow; false when the issue is no longer on the board.
    fn hand_over(
        &mut self,
        issue: PathBuf,
        queue: VecDeque<PathBuf>,
        at: usize,
        total: usize,
    ) -> bool {
        let Some(content) = self.board.find(&issue).map(|issue| issue.markdown.clone()) else {
            return false;
        };
        let prompt = prompts::build_prompt(&self.relative(&issue), &content);
        let project = self.located.project_dir.clone();
        let log = build_log::path(&self.board.root, &issue);
        let (answer, progress) =
            agent::start_build(&self.build_agent, &prompt, &issue, &project, &log);
        self.session.build = Some(Build {
            queue,
            current: issue,
            answer,
            progress,
            started: Instant::now(),
            at,
            total,
        });
        true
    }

    /// Moves a Ready issue to the column after Ready, where the agent takes
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

    /// Takes the agent's answer once it is in and moves on to the next issue;
    /// a failure stops the run. Called every frame.
    pub(super) fn poll_build(&mut self, ctx: &egui::Context) {
        let Some(build) = self.session.build.as_ref() else {
            return;
        };
        let Some(result) = build.answer.answer() else {
            ctx.request_repaint_after(job::POLL_EVERY);
            return;
        };
        let Some(build) = self.session.build.take() else {
            return;
        };
        let name = file_name(&build.current);
        match result {
            Ok(_) => {
                self.refresh();
                self.next_build(build.queue, build.at, build.total);
            }
            Err(err) => {
                self.refresh();
                self.error = Some(format!("Couldn't build {name}: {err}"));
            }
        }
    }

    /// The status bar's line while building: the issue, its place in the
    /// run, the time spent on it, and the agent's latest step.
    pub(super) fn build_status(&self) -> Option<String> {
        let build = self.session.build.as_ref()?;
        let mut text = format!("Building {}", file_name(&build.current));
        if build.total > 1 {
            text.push_str(&format!(" ({}/{})", build.at, build.total));
        }
        text.push_str(&format!(" · {}", stopwatch(build.started.elapsed())));
        if let Some(step) = agent::step(&build.progress) {
            text.push_str(&format!(" · {step}"));
        }
        Some(text)
    }
}
