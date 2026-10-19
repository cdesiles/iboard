//! The AI review: running it, its panel, and applying its proposal.

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
        if !self.agent.configured() {
            self.open_config();
            return;
        }
        let (answer, _) = agent::start_plan_review(&self.agent, &prompt, &target.path, target.mode);
        self.mark_notes_sent(&target.path);
        self.close_review();
        let runs = &mut self.session.review_runs;
        runs.ready.remove(&target.path);
        runs.running.insert(
            target.path,
            ReviewRun {
                draft: target.draft,
                mode: target.mode,
                agent: self.agent.label(),
                hash: reviews::content_hash(content),
                answer,
            },
        );
    }

    /// Starts a Quick review of the open issue (or the editor draft).
    pub(super) fn start_review(&mut self, path: &Path, draft: bool) {
        self.start_review_in(path, draft, ReviewMode::Quick);
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
            draft: review.draft,
            mode: review.mode,
        };
        let follow_up = review.follow_up.trim().to_string();
        if follow_up.is_empty() {
            return;
        }
        let Some(content) = self.review_content(&target.path, target.draft) else {
            return;
        };
        let previous = match &review.status {
            ReviewStatus::Done { raw, .. } => Some(raw.clone()),
            _ => None,
        };
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
        self.session.review_runs.ready.remove(&path);
        if self.review_is_running(&path) {
            return;
        }
        if let Some(past) = self.past_reviews.get(&path).cloned() {
            self.session.review = Some(ReviewState {
                path,
                draft: past.draft,
                mode: past.mode,
                plan: past.plan,
                agent: past.agent,
                status: ReviewStatus::Done {
                    raw: past.raw,
                    report: past.report,
                },
                follow_up: String::new(),
            });
        }
    }

    /// Adopts the answers that are in, kept beside their issue and marked
    /// ready; a failure goes to the status strip and leaves the last review
    /// alone. Called every frame.
    pub(super) fn poll_review(&mut self) {
        let landed: Vec<(PathBuf, Result<agent::Reviewed, String>)> = self
            .session
            .review_runs
            .running
            .iter_mut()
            .filter_map(|(path, run)| Some((path.clone(), run.answer.answer()?)))
            .collect();
        for (path, result) in landed {
            let Some(run) = self.session.review_runs.running.remove(&path) else {
                continue;
            };
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
            if let Err(err) = reviews::save_review(&self.board.root, &path, stored.clone()) {
                self.error = Some(err);
            }
            if let Some(issue) = self.board.find(&path) {
                let past = PastReview::new(&stored, &issue.markdown);
                self.past_reviews.insert(path.clone(), past);
            }
            self.session.review_runs.ready.insert(path);
        }
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
        let proposal = match &review.status {
            ReviewStatus::Done {
                report: Some(report),
                ..
            } => report.proposal.clone(),
            _ => None,
        };
        let Some(proposal) = proposal else {
            return;
        };
        if !self
            .session
            .editor
            .as_ref()
            .is_some_and(|editor| editor.path == path)
        {
            self.start_editing(&path);
        }
        if let Some(editor) = self.session.editor.as_mut()
            && editor.path == path
        {
            editor.text = proposal;
            editor.last_edit = Some(Instant::now());
        }
        self.session.detail_mode = DetailMode::Edit;
        self.session.review = None;
    }

    /// The Review split button, rightmost in the Reader and Editor header: the
    /// main part refines the issue (or cancels a running review), the caret
    /// opens a menu with the reviews, naming what runs and leading to the AI
    /// settings.
    pub(super) fn review_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let running = self.review_is_running(path);
        let configured = self.agent.configured();
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
            Some(Icon::Review),
            label,
            Some(if running { "⌘⇧R" } else { "⌘R" }),
            running || configured,
        );
        let draft = self.session.detail_mode == DetailMode::Edit;
        if main.on_hover_text(hint).clicked() {
            if running {
                self.cancel_review_run(path);
            } else if configured {
                self.start_refinement(path);
            } else {
                self.open_config();
            }
        }
        let caret = caret.on_hover_text("Review options");
        let p = theme::palette(ui);
        let runs_with = if configured {
            self.agent.label()
        } else {
            "No agent set up yet".to_string()
        };
        let picked = egui::Popup::menu(&caret)
            .width(REVIEW_MENU_WIDTH)
            .align(egui::RectAlign::BOTTOM_END)
            .show(|ui| {
                ui.set_min_width(REVIEW_MENU_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let mut picked = None;
                let option = |ui: &mut egui::Ui, name: &str, mono: bool, meta: &str| {
                    widgets::menu_option(
                        ui,
                        widgets::MenuOption {
                            name,
                            mono,
                            muted: false,
                            meta,
                            checked: false,
                            active: false,
                        },
                    )
                };
                if option(ui, "Refine with a challenger", false, "⌘R").clicked() {
                    picked = Some(ReviewMenu::Refine);
                }
                if option(ui, "Review this issue", false, "⌘⇧R").clicked() {
                    picked = Some(ReviewMenu::Review);
                }
                if option(ui, "Deep review", false, "reads the repo").clicked() {
                    picked = Some(ReviewMenu::Deep);
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
                    &runs_with,
                    egui::FontId::monospace(12.0),
                    p.ink,
                );
                if option(ui, "AI settings…", false, "⌘,").clicked() {
                    picked = Some(ReviewMenu::Settings);
                }
                picked
            })
            .and_then(|inner| inner.inner);
        match picked {
            Some(ReviewMenu::Review | ReviewMenu::Deep | ReviewMenu::Refine) if running => {}
            Some(ReviewMenu::Review) if configured => self.start_review(path, draft),
            Some(ReviewMenu::Deep) if configured => {
                self.start_review_in(path, draft, ReviewMode::Deep);
            }
            Some(ReviewMenu::Refine) if configured => self.start_refinement(path),
            Some(
                ReviewMenu::Review | ReviewMenu::Deep | ReviewMenu::Refine | ReviewMenu::Settings,
            ) => {
                self.open_config();
            }
            None => {}
        }
        if self.session.review_runs.is_ready(path)
            && widgets::button(ui, ButtonKind::Review, None, "Review ready", None)
                .on_hover_text("The review is in: show it")
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
        let Some(review) = self.session.review.as_ref() else {
            return;
        };
        let name = file_name(&review.path);
        let source = if review.draft { "draft" } else { "file" };
        let agent = format!("{} · {}", review.agent, review.mode.key());
        let plan = review.plan;
        let mut follow_up = review.follow_up.clone();
        let mut drafts = match &review.status {
            ReviewStatus::SplitReady { drafts, .. } => Some(drafts.clone()),
            _ => None,
        };
        let mut action = None;

        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                review_head(
                    ui,
                    &format!("Review of {name}"),
                    &format!("{agent} · {source}"),
                );
                Frame::new()
                    .inner_margin(Margin::same(theme::SPACE_4 as i8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if plan {
                            ui.label(
                                RichText::new(PLAN_NOTE)
                                    .font(theme::meta())
                                    .color(p.ink_muted),
                            );
                            ui.add_space(theme::SPACE_2);
                        }
                        action = review_body(ui, &review.status, drafts.as_deref_mut());
                        if matches!(
                            review.status,
                            ReviewStatus::Done { .. } | ReviewStatus::Failed { .. }
                        ) {
                            ui.add_space(theme::SPACE_2);
                            ui.add(
                                egui::TextEdit::singleline(&mut follow_up)
                                    .hint_text(
                                        RichText::new("Ask again: make it smaller…")
                                            .color(p.ink_faint),
                                    )
                                    .font(theme::ui_body())
                                    .desired_width(f32::INFINITY),
                            );
                        }
                    });
                Frame::new()
                    .inner_margin(Margin::symmetric(
                        theme::SPACE_4 as i8,
                        theme::SPACE_3 as i8,
                    ))
                    .show(ui, |ui| {
                        let kept = drafts
                            .iter()
                            .flatten()
                            .filter(|draft| draft.include)
                            .count();
                        let asked = !follow_up.trim().is_empty();
                        action = review_footer(ui, &review.status, asked, kept).or(action.take());
                    });
            });

        if let Some(review) = self.session.review.as_mut() {
            review.follow_up = follow_up;
            if let (Some(drafts), ReviewStatus::SplitReady { drafts: kept, .. }) =
                (drafts, &mut review.status)
            {
                *kept = drafts;
            }
        }
        self.apply_panel_action(action);
    }

    fn apply_panel_action(&mut self, action: Option<PanelAction>) {
        match action {
            Some(PanelAction::Cancel) => self.close_review(),
            Some(PanelAction::Dismiss) => self.session.review = None,
            Some(PanelAction::Retry) => {
                let target = self
                    .session
                    .review
                    .as_ref()
                    .map(|review| (review.path.clone(), review.draft, review.mode));
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

/// The panel's `review-soft` head: "Review of x.md", and the agent, mode and
/// source on the right.
pub(super) fn review_head(ui: &mut egui::Ui, title: &str, detail: &str) {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.review_soft)
        // The header is flush with the panel's top edge, so its
        // fill has to round the top corners itself: egui frames
        // don't clip children, and a square fill would paint over
        // the panel's rounded corners and their border. One less
        // than RADIUS_MD matches the inside of the 1pt border.
        .corner_radius(CornerRadius {
            nw: theme::RADIUS_MD - 1,
            ne: theme::RADIUS_MD - 1,
            sw: 0,
            se: 0,
        })
        .inner_margin(Margin::symmetric(
            theme::SPACE_4 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            // Agent first, so a long file name truncates into the
            // space left instead of widening the panel (which
            // flickers while the spinner repaints). The row has a
            // fixed height: a bare right-to-left layout would
            // stretch to fill the whole panel.
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), theme::CONTROL_H),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.label(RichText::new(detail).font(theme::meta()).color(p.review));
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(title)
                                    .font(theme::ui_strong())
                                    .color(p.review),
                            )
                            .truncate(),
                        );
                    });
                },
            );
        });
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
fn review_body(
    ui: &mut egui::Ui,
    status: &ReviewStatus,
    drafts: Option<&mut [split::SplitDraft]>,
) -> Option<PanelAction> {
    let p = theme::palette(ui);
    let mut action = None;
    match status {
        ReviewStatus::Splitting {
            answer, progress, ..
        } => {
            let step = agent::step(progress).or(Some("The agent is cutting the issue…".into()));
            running_line(ui, answer.elapsed(), step);
        }
        ReviewStatus::Failed { error } => {
            ui.label(RichText::new(error).color(p.danger));
        }
        ReviewStatus::SplitReady { .. } => split::drafts_ui(ui, drafts.unwrap_or_default()),
        ReviewStatus::Done { raw, report: None } => {
            ui.label(RichText::new(raw).color(p.ink_muted));
        }
        ReviewStatus::Done {
            report: Some(report),
            ..
        } => {
            for finding in &report.findings {
                let (label, color) = match finding.verdict {
                    agent::Verdict::Pass => ("Pass", p.status_done),
                    agent::Verdict::Challenge => ("Challenge", p.review),
                    agent::Verdict::Block => ("Block", p.danger),
                };
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&finding.axis)
                            .font(theme::ui_strong())
                            .color(p.ink),
                    );
                    widgets::pill(ui, label, color);
                });
                if !finding.body.is_empty() {
                    ui.label(RichText::new(&finding.body).color(p.ink_muted));
                }
                if finding.axis == "Size" && finding.verdict != agent::Verdict::Pass {
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
fn review_footer(
    ui: &mut egui::Ui,
    status: &ReviewStatus,
    asked: bool,
    kept: usize,
) -> Option<PanelAction> {
    let mut action = None;
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), theme::CONTROL_H),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            let mut button = |ui: &mut egui::Ui, kind, label: &str, key, picked| {
                if widgets::button(ui, kind, None, label, key).clicked() {
                    action = Some(picked);
                }
            };
            match status {
                ReviewStatus::Splitting { .. } => {
                    button(
                        ui,
                        ButtonKind::Secondary,
                        "Cancel",
                        Some("esc"),
                        PanelAction::Cancel,
                    );
                }
                ReviewStatus::SplitReady { .. } => {
                    let create = format!("Create {kept} tickets");
                    let kind = if kept > 0 {
                        ButtonKind::Primary
                    } else {
                        ButtonKind::Disabled
                    };
                    button(ui, kind, &create, None, PanelAction::CreateSplit);
                    button(
                        ui,
                        ButtonKind::Secondary,
                        "Back to review",
                        None,
                        PanelAction::BackToReview,
                    );
                    button(ui, ButtonKind::Ghost, "Dismiss", None, PanelAction::Dismiss);
                }
                ReviewStatus::Done { .. } | ReviewStatus::Failed { .. } => {
                    let has_proposal = matches!(
                        status,
                        ReviewStatus::Done { report: Some(report), .. } if report.proposal.is_some()
                    );
                    ui.add_enabled_ui(has_proposal, |ui| {
                        button(
                            ui,
                            ButtonKind::Primary,
                            "Apply to draft",
                            None,
                            PanelAction::Apply,
                        );
                    });
                    if matches!(status, ReviewStatus::Failed { .. }) {
                        button(ui, ButtonKind::Secondary, "Retry", None, PanelAction::Retry);
                    }
                    if asked {
                        button(
                            ui,
                            ButtonKind::Secondary,
                            "Ask again",
                            None,
                            PanelAction::Again,
                        );
                    }
                    button(ui, ButtonKind::Ghost, "Dismiss", None, PanelAction::Dismiss);
                }
            }
        },
    );
    action
}

/// The spinner, the time the agent has taken, and the step it is on:
/// "0:42 · Reading app/review.rs…".
pub(super) fn running_line(ui: &mut egui::Ui, elapsed: Duration, step: Option<String>) {
    let p = theme::palette(ui);
    let seconds = elapsed.as_secs();
    let clock = format!("{}:{:02}", seconds / 60, seconds % 60);
    let step = step.unwrap_or_else(|| "The agent is reading the issue…".to_string());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        ui.spinner();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(clock).font(theme::meta()).color(p.ink_muted));
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(step).color(p.ink_muted)).truncate());
            });
        });
    });
}
