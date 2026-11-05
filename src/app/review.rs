//! The AI review: running it, its panel, and applying its proposal.

use super::side_panel::side_panel;
use super::split::Split;
use super::*;

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
        let answer = agent::start_plan_review(
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
            },
        );
    }

    /// Starts a review of the open issue (or the editor draft), in the scope
    /// Settings › AI review chose.
    pub(super) fn start_review(&mut self, path: &Path, draft: bool) {
        self.start_review_in(path, draft, self.prefs.agent.scope);
    }

    /// Starts a one-shot review in `mode`.
    pub(super) fn start_review_in(&mut self, path: &Path, draft: bool, mode: ReviewMode) {
        if !self.is_todo_issue(path) || self.review_is_running(path) {
            return;
        }
        let Some(content) = self.review_content(path, draft) else {
            return;
        };
        let notes = self.reviewer_notes(path);
        let prompt = prompts::review_prompt(&content, mode, &notes);
        let target = ReviewTarget {
            path: path.to_path_buf(),
            draft,
            mode,
        };
        self.run_review(target, &content, prompt);
    }

    /// Sends a follow-up ("make it smaller") with the previous answer attached.
    pub(super) fn ask_again(&mut self) {
        let Some(review) = self.session.review.as_ref() else {
            return;
        };
        let target = ReviewTarget {
            path: review.path.clone(),
            draft: review.review.draft,
            mode: review.review.mode,
        };
        let follow_up = review.follow_up.trim().to_string();
        if follow_up.is_empty() {
            return;
        }
        let Some(content) = self.review_content(&target.path, target.draft) else {
            return;
        };
        let previous = review.split.is_none().then(|| review.review.raw.clone());
        let notes = self.reviewer_notes(&target.path);
        let mut prompt = prompts::review_prompt(&content, target.mode, &notes);
        if let Some(raw) = previous {
            prompt.push_str(&format!("\n\nA previous review said:\n{raw}"));
        }
        prompt.push_str(&format!("\n\nFollow-up: {follow_up}"));
        self.run_review(target, &content, prompt);
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
                follow_up: String::new(),
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
                    self.error = Some(format!("Review of {} failed: {error}", file_name(&path)));
                    continue;
                }
            };
            let stored = reviews::StoredReview {
                hash: run.hash,
                agent: run.agent,
                mode: run.mode.key().to_string(),
                draft: run.draft,
                plan: reviewed.plan,
                at: chrono::Local::now().to_rfc3339(),
                raw: reviewed.answer,
            };
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

    /// Whether the review panel shows a proposal `⌘↩` would apply.
    pub(super) fn proposal_shown(&self) -> bool {
        self.session
            .review
            .as_ref()
            .is_some_and(|review| review.proposal().is_some())
    }

    /// Whether a review of `path` is running in the background.
    pub(super) fn review_is_running(&self, path: &Path) -> bool {
        self.session.review_runs.is_running(path)
    }

    /// The proposed rewrite, when the agent offered one: the editor opens with
    /// it as an unsaved draft, so saving or discarding is the accept or refuse.
    pub(super) fn apply_review_proposal(&mut self) {
        let Some(review) = self.session.review.as_ref() else {
            return;
        };
        let path = review.path.clone();
        let Some(proposal) = review.proposal().map(str::to_string) else {
            return;
        };
        self.open_editor(&path);
        if let Some(editor) = self.session.editor.as_mut()
            && editor.path == path
        {
            editor.replace(proposal);
        }
        self.session.review = None;
    }

    /// The Review split button, rightmost in the Reader and Editor header: the
    /// main part refines the issue (or cancels a running review), the caret
    /// opens a menu with the reviews, naming what runs and leading to the AI
    /// settings.
    pub(super) fn review_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let running = self.review_is_running(path);
        let configured = self.prefs.agent.configured();
        let label = if running { "Reviewing…" } else { "Refine" };
        let hint = if running {
            "The review runs in the background: cancel it (⌘⇧R)"
        } else if configured {
            "Refine this issue in a conversation with a challenger"
        } else {
            "No agent is set up yet: choose one in Settings › AI review (⌘, or the caret)"
        };
        let (main, caret) = widgets::split_button(
            ui,
            egui::Id::new("review-button"),
            ButtonKind::Review,
            Some(if running {
                Icon::Thinking
            } else {
                Icon::Review
            }),
            label,
            Some(if running { "⌘⇧R" } else { "⌘R" }),
            running || configured,
        );
        if main.on_hover_text(hint).clicked() {
            if running {
                self.cancel_review_run(path);
            } else if configured {
                self.start_refinement(path);
            } else {
                self.open_config();
            }
        }
        let options = "Review options";
        let caret = caret.on_hover_text(options);
        caret.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), options)
        });
        let runs_with = if configured {
            self.prefs.agent.label()
        } else {
            "No agent set up yet".to_string()
        };
        let other_scope = match self.prefs.agent.scope {
            ReviewMode::Quick => ReviewMode::Deep,
            ReviewMode::Deep => ReviewMode::Quick,
        };
        let picked = review_menu(&caret, &runs_with, other_scope);
        self.apply_review_menu(path, picked, other_scope);
        self.ready_button(ui, path, "Review ready", "The review is in: show it");
    }

    /// Runs the Review menu's pick. While a review runs, its reviews and
    /// Refine do nothing; with no agent set up, they open Settings as AI
    /// settings does.
    fn apply_review_menu(
        &mut self,
        path: &Path,
        picked: Option<ReviewMenu>,
        other_scope: ReviewMode,
    ) {
        let running = self.review_is_running(path);
        let configured = self.prefs.agent.configured();
        let draft = self.session.editing().is_some();
        match picked {
            Some(ReviewMenu::Review | ReviewMenu::OtherScope | ReviewMenu::Refine) if running => {}
            Some(ReviewMenu::Review) if configured => self.start_review(path, draft),
            Some(ReviewMenu::OtherScope) if configured => {
                self.start_review_in(path, draft, other_scope);
            }
            Some(ReviewMenu::Refine) if configured => self.start_refinement(path),
            Some(
                ReviewMenu::Review
                | ReviewMenu::OtherScope
                | ReviewMenu::Refine
                | ReviewMenu::Settings,
            ) => {
                self.open_config();
            }
            None => {}
        }
    }

    /// Review ready, or Update ready, beside its button once the answer on
    /// `path` is in: a click shows it.
    pub(super) fn ready_button(&mut self, ui: &mut egui::Ui, path: &Path, label: &str, hint: &str) {
        if self.session.review_runs.is_ready(path)
            && widgets::button(ui, ButtonKind::Review, None, label, None)
                .on_hover_text(hint)
                .clicked()
        {
            self.show_past_review(path.to_path_buf());
        }
    }

    /// The review panel beside the page: the agent's three findings and the
    /// actions on them, or the tickets of a split. The agent only ever
    /// proposes; Apply opens the editor, Create writes the tickets.
    pub(super) fn review_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let Some(mut review) = self.session.review.take() else {
            return;
        };
        let past = &review.review;
        let source = if past.draft { "draft" } else { "file" };
        let title = format!("Review of {}", file_name(&review.path));
        let detail = format!("{} · {} · {source}", past.agent, past.mode.scope_name());
        let plan = past.plan;

        let action = side_panel(ui, &title, &detail, |ui| {
            let mut action = None;
            Frame::new().inner_margin(theme::SPACE_4).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if plan {
                    ui.label(
                        RichText::new(PLAN_NOTE)
                            .font(theme::meta())
                            .color(p.ink_muted),
                    );
                    ui.add_space(theme::SPACE_2);
                }
                action = review_body(ui, &mut review);
                if matches!(review.split, None | Some(Split::Failed(_))) {
                    ui.add_space(theme::SPACE_2);
                    widgets::text_entry(
                        ui,
                        egui::TextEdit::singleline(&mut review.follow_up)
                            .hint_text(
                                RichText::new("Ask again: make it smaller…").color(p.ink_faint),
                            )
                            .font(theme::ui_body())
                            .desired_width(f32::INFINITY),
                    );
                }
            });
            review_footer(ui, &review).or(action)
        });

        self.session.review = Some(review);
        self.apply_panel_action(action);
    }

    fn apply_panel_action(&mut self, action: Option<PanelAction>) {
        match action {
            Some(PanelAction::Cancel) => self.close_review(),
            Some(PanelAction::Dismiss) => self.session.review = None,
            Some(PanelAction::Retry) => {
                let target =
                    self.session.review.as_ref().map(|review| {
                        (review.path.clone(), review.review.draft, review.review.mode)
                    });
                if let Some((path, draft, mode)) = target {
                    self.start_review_in(&path, draft, mode);
                }
            }
            Some(PanelAction::Again) => self.ask_again(),
            Some(PanelAction::Apply) => self.apply_review_proposal(),
            Some(PanelAction::Split) => self.start_split(),
            Some(PanelAction::CreateSplit) => self.create_split(),
            Some(PanelAction::BackToReview) => self.back_to_review(),
            None => {}
        }
    }
}

