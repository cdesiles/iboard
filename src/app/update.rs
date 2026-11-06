//! Update from the code: the agent checks in the issue's project where an issue
//! stands (done, under way, blocked, no longer worth it) and proposes it
//! rewritten as things are. Its answer is a review, in the same panel; Apply
//! opens the editor with the update, and saving it is the accept.

use super::review::{ReviewTarget, RunKind};
use super::{IboardApp, Path, ReviewMode, prompts};

impl IboardApp {
    /// Whether `path` can be updated: on the board and not done.
    pub(super) fn updatable(&self, path: &Path) -> bool {
        self.board
            .column_of(path)
            .is_some_and(|column| !column.is_done())
    }

    /// Asks the agent where the issue at `path` stands, in its project.
    pub(super) fn start_update(&mut self, path: &Path) {
        if !self.updatable(path) || self.review_is_running(path) {
            return;
        }
        let Some(content) = self.review_content(path, false) else {
            return;
        };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let prompt = prompts::update_prompt(&content, &today);
        let target = ReviewTarget {
            path: path.to_path_buf(),
            draft: false,
            mode: ReviewMode::Deep,
            kind: RunKind::Update,
        };
        self.run_review(target, &content, prompt);
    }

    /// The button and `⌘U`: starts an update, or cancels the one running; with
    /// no agent set up, Settings open instead. A run on an issue that reached
    /// Done can still be cancelled or shown. A Review running is left alone, as
    /// the AI menu leaves it: its key is `⌘⇧R`.
    pub(super) fn toggle_update(&mut self, path: &Path) {
        if self.session.review_runs.is_ready(path) {
            self.show_past_review(path.to_path_buf());
        } else if self.session.review_runs.runs(path, RunKind::Update) {
            self.cancel_review_run(path);
        } else if self.updatable(path) && !self.review_is_running(path) {
            if self.prefs.agent.configured() {
                self.start_update(path);
            } else {
                self.open_config();
            }
        }
    }
}
