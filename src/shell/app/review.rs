//! The AI review: running it and applying its proposal. Its state lives in
//! `model`, its panel in `panel`.

use super::side_panel::side_panel;
use super::split::{self, Split};
use super::{
    Align, ButtonKind, Color32, CommonMarkCache, CommonMarkViewer, Duration, Frame, HashMap,
    IboardApp, Job, Layout, Palette, Path, PathBuf, ReviewMode, RichText, Screen, Vec2, agent,
    file_name, job, prompts, reviews, theme, widgets,
};
pub(super) use crate::reviews::PastReview;

mod model;
mod panel;

pub(super) use model::{
    CardReviews, REVIEW_KEY, REVIEW_RUNNING, ReviewPill, ReviewRun, ReviewRuns, ReviewState,
};
#[cfg(test)]
pub(super) use model::{card_pill, past_pill};
#[cfg(test)]
pub(super) use panel::PLAN_NOTE;

impl IboardApp {
    /// The text an agent should read: the saved file, or the editor draft when
    /// the review is of the current work.
    pub(super) fn review_content(&self, path: &Path, draft: bool) -> Option<String> {
        if draft {
            self.session
                .editor
                .as_ref()
                .filter(|editor| editor.path == path)
                .map(|editor| editor.text.clone())
        } else {
            self.board.find(path).map(|issue| issue.markdown.clone())
        }
    }

    /// Hands the prompt to the agent in the background and returns at once:
    /// the issue's card and reader show the run until its answer lands.
    pub(super) fn run_review(&mut self, target: ReviewTarget, content: &str, prompt: String) {
        if !self.prefs.agent.configured() {
            self.open_config();
            return;
        }
        let project = Some(self.board.project_of(&target.path));
        let (answer, progress) = agent::start_plan_review(
            &self.prefs.agent,
            &prompt,
            &target.path,
            project,
            target.mode,
        );
        self.mark_notes_sent(&target.path);
        self.close_review();
        let runs = &mut self.session.review_runs;
        runs.ready.remove(&target.path);
        runs.running.insert(
            target.path,
            ReviewRun {
                draft: target.draft,
                mode: target.mode,
                agent: self.prefs.agent.label(),
                hash: reviews::content_hash(content),
                answer,
                progress,
            },
        );
    }

    /// Starts a review of the open issue (or the editor draft), in the scope
    /// Settings › AI review chose.
    pub(super) fn start_review(&mut self, path: &Path, draft: bool) {
        self.start_review_in(path, draft, self.prefs.agent.scope);
    }

    /// The text an agent reads of `path` (see [`Self::review_content`]), and
    /// the prompt `build` makes of it and the reviewer's notes.
    pub(super) fn prompt_for(
        &mut self,
        path: &Path,
        draft: bool,
        build: impl FnOnce(&str, &[(String, String)]) -> String,
    ) -> Option<(String, String)> {
        let content = self.review_content(path, draft)?;
        let notes = self.reviewer_notes(path);
        let prompt = build(&content, &notes);
        Some((content, prompt))
    }

    /// Runs the prompt `build` makes for `target`, unless one runs on it.
    pub(super) fn review_with(
        &mut self,
        target: ReviewTarget,
        build: impl FnOnce(&str, &[(String, String)]) -> String,
    ) {
        if self.review_is_running(&target.path) {
            return;
        }
        if let Some((content, prompt)) = self.prompt_for(&target.path, target.draft, build) {
            self.run_review(target, &content, prompt);
        }
    }

    /// Starts a one-shot review in `mode`.
    pub(super) fn start_review_in(&mut self, path: &Path, draft: bool, mode: ReviewMode) {
        if !self.is_todo_issue(path) {
            return;
        }
        let target = ReviewTarget {
            path: path.to_path_buf(),
            draft,
            mode,
        };
        self.review_with(target, |content, notes| {
            prompts::review_prompt(content, mode, notes)
        });
    }