/// What the review panel says of a review run in plan mode.
pub(super) const PLAN_NOTE: &str = "Plan mode: the agent could not change the repository";

/// What a review reads, and how far it goes.
pub(super) struct ReviewTarget {
    pub(super) path: PathBuf,
    pub(super) draft: bool,
    pub(super) mode: ReviewMode,
}

/// The review panel on one issue: its findings, or the split they led to.
pub(super) struct ReviewState {
    pub(super) path: PathBuf,
    pub(super) review: PastReview,
    /// The split the findings led to, shown in their place.
    pub(super) split: Option<Split>,
    /// The "Ask again" follow-up, kept between renders.
    pub(super) follow_up: String,
}

impl ReviewState {
    /// The rewrite Apply and `⌘↩` take to the editor: the review's, while no
    /// split shows in its place.
    pub(super) fn proposal(&self) -> Option<&str> {
        if self.split.is_some() {
            return None;
        }
        self.review.report.as_ref()?.proposal.as_deref()
    }
}

/// The reviews running in the background, one per issue, and the issues
/// whose review landed since it was last shown.
#[derive(Default)]
pub(super) struct ReviewRuns {
    running: HashMap<PathBuf, ReviewRun>,
    ready: HashSet<PathBuf>,
}

/// A review the agent is working on, and what to keep with its answer.
struct ReviewRun {
    draft: bool,
    mode: ReviewMode,
    agent: String,
    /// [`reviews::content_hash`] of the text the agent reads.
    hash: String,
    answer: Job<Result<agent::Reviewed, String>>,
}

