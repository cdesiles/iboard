//! The AI review: running it, its panel, and applying its proposal.

use super::side_panel::side_panel;
use super::split::{self, Split};
use super::{
    Align, ButtonKind, Color32, CommonMarkCache, CommonMarkViewer, Duration, Frame, HashMap,
    IboardApp, Job, Layout, Palette, Path, PathBuf, Report, ReviewMode, RichText, Screen, Vec2,
    agent, file_name, job, prompts, reviews, theme, widgets,
};

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
                kind: target.kind,
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
            kind: RunKind::Review,
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
            kind: RunKind::Review,
        };
        let follow_up = review.follow_up.as_deref().unwrap_or("").trim().to_string();
        if follow_up.is_empty() {
            return;
        }
        let Some(content) = self.review_content(&target.path, target.draft) else {
            return;
        };
        let previous = review.review.raw.clone();
        let notes = self.reviewer_notes(&target.path);
        let mut prompt = prompts::review_prompt(&content, target.mode, &notes);
        prompt.push_str(&format!("\n\nA previous review said:\n{previous}"));
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
            self.session
                .review_runs
                .ready
                .insert(path.clone(), run.kind);
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

    /// Review ready, or Update ready, beside its button once the answer on
    /// `path` is in: a click shows it.
    pub(super) fn ready_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let Some(kind) = self.session.review_runs.ready.get(path) else {
            return;
        };
        if widgets::button(ui, ButtonKind::Review, None, kind.ready(), None)
            .on_hover_text(kind.ready_hint())
            .clicked()
        {
            self.show_past_review(path.to_path_buf());
        }
    }

    /// The review panel beside the page: the agent's three findings and the
    /// actions on them, or the tickets of a split. The agent only ever
    /// proposes; Apply opens the editor, Create writes the tickets.
    pub(super) fn review_panel(&mut self, ui: &mut egui::Ui) {
        let Some(mut review) = self.session.review.take() else {
            return;
        };
        let past = &review.review;
        let title = review_title(&review.path);
        let detail = review_detail(&past.agent, past.mode, past.draft);
        let cache = &mut self.reader.cache;

        let action = side_panel(ui, &title, &detail, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let room = (ui.available_height() - PANEL_FOOT_H).max(theme::CONTROL_H);
            let action = theme::scroll_area(
                ui,
                egui::ScrollArea::vertical()
                    .id_salt("review-body")
                    .max_height(room)
                    .auto_shrink([false, true]),
                |ui| review_content(ui, &mut review, cache),
            )
            .inner;
            widgets::rule(ui);
            review_footer(ui, &review).or(action)
        });

        if matches!(action, Some(PanelAction::OpenFollowUp)) {
            review.follow_up = Some(String::new());
            ui.data_mut(|d| d.insert_temp(follow_up_id(), true));
        }
        self.session.review = Some(review);
        self.apply_panel_action(action);
    }

    /// The review panel while the agent works on `path`: the time it has
    /// taken and the step it is on, and Cancel. The board and the page stay
    /// usable; the answer takes its place when it lands.
    pub(super) fn running_review_panel(&mut self, ui: &mut egui::Ui, path: &Path) {
        let Some(run) = self.session.review_runs.running.get(path) else {
            return;
        };
        let title = review_title(path);
        let detail = review_detail(&run.agent, run.mode, run.draft);
        let cancel = side_panel(ui, &title, &detail, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            padded(ui, |ui| {
                running_line(ui, run.answer.elapsed(), agent::step(&run.progress));
            });
            widgets::rule(ui);
            widgets::footer_bar(ui, theme::SPACE_4, |ui| {
                widgets::button(
                    ui,
                    ButtonKind::Secondary,
                    None,
                    "Cancel",
                    Some(run.kind.key()),
                )
                .clicked()
            })
            .inner
        });
        if cancel {
            self.cancel_review_run(path);
        }
    }

    fn apply_panel_action(&mut self, action: Option<PanelAction>) {
        match action {
            Some(PanelAction::Cancel) => self.close_review(),
            Some(PanelAction::Dismiss) => self.session.review = None,
            Some(PanelAction::Split) => self.start_split(),
            Some(PanelAction::Again) => self.ask_again(),
            Some(PanelAction::OpenFollowUp) | None => {}
            Some(PanelAction::Apply) => self.apply_review_proposal(),
            Some(PanelAction::CreateSplit) => self.create_split(),
            Some(PanelAction::BackToReview) => self.back_to_review(),
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
    pub(super) kind: RunKind,
}

/// What a run asked the agent, for what the button and the card call it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RunKind {
    Review,
    /// Update from the code.
    Update,
}

