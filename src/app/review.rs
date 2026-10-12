//! The AI review: running it, its panel, and applying its proposal.

use super::*;

impl IboardApp {
    /// The text an agent should read: the saved file, or the editor draft when
    /// the review is of the current work.
    pub(super) fn review_content(&self, path: &Path, draft: bool) -> Option<String> {
        if draft {
            self.editor
                .as_ref()
                .filter(|editor| editor.path == path)
                .map(|editor| editor.text.clone())
        } else {
            self.board.find(path).map(|issue| issue.markdown.clone())
        }
    }

    /// Stores a running review of `content` and hands the prompt to the agent.
    pub(super) fn run_review(&mut self, target: ReviewTarget, content: &str, prompt: String) {
        if !self.agent.configured() {
            self.open_config();
            return;
        }
        let (answer, progress) =
            agent::start_review(&self.agent, &prompt, &target.path, target.mode);
        self.mark_notes_sent(&target.path);
        self.review = Some(ReviewState {
            path: target.path,
            draft: target.draft,
            mode: target.mode,
            agent: self.agent.label(),
            status: ReviewStatus::Running { answer, progress },
            hash: reviews::content_hash(content),
            follow_up: String::new(),
        });
    }

    /// Starts a Quick review of the open issue (or the editor draft).
    pub(super) fn start_review(&mut self, path: &Path, draft: bool) {
        self.start_review_in(path, draft, ReviewMode::Quick);
    }