    /// Sends a follow-up ("make it smaller") with the previous answer attached.
    pub(super) fn ask_again(&mut self) {
        let Some(review) = self.session.review.as_ref() else {
            return;
        };
        let (draft, mode) = (review.review.draft, review.review.mode);
        let target = ReviewTarget {
            path: review.path.clone(),
            draft,
            mode,
        };
        let follow_up = review.follow_up.as_deref().unwrap_or("").trim().to_string();
        if follow_up.is_empty() {
            return;
        }
        let previous = review.review.raw.clone();
        self.review_with(target, |content, notes| {
            let review = prompts::review_prompt(content, mode, notes);
            format!("{review}\n\nA previous review said:\n{previous}\n\nFollow-up: {follow_up}")
        });
    }

    /// Closes the review panel; the review stays behind the card's pill. A
    /// refinement, in the same place, goes with it. A review running in the
    /// background goes on.
    pub(super) fn close_review(&mut self) {
        self.session.review = None;
        self.session.refine = None;
    }

    /// Stops the review running on `path`: dropping its job stops the agent.
    pub(super) fn cancel_review_run(&mut self, path: &Path) {
        self.session.review_runs.forget(path);
    }

    /// The card's review pill: opens the issue with its last review, which
    /// is no longer news. While one runs, the reader shows it running.
    pub(super) fn show_past_review(&mut self, path: PathBuf) {
        self.open_issue(path.clone());
        self.reveal_review(&path);
    }

    /// Opens the review panel on the last review of the open issue `path`,
    /// which is no longer news.
    pub(super) fn reveal_review(&mut self, path: &Path) {
        self.session.review_runs.ready.remove(path);
        if self.review_is_running(path) {
            return;
        }
        let path = path.to_path_buf();
        if let Some(review) = self.past_reviews.get(&path).cloned() {
            self.session.review = Some(ReviewState {
                path,
                review,
                split: None,
                follow_up: None,
            });
        }
    }

    /// Adopts the answers that are in, kept beside their issue and marked
    /// ready; a failure goes to the status strip and leaves the last review
    /// alone. Called every frame.
    pub(super) fn poll_review(&mut self) -> Option<Duration> {
        let landed = job::take_landed(&mut self.session.review_runs.running, |run| &run.answer);
        for (path, run, result) in landed {
            let reviewed = match result {
                Ok(reviewed) => reviewed,
                Err(error) => {
                    self.error = Some(job::failure("Review of", &path, &error));
                    continue;
                }
            };
            let stored =
                reviews::StoredReview::landed(run.hash, run.agent, run.mode, run.draft, reviewed);
            if let Err(err) = reviews::save_review(self.board.root_of(&path), &path, stored.clone())
            {
                self.error = Some(err);
            }
            if let Some(issue) = self.board.find(&path) {
                let past = PastReview::new(&stored, &issue.markdown);
                self.past_reviews.insert(path.clone(), past);
            }
            self.session.review_runs.ready.insert(path.clone());
            // The issue being read shows its answer at once.
            let reading = self.session.reading() == Some(path.as_path())
                && self.session.review.is_none()
                && !self.refinement_shown();
            if reading {
                self.reveal_review(&path);
            }
        }
        self.session
            .review_runs
            .any_running()
            .then_some(job::POLL_EVERY)
    }

    /// Whether the review panel shows a proposal `⌘↩` would apply: it is on
    /// screen, beside the issue it reviews.
    pub(super) fn proposal_shown(&self) -> bool {
        let Screen::Detail(open, _) = &self.session.screen else {
            return false;
        };
        self.session
            .review
            .as_ref()
            .is_some_and(|review| review.path == *open && review.proposal().is_some())
    }

    /// Whether a review of `path` is running in the background.
    pub(super) fn review_is_running(&self, path: &Path) -> bool {
        self.session.review_runs.is_running(path)
    }

    /// The proposed rewrite, when the agent offered one: the editor opens with
    /// it as an unsaved draft, so saving or discarding is the accept or refuse.
    /// Over unsaved text the editor asks first, the panel staying to read.
    pub(super) fn apply_review_proposal(&mut self) {
        let Some(review) = self.session.review.as_ref() else {
            return;
        };
        let path = review.path.clone();
        let Some(proposal) = review.proposal().map(str::to_string) else {
            return;
        };
        self.open_editor(&path);
        if self.offer_to_editor(&path, proposal) {
            self.session.review = None;
        }
    }
}

/// What a review reads, and how far it goes.
pub(super) struct ReviewTarget {
    pub(super) path: PathBuf,
    pub(super) draft: bool,
    pub(super) mode: ReviewMode,
}