impl RunKind {
    pub(super) fn running(self) -> &'static str {
        match self {
            RunKind::Review => "Reviewing…",
            RunKind::Update => "Updating…",
        }
    }

    /// The key that cancels a run of this kind, and only this kind.
    pub(super) fn key(self) -> &'static str {
        match self {
            RunKind::Review => "⌘⇧R",
            RunKind::Update => "⌘U",
        }
    }

    fn running_hint(self) -> &'static str {
        match self {
            RunKind::Review => "The review is running",
            RunKind::Update => "The update is running",
        }
    }

    fn ready(self) -> &'static str {
        match self {
            RunKind::Review => "Review ready",
            RunKind::Update => "Update ready",
        }
    }

    fn ready_hint(self) -> &'static str {
        match self {
            RunKind::Review => "The review is in: show it",
            RunKind::Update => "The agent's answer is in: show it",
        }
    }
}

/// The review panel on one issue: its findings, or the split they led to.
pub(super) struct ReviewState {
    pub(super) path: PathBuf,
    pub(super) review: PastReview,
    /// The split the findings led to, shown in their place.
    pub(super) split: Option<Split>,
    /// The "Ask again" follow-up, once its button opened the field.
    pub(super) follow_up: Option<String>,
}

impl ReviewState {
    /// The rewrite Apply and `⌘↩` take to the editor: the review's, while no
    /// split runs or shows its tickets in its place.
    pub(super) fn proposal(&self) -> Option<&str> {
        if matches!(self.split, Some(Split::Cutting { .. } | Split::Ready(_))) {
            return None;
        }
        self.review.report.as_ref()?.proposal.as_deref()
    }
}

/// The reviews running in the background, one per issue, and the issues
/// whose review landed since it was last shown.
#[derive(Default)]
pub(super) struct ReviewRuns {
    pub(super) running: HashMap<PathBuf, ReviewRun>,
    ready: HashMap<PathBuf, RunKind>,
}

/// A review the agent is working on, and what to keep with its answer.
pub(super) struct ReviewRun {
    pub(super) kind: RunKind,
    draft: bool,
    mode: ReviewMode,
    agent: String,
    /// [`reviews::content_hash`] of the text the agent reads.
    hash: String,
    answer: Job<Result<agent::Reviewed, String>>,
    /// The step the agent is on, for the panel.
    pub(super) progress: agent::Progress,
}

impl ReviewRuns {
    pub(super) fn is_running(&self, path: &Path) -> bool {
        self.running.contains_key(path)
    }

    /// Whether a run of `kind` works on `path`.
    pub(super) fn runs(&self, path: &Path, kind: RunKind) -> bool {
        self.running.get(path).is_some_and(|run| run.kind == kind)
    }

    pub(super) fn is_ready(&self, path: &Path) -> bool {
        self.ready.contains_key(path)
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
        if let Some(kind) = self.ready.remove(old) {
            self.ready.insert(new.to_path_buf(), kind);
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
        if let Some(run) = self.runs.running.get(path) {
            return Some((run.kind.running(), p.review, run.kind.running_hint()));
        }
        if let Some(kind) = self.runs.ready.get(path) {
            return Some((kind.ready(), p.review, kind.ready_hint()));
        }
        self.past.get(path).map(|past| past.card_pill(p))
    }
}

/// The label column of a finding's row, and the height of its first line.
const AXIS_W: f32 = 76.0;
const AXIS_H: f32 = 18.0;

/// A row's padding in the panel's body, top and bottom.
const ROW_PAD_Y: f32 = 10.0;

/// The panel's footer under the body: its bar, and the rule over it.
const PANEL_FOOT_H: f32 = theme::CONTROL_H + 2.0 * theme::SPACE_3 + 1.0;

/// What a click in the review panel asks for.
enum PanelAction {
    Apply,
    Cancel,
    Again,
    OpenFollowUp,
    Dismiss,
    Split,
    CreateSplit,
    BackToReview,
}

/// "Review of x.md": the panel's title, whatever its state.
fn review_title(path: &Path) -> String {
    format!("Review of {}", file_name(path))
}

/// The tool and model, the scope and what the agent read.
fn review_detail(agent: &str, mode: ReviewMode, draft: bool) -> String {
    let source = if draft { "draft" } else { "file" };
    format!("{agent} · {} · {source}", mode.scope_name())
}

/// A row of the panel's body: 12pt in, 10pt down.
fn padded(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .inner_margin(Vec2::new(theme::SPACE_4, ROW_PAD_Y))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            add(ui);
        });
}

fn follow_up_id() -> egui::Id {
    egui::Id::unique("review-follow-up")
}