impl ReviewRuns {
    pub(super) fn is_running(&self, path: &Path) -> bool {
        self.running.contains_key(path)
    }

    pub(super) fn is_ready(&self, path: &Path) -> bool {
        self.ready.contains(path)
    }

    pub(super) fn any_running(&self) -> bool {
        !self.running.is_empty()
    }

    /// How many reviews run in the background.
    pub(super) fn running_count(&self) -> usize {
        self.running.len()
    }

    /// Stops the run on `path`, if any, and drops its ready mark.
    pub(super) fn forget(&mut self, path: &Path) {
        self.running.remove(path);
        self.ready.remove(path);
    }

    /// Follows an issue renamed from `old` to `new`.
    pub(super) fn rename(&mut self, old: &Path, new: &Path) {
        if let Some(run) = self.running.remove(old) {
            self.running.insert(new.to_path_buf(), run);
        }
        if self.ready.remove(old) {
            self.ready.insert(new.to_path_buf());
        }
    }
}

/// A card's review pill: its word, colour and tooltip.
pub(super) type ReviewPill = (&'static str, Color32, &'static str);

/// An issue's last review, read from its sidecar, so its card pill can
/// reopen it.
#[derive(Clone)]
pub(super) struct PastReview {
    /// True when the agent read the editor draft rather than the saved file.
    pub(super) draft: bool,
    pub(super) mode: ReviewMode,
    /// True when the agent ran in plan mode, unable to change the repository.
    pub(super) plan: bool,
    /// The configured tool (and model), shown in the panel header.
    pub(super) agent: String,
    pub(super) raw: String,
    pub(super) report: Option<Report>,
    /// The issue changed since the agent read it.
    pub(super) stale: bool,
}

