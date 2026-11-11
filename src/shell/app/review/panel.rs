//! The review panel beside the page: the findings and the actions on them,
//! the split they led to, or the run under way.

use super::model::{REVIEW_KEY, REVIEW_READY, REVIEW_READY_HINT, verdict_look};
use super::{
    Align, ButtonKind, CommonMarkCache, CommonMarkViewer, Duration, Frame, IboardApp, Layout, Path,
    ReviewMode, ReviewState, RichText, Split, Vec2, agent, file_name, side_panel, split, theme,
    widgets,
};

/// What the review panel says of a review run in plan mode.
pub(in crate::shell::app) const PLAN_NOTE: &str =
    "Plan mode: the agent could not change the repository";

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

impl IboardApp {
    /// Review ready beside its button once the answer on `path` is in: a
    /// click shows it.
    pub(in crate::shell::app) fn ready_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        if !self.session.review_runs.is_ready(path) {
            return;
        }
        if widgets::button(ui, ButtonKind::Review, None, REVIEW_READY, None)
            .on_hover_text(REVIEW_READY_HINT)
            .clicked()
        {
            self.show_past_review(path.to_path_buf());
        }
    }

    /// The review panel beside the page: the agent's three findings and the
    /// actions on them, or the tickets of a split. The agent only ever
    /// proposes; Apply opens the editor, Create writes the tickets.
    pub(in crate::shell::app) fn review_panel(&mut self, ui: &mut egui::Ui) {
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
    pub(in crate::shell::app) fn running_review_panel(&mut self, ui: &mut egui::Ui, path: &Path) {
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
                widgets::button(ui, ButtonKind::Secondary, None, "Cancel", Some(REVIEW_KEY))
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

/// A footer button: its kind, label and key, and what it asks for, `None`
/// while it is disabled.
type FooterButton = (
    ButtonKind,
    String,
    Option<&'static str>,
    Option<PanelAction>,
);

/// The footer's buttons now, right to left: the main action first.
fn footer_actions(review: &ReviewState) -> Vec<FooterButton> {
    let button = |kind, label: &str, key, then| (kind, label.to_string(), key, Some(then));
    match &review.split {
        Some(Split::Cutting { .. }) => vec![button(
            ButtonKind::Secondary,
            "Cancel",
            Some("esc"),
            PanelAction::Cancel,
        )],
        Some(Split::Ready(drafts)) => {
            let kept = drafts.iter().filter(|draft| draft.include).count();
            let create = format!("Create {kept} tickets");
            vec![
                button(
                    ButtonKind::Primary.when(kept > 0),
                    &create,
                    None,
                    PanelAction::CreateSplit,
                ),
                button(
                    ButtonKind::Secondary,
                    "Back to review",
                    None,
                    PanelAction::BackToReview,
                ),
                button(ButtonKind::Ghost, "Dismiss", None, PanelAction::Dismiss),
            ]
        }
        None | Some(Split::Failed(_)) => {
            let (kind, ask) = match review.follow_up.as_deref().map(str::trim) {
                None => (ButtonKind::Secondary, PanelAction::OpenFollowUp),
                Some("") => (ButtonKind::Secondary.when(false), PanelAction::Again),
                Some(_) => (ButtonKind::Secondary, PanelAction::Again),
            };
            let apply = review.proposal().map(|_| PanelAction::Apply);
            vec![
                (
                    ButtonKind::Primary,
                    "Apply to draft".into(),
                    Some("⌘↩"),
                    apply,
                ),
                button(kind, "Ask again", None, ask),
                button(
                    ButtonKind::Ghost,
                    "Dismiss",
                    Some("esc"),
                    PanelAction::Dismiss,
                ),
            ]
        }
    }
}

fn review_footer(ui: &mut egui::Ui, review: &ReviewState) -> Option<PanelAction> {
    let mut action = None;
    widgets::footer_bar(ui, theme::SPACE_4, |ui| {
        for (kind, label, key, then) in footer_actions(review) {
            ui.add_enabled_ui(then.is_some(), |ui| {
                if widgets::button(ui, kind, None, &label, key).clicked() {
                    action = then;
                }
            });
        }
    });
    action
}

/// The Thinking mark, the time the agent has taken, and the step it is on:
/// "42s · Reading app/review.rs…".
fn running_line(ui: &mut egui::Ui, elapsed: Duration, step: Option<String>) {
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
