//! The Question card: its head, bounded body and pinned foot.

use super::fields::{answer_body, error_line, permission_outside, project_label};
use super::{
    Align, AnswerAction, AnswerDraft, ButtonKind, CornerRadius, FieldKind, Frame, Icon, Instant,
    Layout, Path, PermissionReply, Question, QuestionKind, RichText, Sense, Stroke, Vec2,
    question_heading, theme, widgets,
};

pub(super) fn answer_card(
    ui: &mut egui::Ui,
    question: &Question,
    draft: &mut AnswerDraft,
    max_height: f32,
    since: Option<Instant>,
    project: Option<&Path>,
) -> Option<AnswerAction> {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let heading = question_heading(question);
    let outside = matches!(&question.kind, QuestionKind::Permission(permission) if permission_outside(permission, project));
    let mut action = None;
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_soft))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .show(ui, |ui| {
            ui.set_width(width);
            let head = Frame::new()
                .inner_margin(Vec2::new(theme::SPACE_4, theme::SPACE_4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    answer_head(ui, &heading, since);
                })
                .response
                .rect
                .height();
            let foot = theme::CONTROL_H + 2.0 * theme::SPACE_3 + 1.0;
            let body_max = (max_height - head - foot - theme::SPACE_4).max(48.0);
            let sending = draft.reply.is_some();
            Frame::new()
                .inner_margin(Vec2::new(theme::SPACE_4, 0.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add_enabled_ui(!sending, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("answer_body", question.id.as_str()))
                            .max_height(body_max)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                answer_body(ui, draft, question, &heading, outside, project);
                            });
                    });
                    ui.add_space(theme::SPACE_4);
                });
            card_rule(ui);
            action = answer_foot(ui, question, draft, outside, project);
        });
    action
}

/// A question the board cannot answer: the tool waits in its own terminal,
/// and the card gives the command that reaches it. No field, no Send.
pub(super) fn waiting_card(
    ui: &mut egui::Ui,
    question: &Question,
    max_height: f32,
    since: Option<Instant>,
) {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let heading = question_heading(question);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_soft))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .show(ui, |ui| {
            ui.set_width(width);
            let head = Frame::new()
                .inner_margin(Vec2::new(theme::SPACE_4, theme::SPACE_4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    answer_head(ui, &heading, since);
                })
                .response
                .rect
                .height();
            let foot = theme::CONTROL_H + 2.0 * theme::SPACE_3 + 1.0;
            let body_max = (max_height - head - foot - theme::SPACE_4).max(48.0);
            Frame::new()
                .inner_margin(Vec2::new(theme::SPACE_4, 0.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical()
                        .id_salt(("waiting_body", question.id.as_str()))
                        .max_height(body_max)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            widgets::help(
                                ui,
                                "This build waits on you in Claude Code. Answer it there; \
                                 the board picks the build up again.",
                                p.ink_muted,
                            );
                        });
                    ui.add_space(theme::SPACE_4);
                });
            card_rule(ui);
            waiting_foot(ui, question);
        });
}

fn waiting_foot(ui: &mut egui::Ui, question: &Question) {
    let QuestionKind::Waiting { command } = &question.kind else {
        return;
    };
    let p = theme::palette(ui);
    Frame::new()
        .inner_margin(Vec2::new(theme::SPACE_4, theme::SPACE_3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            Frame::new()
                .fill(p.surface_sunken)
                .corner_radius(CornerRadius::same(theme::RADIUS_SM))
                .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            RichText::new(command).font(theme::edit_body()).color(p.ink),
                        )
                        .wrap(),
                    );
                });
        });
}

fn answer_head(ui: &mut egui::Ui, heading: &str, since: Option<Instant>) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        let (icon, _) = ui.allocate_exact_size(Vec2::splat(theme::ICON_SIZE), Sense::hover());
        theme::paint_icon(ui.painter(), Icon::Bubble, icon, p.review);
        ui.label(
            RichText::new("THE AGENT ASKS")
                .font(theme::ui_label())
                .extra_letter_spacing(theme::LABEL_TRACKING)
                .color(p.review),
        );
        if let Some(waited) = waited(since) {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(waited).font(theme::meta()).color(p.ink_faint));
            });
        }
    });
    ui.add_space(theme::SPACE_2);
    ui.add(egui::Label::new(RichText::new(heading).font(theme::ui_title()).color(p.ink)).wrap());
}

fn waited(since: Option<Instant>) -> Option<String> {
    let secs = since?.elapsed().as_secs();
    Some(if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    })
}

fn card_rule(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, p.line_soft);
}

fn answer_foot(
    ui: &mut egui::Ui,
    question: &Question,
    draft: &AnswerDraft,
    outside: bool,
    project: Option<&Path>,
) -> Option<AnswerAction> {
    let sending = draft.reply.is_some();
    let failed = draft.failure.as_deref();
    let mut action = None;
    let pad = Vec2::new(theme::SPACE_4, theme::SPACE_3);
    Frame::new().inner_margin(pad).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), theme::CONTROL_H),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                ui.add_enabled_ui(!sending, |ui| match &question.kind {
                    QuestionKind::Form(fields) => {
                        if let Some(failure) = failed {
                            error_line(ui, failure);
                        }
                        if !external_only(fields) {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if failed.is_some() {
                                    if retry_button(ui).clicked() {
                                        action = Some(AnswerAction::Send);
                                    }
                                } else {
                                    let label = if sending { "Sending…" } else { "Send answer" };
                                    let kind = ButtonKind::Send.when(!sending);
                                    if widgets::button(ui, kind, None, label, Some("⌘↩")).clicked()
                                    {
                                        action = Some(AnswerAction::Send);
                                    }
                                }
                            });
                        }
                    }
                    QuestionKind::Permission(_) => {
                        if widgets::button(ui, ButtonKind::Delete, None, "Reject", None).clicked() {
                            action = Some(AnswerAction::Permission(PermissionReply::Reject));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let label = if sending { "Sending…" } else { "Allow once" };
                            let kind = ButtonKind::Send.when(!sending);
                            if widgets::button(ui, kind, None, label, Some("⌘↩")).clicked() {
                                action = Some(AnswerAction::Permission(PermissionReply::Once));
                            }
                            if !outside && let Some(project) = project {
                                let label =
                                    format!("Always allow in {}", project_label(Some(project)));
                                if widgets::button(ui, ButtonKind::Secondary, None, &label, None)
                                    .clicked()
                                {
                                    action =
                                        Some(AnswerAction::Permission(PermissionReply::Always));
                                }
                            }
                        });
                    }
                    QuestionKind::Waiting { .. } => {}
                });
            },
        );
    });
    action
}

fn retry_button(ui: &mut egui::Ui) -> egui::Response {
    let p = theme::palette(ui);
    let text = ui
        .painter()
        .layout_no_wrap("Retry".to_string(), theme::ui_button(), p.danger);
    let size = Vec2::new(text.size().x + 2.0 * theme::SPACE_3, theme::CONTROL_H);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Retry"));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.danger_soft);
    }
    ui.painter()
        .galley(rect.center() - text.size() / 2.0, text, p.danger);
    widgets::focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

fn external_only(fields: &[super::Field]) -> bool {
    let mut visible = fields.iter().filter(|field| !field.hidden);
    let Some(field) = visible.next() else {
        return false;
    };
    visible.next().is_none()
        && matches!(&field.kind, FieldKind::External { url } if !url.is_empty())
}