impl PastReview {
    pub(super) fn new(stored: &reviews::StoredReview, markdown: &str) -> Self {
        Self {
            draft: stored.draft,
            mode: ReviewMode::from_key(&stored.mode),
            plan: stored.plan,
            agent: stored.agent.clone(),
            raw: stored.raw.clone(),
            report: agent::parse(&stored.raw),
            stale: stored.is_stale(markdown),
        }
    }

    /// The review's verdict: the worst of its three, `None` when the answer
    /// did not parse.
    fn verdict(&self) -> Option<agent::Verdict> {
        self.report
            .iter()
            .flat_map(|r| r.findings.iter().map(|f| f.verdict))
            .max()
    }

    /// The pill: the worst verdict of the three, or "Reviewed" when the
    /// answer did not parse.
    pub(super) fn pill(&self, p: &Palette) -> (&'static str, Color32) {
        self.verdict()
            .map_or(("Reviewed", p.review), |verdict| verdict_look(verdict, p))
    }

    /// The card's pill: the verdict, faint once the issue changed since.
    pub(super) fn card_pill(&self, p: &Palette) -> ReviewPill {
        let (word, color) = self.pill(p);
        let color = if self.stale { p.ink_faint } else { color };
        (word, color, verdict_hint(self.verdict(), self.stale))
    }
}

/// A verdict's word, and the colour of its pill.
fn verdict_look(verdict: agent::Verdict, p: &Palette) -> (&'static str, Color32) {
    match verdict {
        agent::Verdict::Pass => ("Pass", p.status_done),
        agent::Verdict::Challenge => ("Challenge", p.review),
        agent::Verdict::Block => ("Block", p.danger),
    }
}

/// What a card's verdict pill means: the worst of the review's three axes.
fn verdict_hint(verdict: Option<agent::Verdict>, stale: bool) -> &'static str {
    match (verdict, stale) {
        (Some(agent::Verdict::Pass), false) => {
            "Last AI review: Pass. Relevance, size and clarity are all fine. Click to show it"
        }
        (Some(agent::Verdict::Challenge), false) => {
            "Last AI review: Challenge. Relevance, size or clarity is worth another look, \
             nothing blocks. Click to show it"
        }
        (Some(agent::Verdict::Block), false) => {
            "Last AI review: Block. Relevance, size or clarity is a real problem \
             (for an update: done, or no longer worth doing). Click to show it"
        }
        (None, false) => "Last AI review. Click to show it",
        (Some(agent::Verdict::Pass), true) => {
            "Last AI review: Pass, before the issue's last edit. Click to show it"
        }
        (Some(agent::Verdict::Challenge), true) => {
            "Last AI review: Challenge (relevance, size or clarity worth another look), \
             before the issue's last edit. Click to show it"
        }
        (Some(agent::Verdict::Block), true) => {
            "Last AI review: Block (relevance, size or clarity a real problem), \
             before the issue's last edit. Click to show it"
        }
        (None, true) => "Last AI review, before the issue's last edit. Click to show it",
    }
}

/// What the cards' review pill says: the review running or ready, else the
/// last verdict.
#[derive(Clone, Copy)]
pub(super) struct CardReviews<'a> {
    pub(super) past: &'a HashMap<PathBuf, PastReview>,
    pub(super) runs: &'a ReviewRuns,
}