/// What scrolls in the panel: the plan note, the body, and the follow-up
/// once Ask again opened it. Enter sends the follow-up.
fn review_content(
    ui: &mut egui::Ui,
    review: &mut ReviewState,
    cache: &mut CommonMarkCache,
) -> Option<PanelAction> {
    let p = theme::palette(ui);
    if review.review.plan {
        padded(ui, |ui| {
            ui.label(
                RichText::new(PLAN_NOTE)
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        });
    }
    let mut action = review_body(ui, review, cache);
    if !matches!(review.split, None | Some(Split::Failed(_))) {
        return action;
    }
    let Some(follow_up) = review.follow_up.as_mut() else {
        return action;
    };
    widgets::rule(ui);
    padded(ui, |ui| {
        let entry = widgets::text_entry(
            ui,
            egui::TextEdit::singleline(follow_up)
                .id(follow_up_id())
                .hint_text(RichText::new("Make it smaller…").color(p.ink_faint))
                .font(theme::ui_body())
                .desired_width(f32::INFINITY),
        );
        if ui
            .data_mut(|d| d.remove_temp::<bool>(follow_up_id()))
            .is_some()
        {
            entry.request_focus();
        }
        if entry.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            action = Some(PanelAction::Again);
        }
    });
    action
}

/// The panel's body: the error, the findings as rows or the sections proposed
/// for a title, the answer that did not parse, or the split, under way or in
/// its tickets.
fn review_body(
    ui: &mut egui::Ui,
    review: &mut ReviewState,
    cache: &mut CommonMarkCache,
) -> Option<PanelAction> {
    let p = theme::palette(ui);
    let mut action = None;
    match (&mut review.split, &review.review.report) {
        (Some(Split::Cutting { answer, progress }), _) => padded(ui, |ui| {
            let step = agent::step(progress).or(Some("The agent is cutting the issue…".into()));
            running_line(ui, answer.elapsed(), step);
        }),
        (Some(Split::Failed(error)), _) => padded(ui, |ui| {
            ui.label(RichText::new(error.as_str()).color(p.danger));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                let retry = PanelAction::Split;
                let kind = ButtonKind::Secondary;
                widgets::action_button(ui, &mut action, kind, "Retry", None, retry);
                let back = PanelAction::BackToReview;
                widgets::action_button(ui, &mut action, kind, "Back to review", None, back);
            });
        }),
        (Some(Split::Ready(drafts)), _) => padded(ui, |ui| split::drafts_ui(ui, drafts)),
        (None, None) => padded(ui, |ui| {
            ui.label(RichText::new(&review.review.raw).color(p.ink_muted));
        }),
        (None, Some(report)) if report.findings.is_empty() => padded(ui, |ui| {
            ui.label(
                RichText::new("This issue is only a title. The sections it is missing:")
                    .font(theme::ui_small())
                    .color(p.ink_muted),
            );
            let proposal = report.proposal.as_deref().unwrap_or_default();
            CommonMarkViewer::new().show(ui, cache, proposal);
        }),
        (None, Some(report)) => {
            for finding in &report.findings {
                widgets::rule(ui);
                if finding_row(ui, finding) {
                    action = Some(PanelAction::Split);
                }
            }
        }
    }
    action
}

/// One finding: its axis in the label column; its pill and words beside it,
/// and a Size that is not a Pass offering a split. Returns whether it was
/// taken up.
fn finding_row(ui: &mut egui::Ui, finding: &agent::Finding) -> bool {
    let p = theme::palette(ui);
    let (verdict, color) = verdict_look(finding.verdict, p);
    let splits = finding.axis == agent::Axis::Size && finding.verdict != agent::Verdict::Pass;
    let mut split = false;
    padded(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.allocate_ui_with_layout(
                Vec2::new(AXIS_W, AXIS_H),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_min_width(AXIS_W);
                    ui.label(
                        RichText::new(finding.axis.name().to_uppercase())
                            .font(theme::ui_label())
                            .extra_letter_spacing(theme::LABEL_TRACKING)
                            .color(p.ink),
                    );
                },
            );
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                widgets::pill(ui, verdict, color);
                if !finding.body.is_empty() {
                    ui.label(
                        RichText::new(&finding.body)
                            .font(theme::ui_small())
                            .color(p.ink),
                    );
                }
                if splits {
                    ui.add_space(theme::SPACE_1);
                    split = widgets::button(
                        ui,
                        ButtonKind::ReviewHairline,
                        None,
                        "Split into tickets…",
                        None,
                    )
                    .on_hover_text("Ask the agent to cut this issue into smaller ones")
                    .clicked();
                }
            });
        });
    });
    split
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
            let kind = ButtonKind::Primary.when(kept > 0);
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
            let (kind, ask) = match review.follow_up.as_deref().map(str::trim) {
                None => (ButtonKind::Secondary, PanelAction::OpenFollowUp),
                Some("") => (ButtonKind::Secondary.when(false), PanelAction::Again),
                Some(_) => (ButtonKind::Secondary, PanelAction::Again),
            };
            widgets::action_button(ui, &mut action, kind, "Ask again", None, ask);
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
/// "42s · Reading app/review.rs…".
pub(super) fn running_line(ui: &mut egui::Ui, elapsed: Duration, step: Option<String>) {
    let p = theme::palette(ui);
    let clock = crate::format::clock(elapsed);
    let step = step.unwrap_or_else(|| "The agent is reading the issue…".to_string());
    widgets::announce_thinking(ui, "The agent is working", elapsed);
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
