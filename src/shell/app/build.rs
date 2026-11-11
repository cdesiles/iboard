//! The AI build: Ready's issues handed to the build agents top to bottom,
//! each moved to the next column while an agent works on it. Doing's header
//! sets how many agents work at once.

use std::collections::HashMap;

use super::card::CardBuild;
use super::{
    AdoptedBuilds, Column, Duration, IboardApp, Instant, Issue, Path, PathBuf, agent, discovery,
    file_name, job, prompts, settings, settings_ui,
};
use crate::agent::build::{self as queue, Run};
pub(super) use crate::agent::build::{Build, MAX_AGENTS, Watch};
use crate::build_log;
use crate::domains::board::lifecycle;
use crate::job::Job;
use crate::reviews;

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

    /// An agent is free for a Doing card's retry. With none left the build
    /// is stopped, and a retry starts it again with one, as Play does.
    fn has_room(&self) -> bool {
        self.running.len() < self.agents.max(1)
    }
}

/// Where `issue`, in `column`, stands with the AI build: at work, or a Doing
/// issue to retry while an agent is free, unless it is manual.
pub(super) fn card_build(column: &Column, issue: &Issue, builds: Builds) -> Option<CardBuild> {
    if let Some((_, since)) = builds.running.iter().find(|(path, _)| *path == issue.path) {
        return Some(CardBuild::Running(*since));
    }
    let retry = builds.has_room() && discovery::is_doing_column(&column.dir) && buildable(issue);
    retry.then_some(CardBuild::Retry)
}

/// Whether Play hands `issue` to the build agent.
pub(super) fn buildable(issue: &Issue) -> bool {
    queue::buildable(&issue.labels)
}