impl CardReviews<'_> {
    pub(super) fn pill(&self, path: &Path, p: &Palette) -> Option<ReviewPill> {
        if self.runs.is_running(path) {
            return Some(("Reviewing…", p.review, "The review is running"));
        }
        if self.runs.is_ready(path) {
            return Some(("Review ready", p.review, "The review is in: show it"));
        }
        self.past.get(path).map(|past| past.card_pill(p))
    }
}

/// Width of the Review button's menu, in points.
const REVIEW_MENU_WIDTH: f32 = 240.0;

/// A choice made in the Review button's menu.
enum ReviewMenu {
    Review,
    /// A review in the scope Settings › AI review did not choose.
    OtherScope,
    Refine,
    Settings,
}

/// The Review button's menu, under its caret: Refine, the review in either
/// scope, what the agent runs with, and AI settings.
fn review_menu(
    caret: &egui::Response,
    runs_with: &str,
    other_scope: ReviewMode,
) -> Option<ReviewMenu> {
    egui::Popup::menu(caret)
        .width(REVIEW_MENU_WIDTH)
        .align(egui::RectAlign::BOTTOM_END)
        .show(|ui| {
            ui.set_min_width(REVIEW_MENU_WIDTH);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let p = theme::palette(ui);
            let mut picked = None;
            let option = |ui: &mut egui::Ui, name: &str, meta: &str| {
                widgets::menu_option(
                    ui,
                    widgets::MenuOption {
                        name,
                        mono: false,
                        muted: false,
                        meta,
                        checked: false,
                        active: false,
                    },
                )
            };
            if option(ui, "Refine with a challenger", "⌘R").clicked() {
                picked = Some(ReviewMenu::Refine);
            }
            if option(ui, "Review this issue", "⌘⇧R").clicked() {
                picked = Some(ReviewMenu::Review);
            }
            let (other, meta) = match other_scope {
                ReviewMode::Deep => ("Deep review", "reads the repo"),
                ReviewMode::Quick => ("Quick review", "ticket only"),
            };
            if option(ui, other, meta).clicked() {
                picked = Some(ReviewMenu::OtherScope);
            }
            ui.add_space(theme::SPACE_2);
            ui.separator();
            widgets::menu_group(ui, "Runs with");
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), theme::CONTROL_H),
                Sense::hover(),
            );
            ui.painter().text(
                Pos2::new(
                    rect.min.x + 2.0 * theme::SPACE_3 + theme::ICON_SIZE,
                    rect.center().y,
                ),
                egui::Align2::LEFT_CENTER,
                runs_with,
                egui::FontId::monospace(12.0),
                p.ink,
            );
            if option(ui, "AI settings…", "⌘,").clicked() {
                picked = Some(ReviewMenu::Settings);
            }
            picked
        })
        .and_then(|inner| inner.inner)
}

/// What a click in the review panel asks for.
enum PanelAction {
    Apply,
    Cancel,
    Retry,
    Again,
    Dismiss,
    Split,
    CreateSplit,
    BackToReview,
}