    /// Starts a one-shot review in `mode`.
    pub(super) fn start_review_in(&mut self, path: &Path, draft: bool, mode: ReviewMode) {
        if !self.is_todo_issue(path) {
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
        let Some(review) = self.review.as_ref() else {
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

    /// Cancels a running review (dropping the job stops the agent), or closes
    /// a finished one, which stays behind the card's pill.
    pub(super) fn cancel_review(&mut self) {
        self.review = None;
    }

    /// The card's verdict pill: opens the issue with its last review.
    pub(super) fn show_past_review(&mut self, path: PathBuf) {
        self.open_issue(path.clone());
        if let Some(past) = self.past_reviews.get(&path).cloned() {
            self.review = Some(ReviewState {
                path,
                draft: past.draft,
                mode: past.mode,
                agent: past.agent,
                status: ReviewStatus::Done {
                    raw: past.raw,
                    report: past.report,
                },
                // Only a running review is saved, so this one needs none.
                hash: String::new(),
                follow_up: String::new(),
            });
        }
    }

    /// Adopts the agent's answer once it is in; called every frame.
    pub(super) fn poll_review(&mut self) {
        let Some(review) = self.review.as_mut() else {
            return;
        };
        let ReviewStatus::Running { answer, .. } = &review.status else {
            return;
        };
        let Some(result) = answer.answer() else {
            return;
        };
        let raw = match result {
            Ok(raw) => raw,
            Err(error) => {
                review.status = ReviewStatus::Failed { error };
                return;
            }
        };
        let stored = reviews::StoredReview {
            hash: review.hash.clone(),
            agent: review.agent.clone(),
            mode: review.mode.key().to_string(),
            draft: review.draft,
            at: chrono::Local::now().to_rfc3339(),
            raw: raw.clone(),
        };
        review.status = ReviewStatus::Done {
            report: agent::parse(&raw),
            raw,
        };
        let path = review.path.clone();
        if let Err(err) = reviews::save_review(&self.board.root, &path, stored.clone()) {
            self.error = Some(err);
        }
        if let Some(issue) = self.board.find(&path) {
            let past = PastReview::new(&stored, &issue.markdown);
            self.past_reviews.insert(path, past);
        }
    }

    /// Whether a review is running for the issue currently in the reader.
    pub(super) fn review_is_running(&self, path: &Path) -> bool {
        self.review.as_ref().is_some_and(|review| {
            review.path == path && matches!(review.status, ReviewStatus::Running { .. })
        })
    }

    /// The proposed rewrite, when the agent offered one: the editor opens with
    /// it as an unsaved draft, so saving or discarding is the accept or refuse.
    pub(super) fn apply_review_proposal(&mut self) {
        let Some(review) = self.review.as_ref() else {
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
            .editor
            .as_ref()
            .is_some_and(|editor| editor.path == path)
        {
            self.start_editing(&path);
        }
        if let Some(editor) = self.editor.as_mut()
            && editor.path == path
        {
            editor.text = proposal;
            editor.last_edit = Some(Instant::now());
        }
        self.detail_mode = DetailMode::Edit;
        self.review = None;
    }

    /// The Review split button, rightmost in the Reader and Editor header: the
    /// main part runs the review (or cancels a running one), the caret opens a
    /// menu naming what runs and leading to the AI settings.
    pub(super) fn review_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let running = self.review_is_running(path);
        let configured = self.agent.configured();
        let review_label = if running { "Reviewing…" } else { "Review" };
        let review_hint = if running {
            "Cancel the review (esc)"
        } else if configured {
            "Ask an agent to challenge this issue"
        } else {
            "No agent is set up yet: choose one in Settings › AI review (⌘, or the caret)"
        };
        let (main, caret) = widgets::split_button(
            ui,
            egui::Id::new("review-button"),
            ButtonKind::Review,
            Some(Icon::Review),
            review_label,
            Some(if running { "esc" } else { "⌘⇧R" }),
            running || configured,
        );
        let draft = self.detail_mode == DetailMode::Edit;
        if main.on_hover_text(review_hint).clicked() {
            if running {
                self.cancel_review();
            } else if configured {
                self.start_review(path, draft);
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
            Some(ReviewMenu::Review | ReviewMenu::Deep) if running => {}
            Some(ReviewMenu::Review) if configured => self.start_review(path, draft),
            Some(ReviewMenu::Deep) if configured => {
                self.start_review_in(path, draft, ReviewMode::Deep);
            }
            Some(ReviewMenu::Review | ReviewMenu::Deep | ReviewMenu::Settings) => {
                self.open_config();
            }
            None => {}
        }
    }

    /// The review panel beside the page: the agent's three findings and the
    /// actions on them. The agent only ever proposes; Apply opens the editor.
    pub(super) fn review_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let Some(review) = self.review.as_ref() else {
            return;
        };
        let name = file_name(&review.path);
        let source = if review.draft { "draft" } else { "file" };
        let agent = format!("{} · {}", review.agent, review.mode.key());
        let mut follow_up = review.follow_up.clone();
        let mut dismiss = false;
        let mut again = false;
        let mut apply = false;
        let mut cancel = false;
        let mut retry = false;
        let running = matches!(review.status, ReviewStatus::Running { .. });

        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
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
                                ui.label(
                                    RichText::new(format!("{agent} · {source}"))
                                        .font(theme::meta())
                                        .color(p.review),
                                );
                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(format!("Review of {name}"))
                                                .font(theme::ui_strong())
                                                .color(p.review),
                                        )
                                        .truncate(),
                                    );
                                });
                            },
                        );
                    });
                Frame::new()
                    .inner_margin(Margin::same(theme::SPACE_4 as i8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match &review.status {
                            ReviewStatus::Running { answer, progress } => {
                                running_line(ui, answer.elapsed(), agent::step(progress));
                            }
                            ReviewStatus::Failed { error } => {
                                ui.label(RichText::new(error).color(p.danger));
                            }
                            ReviewStatus::Done { raw, report } => match report {
                                Some(report) => {
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
                                            ui.label(
                                                RichText::new(&finding.body).color(p.ink_muted),
                                            );
                                        }
                                        ui.add_space(theme::SPACE_4);
                                    }
                                }
                                None => {
                                    ui.label(RichText::new(raw).color(p.ink_muted));
                                }
                            },
                        }
                        if !running {
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
                        ui.allocate_ui_with_layout(
                            Vec2::new(ui.available_width(), theme::CONTROL_H),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                                let has_proposal = matches!(
                                    &review.status,
                                    ReviewStatus::Done { report: Some(report), .. }
                                        if report.proposal.is_some()
                                );
                                ui.add_enabled_ui(has_proposal, |ui| {
                                    if widgets::button(
                                        ui,
                                        ButtonKind::Primary,
                                        None,
                                        "Apply to draft",
                                        None,
                                    )
                                    .clicked()
                                    {
                                        apply = true;
                                    }
                                });
                                if running {
                                    if widgets::button(
                                        ui,
                                        ButtonKind::Secondary,
                                        None,
                                        "Cancel",
                                        Some("esc"),
                                    )
                                    .clicked()
                                    {
                                        cancel = true;
                                    }
                                } else {
                                    if matches!(review.status, ReviewStatus::Failed { .. })
                                        && widgets::button(
                                            ui,
                                            ButtonKind::Secondary,
                                            None,
                                            "Retry",
                                            None,
                                        )
                                        .clicked()
                                    {
                                        retry = true;
                                    }
                                    if !follow_up.trim().is_empty()
                                        && widgets::button(
                                            ui,
                                            ButtonKind::Secondary,
                                            None,
                                            "Ask again",
                                            None,
                                        )
                                        .clicked()
                                    {
                                        again = true;
                                    }
                                    if widgets::button(ui, ButtonKind::Ghost, None, "Dismiss", None)
                                        .clicked()
                                    {
                                        dismiss = true;
                                    }
                                }
                            },
                        );
                    });
            });

        if let Some(review) = self.review.as_mut() {
            review.follow_up = follow_up;
        }
        if cancel {
            self.cancel_review();
        } else if dismiss {
            self.review = None;
        } else if retry {
            let target = self
                .review
                .as_ref()
                .map(|review| (review.path.clone(), review.draft, review.mode));
            if let Some((path, draft, mode)) = target {
                self.start_review_in(&path, draft, mode);
            }
        } else if again {
            self.ask_again();
        } else if apply {
            self.apply_review_proposal();
        }
    }
}

/// The spinner, the time the agent has taken, and the step it is on:
/// "0:42 · Reading app/review.rs…".
fn running_line(ui: &mut egui::Ui, elapsed: Duration, step: Option<String>) {
    let p = theme::palette(ui);
    let seconds = elapsed.as_secs();
    let clock = format!("{}:{:02}", seconds / 60, seconds % 60);
    let step = step.unwrap_or_else(|| "The agent is reading the issue…".to_string());
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(RichText::new(clock).font(theme::meta()).color(p.ink_muted));
        ui.add(egui::Label::new(RichText::new(step).color(p.ink_muted)).truncate());
    });
}
