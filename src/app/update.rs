//! Update with AI: the agent checks in the issue's project where an issue
//! stands (done, under way, blocked, no longer worth it) and proposes it
//! rewritten as things are. Its answer is a review, in the same panel; Apply
//! opens the editor with the update, and saving it is the accept.

use super::*;

impl IboardApp {
    /// Whether `path` can be updated: on the board, past Todo (whose review
    /// covers it) and not done.
    pub(super) fn updatable(&self, path: &Path) -> bool {
        self.board
            .column_of(path)
            .is_some_and(|column| !column.is_done() && !column.accepts_new_issues())
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
        };
        self.run_review(target, &content, prompt);
    }

    /// The button and `⌘U`: starts an update, or cancels the one running; with
    /// no agent set up, Settings open instead.
    pub(super) fn toggle_update(&mut self, path: &Path) {
        if !self.updatable(path) {
            return;
        }
        if self.review_is_running(path) {
            self.cancel_review_run(path);
        } else if !self.agent.configured() {
            self.open_config();
        } else {
            self.start_update(path);
        }
    }

    /// Update with AI, in the Reader of an issue [`Self::updatable`]; once the
    /// answer is in, Update ready shows it.
    pub(super) fn update_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let running = self.review_is_running(path);
        let configured = self.agent.configured();
        let (icon, label) = match running {
            true => (Icon::Thinking, "Updating…"),
            false => (Icon::Review, "Update with AI"),
        };
        let hint = if running {
            "The agent is checking the project: cancel it"
        } else if configured {
            "Ask the agent where this issue stands in the project, and to rewrite it as things are"
        } else {
            "No agent is set up yet: choose one in Settings › AI review (⌘,)"
        };
        if widgets::button(ui, ButtonKind::Review, Some(icon), label, Some("⌘U"))
            .on_hover_text(hint)
            .clicked()
        {
            self.toggle_update(path);
        }
        if self.session.review_runs.is_ready(path)
            && widgets::button(ui, ButtonKind::Review, None, "Update ready", None)
                .on_hover_text("The agent's answer is in: show it")
                .clicked()
        {
            self.show_past_review(path.to_path_buf());
        }
    }
}