/// The panel's body: the run in progress, the error, the three findings
/// (Size offering a split when it is not a Pass), or the split's tickets.
fn review_body(ui: &mut egui::Ui, review: &mut ReviewState) -> Option<PanelAction> {
    let p = theme::palette(ui);
    let mut action = None;
    match (&mut review.split, &review.review.report) {
        (Some(Split::Cutting { answer, progress }), _) => {
            let step = agent::step(progress).or(Some("The agent is cutting the issue…".into()));
            running_line(ui, answer.elapsed(), step);
        }
        (Some(Split::Failed(error)), _) => {
            ui.label(RichText::new(error.as_str()).color(p.danger));
        }
        (Some(Split::Ready(drafts)), _) => split::drafts_ui(ui, drafts),
        (None, None) => {
            ui.label(RichText::new(&review.review.raw).color(p.ink_muted));
        }
        (None, Some(report)) => {
            for finding in &report.findings {
                let (label, color) = verdict_look(finding.verdict, p);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(finding.axis.name())
                            .font(theme::ui_strong())
                            .color(p.ink),
                    );
                    widgets::pill(ui, label, color);
                });
                if !finding.body.is_empty() {
                    ui.label(RichText::new(&finding.body).color(p.ink_muted));
                }
                if finding.axis == agent::Axis::Size && finding.verdict != agent::Verdict::Pass {
                    ui.add_space(theme::SPACE_2);
                    if widgets::button(ui, ButtonKind::Review, None, "Split into tickets…", None)
                        .on_hover_text("Ask the agent to cut this issue into smaller ones")
                        .clicked()
                    {
                        action = Some(PanelAction::Split);
                    }
                }
                ui.add_space(theme::SPACE_4);
            }
        }
    }
    action
}

/// The panel's buttons, right to left: the main action first.
fn review_footer(ui: &mut egui::Ui, review: &ReviewState) -> Option<PanelAction> {
    let mut action = None;
    widgets::footer_bar(ui, theme::SPACE_4, |ui| match &review.split {
        Some(Split::Cutting { .. }) => {
            widgets::action_button(
                ui,
                &mut action,
                ButtonKind::Secondary,
                "Cancel",
                Some("esc"),
                PanelAction::Cancel,
            );
        }
        Some(Split::Ready(drafts)) => {
            let kept = drafts.iter().filter(|draft| draft.include).count();
            let create = format!("Create {kept} tickets");
            let kind = if kept > 0 {
                ButtonKind::Primary
            } else {
                ButtonKind::Disabled
            };
            widgets::action_button(
                ui,
                &mut action,
                kind,
                &create,
                None,
                PanelAction::CreateSplit,
            );
            widgets::action_button(
                ui,
                &mut action,
                ButtonKind::Secondary,
                "Back to review",
                None,
                PanelAction::BackToReview,
            );
            widgets::action_button(
                ui,
                &mut action,
                ButtonKind::Ghost,
                "Dismiss",
                None,
                PanelAction::Dismiss,
            );
        }
        None | Some(Split::Failed(_)) => {
            ui.add_enabled_ui(review.proposal().is_some(), |ui| {
                widgets::action_button(
                    ui,
                    &mut action,
                    ButtonKind::Primary,
                    "Apply to draft",
                    Some("⌘↩"),
                    PanelAction::Apply,
                );
            });
            if matches!(review.split, Some(Split::Failed(_))) {
                widgets::action_button(
                    ui,
                    &mut action,
                    ButtonKind::Secondary,
                    "Retry",
                    None,
                    PanelAction::Retry,
                );
            }
            if !review.follow_up.trim().is_empty() {
                widgets::action_button(
                    ui,
                    &mut action,
                    ButtonKind::Secondary,
                    "Ask again",
                    None,
                    PanelAction::Again,
                );
            }
            widgets::action_button(
                ui,
                &mut action,
                ButtonKind::Ghost,
                "Dismiss",
                Some("esc"),
                PanelAction::Dismiss,
            );
        }
    });
    action
}

/// The Thinking mark, the time the agent has taken, and the step it is on:
/// "0:42 · Reading app/review.rs…".
pub(super) fn running_line(ui: &mut egui::Ui, elapsed: Duration, step: Option<String>) {
    let p = theme::palette(ui);
    let seconds = elapsed.as_secs();
    let clock = format!("{}:{:02}", seconds / 60, seconds % 60);
    let step = step.unwrap_or_else(|| "The agent is reading the issue…".to_string());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        widgets::thinking(ui, 14.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(clock).font(theme::meta()).color(p.ink_muted));
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(step).color(p.ink_muted)).truncate());
            });
        });
    });
}