impl IboardApp {
    /// Play, on Ready's header: builds `paths` in order, as many at once as
    /// there are agents (one when none were left), leaving out the manual
    /// ones. Without a build agent set up, Settings › AI build opens instead.
    pub(super) fn start_build(&mut self, mut paths: Vec<PathBuf>) {
        if !self.prefs.build_agent.configured() {
            self.open_settings(settings_ui::Tab::Build);
            return;
        }
        paths.retain(|path| self.board.find(path).is_some_and(buildable));
        self.session.agents = self.session.agents.max(1);
        self.session.watch = Some(Watch::new(
            self.ready_issues().into_iter().collect(),
            self.board_generation,
        ));
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

    /// The builds that stop with the app: the ones still run as a child
    /// process. A build in the opencode service outlives the window, so
    /// quitting leaves it alone and asks nothing.
    pub(super) fn builds_at_risk(&self) -> usize {
        self.session
            .build
            .iter()
            .flat_map(|build| &build.runs)
            .filter(|run| agent::service::session_of(&run.session).is_none())
            .count()
    }

    /// Finds the builds the service is still running for this board, once
    /// per board opened, off the UI thread: they outlived the window closing,
    /// and the cards keep their bar and their clock. A group's board asks
    /// each of its projects.
    pub(super) fn adopt_builds(&mut self) -> Option<Duration> {
        if self.session.adopted {
            return None;
        }
        if self.session.scan.is_some() {
            return Some(job::POLL_EVERY);
        }
        if self.session.adoption.is_none() {
            let projects = self.build_projects();
            let services: Vec<&'static dyn agent::service::Service> = agent::services().collect();
            let look = Job::spawn(move |cancel| {
                let mut found: AdoptedBuilds = Vec::new();
                for service in services {
                    if cancel.is_cancelled() {
                        break;
                    }
                    for project in &projects {
                        if cancel.is_cancelled() {
                            break;
                        }
                        if let Ok(hits) = agent::service::running(service, project) {
                            found.extend(hits.into_iter().map(|hit| (project.clone(), hit)));
                        }
                    }
                }
                Ok::<_, String>(found)
            });
            self.session.adoption = Some(look);
        }
        let Some(answer) = self
            .session
            .adoption
            .as_ref()
            .and_then(|look| look.answer())
        else {
            return Some(job::POLL_EVERY);
        };
        self.session.adoption = None;
        self.session.adopted = true;
        let Ok(found) = answer else {
            return None;
        };
        let mut runs: Vec<Run> = Vec::new();
        for (project, hit) in found {
            let Some(issue) = self.issue_with_slug(&project, &hit.slug) else {
                continue;
            };
            let log = build_log::path(self.board.root_of(&issue), &issue);
            let session = agent::service::Handle::new(Some(hit.service));
            agent::service::remember(&session, &hit.session);
            let answer = agent::service::watch(hit.service, project.clone(), hit.session, log);
            runs.push(Run {
                issue,
                answer,
                project,
                session,
                started: agent::service::started(hit.elapsed),
                at: runs.len() + 1,
            });
        }
        if runs.is_empty() {
            return None;
        }
        self.session.agents = self.session.agents.max(runs.len());
        self.session.build = Some(Build::adopted(runs));
        None
    }

    /// The projects adoption asks the service about: each of a group board's,
    /// or the open project's.
    pub(super) fn build_projects(&self) -> Vec<PathBuf> {
        match self.board.repos.as_slice() {
            [] => vec![self.located.project_dir.clone()],
            repos => repos.iter().map(|repo| repo.project_dir.clone()).collect(),
        }
    }

    /// The issue whose file has `slug` in `project`, wherever it stands on
    /// the board.
    fn issue_with_slug(&self, project: &Path, slug: &str) -> Option<PathBuf> {
        self.board
            .columns
            .iter()
            .flat_map(|column| &column.issues)
            .map(|issue| issue.path.clone())
            .find(|path| self.board.project_of(path) == project && reviews::slug(path) == slug)
    }

    /// While Play is pressed, queues the issues that arrived in Ready since
    /// the board was last read, and hands them to the free agents.
    fn watch_ready(&mut self) {
        let generation = self.board_generation;
        let ready = self.ready_issues();
        let Some(watch) = self.session.watch.as_mut() else {
            return;
        };
        let arrived: Vec<PathBuf> = watch
            .arrivals(generation, ready)
            .into_iter()
            .filter(|path| self.board.find(path).is_some_and(buildable))
            .collect();
        if arrived.is_empty() {
            return;
        }
        let build = self
            .session
            .build
            .get_or_insert_with(|| Build::new(Vec::new()));
        for path in arrived {
            build.enqueue(path);
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
        if !self.board.find(&path).is_some_and(buildable) {
            return;
        }
        if !self.prefs.build_agent.configured() {
            self.open_settings(settings_ui::Tab::Build);
            return;
        }
        self.session.agents = self.session.agents.max(1);
        let mut build = self
            .session
            .build
            .take()
            .unwrap_or_else(|| Build::new(Vec::new()));
        let at = build.retry(&path);
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

    /// Starts following the service of each project a build runs in, and
    /// stops following the ones no run uses any more.
    fn follow_build_questions(&mut self) {
        let projects: HashMap<(&'static str, PathBuf), &'static dyn agent::service::Service> = self
            .session
            .build
            .iter()
            .flat_map(|build| &build.runs)
            .filter_map(|run| {
                let service = run.session.service()?;
                agent::service::session_of(&run.session)?;
                Some(((service.key(), run.project.clone()), service))
            })
            .collect();
        self.session
            .feeds
            .retain(|key, _| projects.contains_key(key));
        for (key, service) in projects {
            let project = key.1.clone();
            self.session
                .feeds
                .entry(key)
                .or_insert_with(|| agent::service::follow_questions(service, project));
        }
    }

    /// The questions the service waits on, each with the issue its build is
    /// on.
    pub(super) fn pending_questions(&self) -> Vec<(PathBuf, agent::service::Question)> {
        self.pending_questions_since()
            .into_iter()
            .map(|(path, question, _)| (path, question))
            .collect()
    }

    /// The questions the service waits on, with when the build took its issue:
    /// the tray lists them oldest first.
    pub(super) fn pending_questions_since(
        &self,
    ) -> Vec<(PathBuf, agent::service::Question, Instant)> {
        let Some(build) = self.session.build.as_ref() else {
            return Vec::new();
        };
        let mut pending = Vec::new();
        for run in &build.runs {
            let Some(service) = run.session.service() else {
                continue;
            };
            let Some(session) = agent::service::session_of(&run.session) else {
                continue;
            };
            let Some((_, feed)) = self
                .session
                .feeds
                .get(&(service.key(), run.project.clone()))
            else {
                continue;
            };
            for question in feed
                .questions()
                .into_iter()
                .filter(|question| question.session == session)
            {
                pending.push((run.issue.clone(), question, run.started));
            }
        }
        pending
    }

    /// The project whose service waits on `question`.
    pub(super) fn question_project(&self, question: &agent::service::Question) -> Option<PathBuf> {
        self.session
            .build
            .as_ref()?
            .runs
            .iter()
            .find(|run| {
                run.session.service().map(|service| service.key()) == Some(question.service.key())
                    && agent::service::session_of(&run.session).as_deref()
                        == Some(question.session.as_str())
            })
            .map(|run| run.project.clone())
    }

    /// The question the build on `path` waits on, if it does.
    pub(super) fn question_for(&self, path: &Path) -> Option<agent::service::Question> {
        self.pending_questions()
            .into_iter()
            .find(|(issue, _)| issue == path)
            .map(|(_, question)| question)
    }

    /// Forgets a question just answered.
    pub(super) fn forget_question(&self, question: &agent::service::Question) {
        for (_, feed) in self.session.feeds.values() {
            feed.remove(&question.session, &question.id);
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

    /// The project and service session of the run on `path`, when it runs in
    /// a service rather than as a child process.
    pub(super) fn build_service_run(
        &self,
        path: &Path,
    ) -> Option<(PathBuf, &'static dyn agent::service::Service, String)> {
        let build = self.session.build.as_ref()?;
        let run = build.runs.iter().find(|run| run.issue == path)?;
        let service = run.session.service()?;
        let session = agent::service::session_of(&run.session)?;
        Some((run.project.clone(), service, session))
    }

    /// The `−` and `+` on Doing's header: from none, which stops the build,
    /// to [`MAX_AGENTS`]. More agents take more of the queue at once; fewer
    /// let the ones at work finish. The count is the user's: it is saved in
    /// `[build] agents` and Settings › AI build shows it.
    pub(super) fn set_agents(&mut self, agents: usize) {
        self.apply_agents(agents);
        self.prefs.agents = self.session.agents;
        self.remember_section(settings::Section::partial(
            "build",
            &agent::agents_section(self.prefs.agents),
        ));
    }

    /// Applies a count from the header or from Settings, without saving it.
    pub(super) fn apply_agents(&mut self, agents: usize) {
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
            for run in &build.runs {
                run.stop();
            }
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
        if let Some(run) = build.runs.iter().find(|run| run.issue == path) {
            run.stop();
        }
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
            let Some(path) = build.take() else {
                break;
            };
            let Some(moved) = self.take_for_build(&path) else {
                build.skip();
                continue;
            };
            let at = build.place();
            build.runs.extend(self.start_agent(moved, at));
        }
        if !build.runs.is_empty() {
            self.session.build = Some(build);
        } else if let Some(total) = build.done() {
            let noun = if total == 1 { "issue" } else { "issues" };
            self.session.notice = Some((format!("Built {total} {noun}"), Instant::now()));
        }
    }

    /// Starts an agent on `issue`, the `at`th of the run; `None` when the
    /// issue is no longer on the board.
    fn start_agent(&mut self, issue: PathBuf, at: usize) -> Option<Run> {
        let content = self.board.find(&issue)?.markdown.clone();
        let prompt = prompts::build_prompt(&self.project_relative(&issue), &content);
        let project = self.board.project_of(&issue).to_path_buf();
        let log = build_log::path(self.board.root_of(&issue), &issue);
        let (answer, _, session) = agent::start_build(
            &self.prefs.build_agent,
            &prompt,
            &issue,
            &project,
            &log,
            self.prefs.build_timeout,
        );
        Some(Run {
            issue,
            answer,
            project,
            session,
            started: Instant::now(),
            at,
        })
    }

    /// Moves a Ready issue to the column after Ready, where an agent takes
    /// it; `None` when it left Ready since the build started.
    fn take_for_build(&mut self, path: &Path) -> Option<PathBuf> {
        let column = self.board.column_of(path)?;
        if !discovery::is_ready_column(&column.dir) {
            return None;
        }
        let next = lifecycle::next_column(&self.board, path).map(Path::to_path_buf);
        match next {
            Some(next) => self.move_issue(path, &next),
            None => Some(path.to_path_buf()),
        }
    }

    /// Takes the agents' answers as they come in and hands the free agents
    /// the next issues; a failure drops the rest of the queue. Called every
    /// frame.
    pub(super) fn poll_build(&mut self) -> Option<Duration> {
        self.watch_ready();
        let adopting = self.adopt_builds();
        self.follow_build_questions();
        let Some(build) = self.session.build.as_mut() else {
            return adopting;
        };
        let mut finished = Vec::new();
        build.runs.retain(|run| match run.answer.answer() {
            Some(result) => {
                finished.push((run.issue.clone(), result));
                false
            }
            None => true,
        });
        if finished.is_empty() && !build.runs.is_empty() {
            return Some(job::POLL_EVERY);
        }
        for (path, result) in finished {
            if let Err(err) = result {
                let name = file_name(&path);
                build.fail();
                self.session.watch = None;
                self.error = Some(format!("Couldn't build {name}: {err}"));
            }
        }
        self.refresh();
        self.fill_agents();
        self.session.build.is_some().then_some(job::POLL_EVERY)
    }

    /// Puts a build at work on `path` with a service waiting on `question`,
    /// without starting a service.
    #[cfg(test)]
    pub(super) fn seed_question(&mut self, path: PathBuf, question: agent::service::Question) {
        let project = self.board.project_of(&path).to_path_buf();
        let service = question.service;
        let session = agent::service::Handle::new(Some(service));
        agent::service::remember(&session, &question.session);
        let run = Run {
            issue: path,
            answer: Job::spawn(|cancel| {
                while !cancel.is_cancelled() {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Ok(String::new())
            }),
            project: project.clone(),
            session,
            started: Instant::now(),
            at: 1,
        };
        self.session.build = Some(Build::adopted(vec![run]));
        self.session.adopted = true;
        self.session.feeds.insert(
            (service.key(), project),
            (Job::done(()), agent::service::Feed::new(vec![question])),
        );
    }

    /// Where the first issue at work stands in its run: its place, from 1,
    /// and the run's size.
    #[cfg(test)]
    pub(super) fn build_place(&self) -> Option<(usize, usize)> {
        let build = self.session.build.as_ref()?;
        let run = build.runs.first()?;
        Some((run.at, build.total()))
    }
}
