//! The bell and the tray under it: the AI-builder questions waiting in Doing.
//! A question is answered in its ticket, never in the tray.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::{
    Align, CornerRadius, Frame, IboardApp, Icon, Layout, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2, file_name, theme, widgets,
};
use crate::format::relative_age;

/// One row: a ticket whose build waits on a question.
struct Row {
    path: PathBuf,
    title: String,
    detail: String,
    since: Instant,
}

/// The bell's hit area.
const BELL: f32 = 24.0;
/// The tray's width.
const TRAY_WIDTH: f32 = 420.0;
/// The tallest the rows scroll in.
const TRAY_MAX_HEIGHT: f32 = 420.0;

impl IboardApp {
    /// How many tickets wait on an answer.
    #[cfg(test)]
    pub(super) fn notification_count(&self) -> usize {
        self.notification_rows().len()
    }

    /// The questions the tray lists: one row per blocked ticket, the build
    /// that waited longest on top.
    fn notification_rows(&self) -> Vec<Row> {
        let mut questions = self.pending_questions_since();
        questions.sort_by_key(|(_, _, since)| *since);
        let mut rows: Vec<Row> = Vec::new();
        for (path, question, since) in questions {
            if rows.iter().any(|row| row.path == path) {
                continue;
            }
            let title = self
                .board
                .find(&path)
                .map_or_else(|| file_name(&path), |issue| issue.title.clone());
            rows.push(Row {
                path,
                title,
                detail: question.title.clone(),
                since,
            });
        }
        rows
    }

    /// The bell in the top bar, and the tray under it. A click on a row opens
    /// its ticket, where the question is answered.
    pub(super) fn notifications_button(&mut self, ui: &mut egui::Ui) {
        let rows = self.notification_rows();
        let count = rows.len();
        let p = theme::palette(ui);
        let (rect, button) = ui.allocate_exact_size(Vec2::splat(BELL), Sense::click());
        let name = match count {
            0 => "Notifications".to_string(),
            _ => format!("Notifications, {count} waiting"),
        };
        let button = button
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(&name);
        button
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &name));
        let fade = widgets::hover_fade(ui.ctx(), button.id, button.hovered());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(theme::RADIUS_SM),
            p.surface_hover.gamma_multiply(fade),
        );
        let bell = Rect::from_center_size(rect.center(), Vec2::splat(18.0));
        theme::paint_icon_stroked(ui.painter(), Icon::Bell, bell, p.ink_muted, 1.8);
        if count > 0 {
            paint_badge(ui, rect, count);
        }
        widgets::focus_ring(ui, &button, theme::RADIUS_SM);

        let popup_id = egui::Popup::default_response_id(&button);
        let was_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        let gap = (ui.max_rect().bottom() - rect.bottom()).max(0.0) + theme::SPACE_2;
        let picked = egui::Popup::from_toggle_button_response(&button)
            .width(TRAY_WIDTH)
            .gap(gap)
            .align(egui::RectAlign::BOTTOM_END)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .frame(widgets::popover_frame(ui).inner_margin(0.0))
            .accessible_name("Notifications")
            .show(|ui| {
                ui.set_width(TRAY_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                notifications_body(ui, &rows, !was_open)
            })
            .and_then(|inner| inner.inner);
        if let Some(path) = picked {
            egui::Popup::close_id(ui.ctx(), popup_id);
            self.open_issue(path);
        } else if was_open && !egui::Popup::is_id_open(ui.ctx(), popup_id) {
            button.request_focus();
        }
    }
}

/// The tray: its head, then the group of questions or the empty state.
fn notifications_body(ui: &mut egui::Ui, rows: &[Row], focus_first: bool) -> Option<PathBuf> {
    let p = theme::palette(ui);
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        Vec2::new(width, theme::BAR_H - 1.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add_space(theme::SPACE_4);
            ui.label(
                RichText::new("Notifications")
                    .font(theme::ui_title())
                    .color(p.ink),
            );
        },
    );
    widgets::rule(ui);
    if rows.is_empty() {
        tray_empty(ui);
        return None;
    }
    ui.add_space(theme::SPACE_3);
    Frame::new()
        .inner_margin(Vec2::new(theme::SPACE_4, 0.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new("Waiting on you".to_uppercase())
                    .font(theme::ui_label())
                    .color(p.ink_muted),
            );
        });
    ui.add_space(theme::SPACE_2);
    let mut picked = None;
    egui::ScrollArea::vertical()
        .id_salt("notifications")
        .max_height(TRAY_MAX_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let mut responses = Vec::with_capacity(rows.len());
            for (index, row) in rows.iter().enumerate() {
                let response = notification_row(ui, row, index);
                if response.clicked() {
                    picked = Some(row.path.clone());
                }
                responses.push(response);
            }
            arrow_keys(ui, &responses);
            if focus_first && let Some(first) = responses.first() {
                first.request_focus();
            }
        });
    picked
}

/// What the tray says when no build waits: the head stays, no group label.
fn tray_empty(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    Frame::new()
        .inner_margin(Vec2::new(theme::SPACE_4, theme::SPACE_4))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                ui.label(
                    RichText::new("No questions waiting.")
                        .font(theme::ui_strong())
                        .color(p.ink),
                );
                ui.label(
                    RichText::new("Questions from the AI builder land here.")
                        .font(theme::ui_small())
                        .color(p.ink_muted),
                );
            });
        });
}

/// `↑ ↓` move the keyboard between the tray's rows.
fn arrow_keys(ui: &egui::Ui, rows: &[egui::Response]) {
    let Some(current) = ui
        .memory(|memory| memory.focused())
        .and_then(|focused| rows.iter().position(|row| row.id == focused))
    else {
        return;
    };
    let next = if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
        Some((current + 1).min(rows.len() - 1))
    } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
        Some(current.saturating_sub(1))
    } else {
        None
    };
    if let Some(next) = next {
        rows[next].request_focus();
    }
}

/// One 56pt row at least: a speech bubble, the ticket, the question under it
/// and how long the build has waited at the top right.
fn notification_row(ui: &mut egui::Ui, row: &Row, index: usize) -> egui::Response {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let pad = Vec2::new(theme::SPACE_4, theme::SPACE_3);
    let gap = theme::SPACE_3;
    let time_text = relative_age(row.since.elapsed());
    let time =
        ui.fonts_mut(|fonts| fonts.layout_no_wrap(time_text.clone(), theme::meta(), p.ink_faint));
    let body_width =
        (width - 2.0 * pad.x - theme::STATUS_ICON - 2.0 * gap - time.size().x).max(0.0);
    let title = wrapped(ui, &row.title, theme::ui_strong(), p.ink, body_width, 1);
    let detail = (!row.detail.is_empty()).then(|| {
        wrapped(
            ui,
            &row.detail,
            theme::ui_small(),
            p.ink_muted,
            body_width,
            2,
        )
    });
    let body_height = title.size().y
        + detail
            .as_ref()
            .map_or(0.0, |detail| theme::SPACE_1 + detail.size().y);
    let height = (2.0 * pad.y + body_height).max(56.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let label = if row.detail.is_empty() {
        format!("{}: {}", row.title, time_text)
    } else {
        format!("{}: {}, {}", row.title, row.detail, time_text)
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
    if ui.is_rect_visible(rect) {
        let fade = widgets::hover_fade(ui.ctx(), response.id, response.hovered());
        let title_height = title.size().y;
        let time_width = time.size().x;
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius::ZERO,
            p.surface_hover.gamma_multiply(fade),
        );
        if index > 0 {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(rect.left(), rect.top()),
                    Vec2::new(rect.width(), 1.0),
                ),
                CornerRadius::ZERO,
                p.line_soft,
            );
        }
        let bubble = Rect::from_min_size(
            Pos2::new(rect.left() + pad.x, rect.top() + pad.y + theme::SPACE_1),
            Vec2::splat(theme::STATUS_ICON),
        );
        theme::paint_icon(painter, Icon::Bubble, bubble, p.status_doing);
        let left = rect.left() + pad.x + theme::STATUS_ICON + gap;
        painter.galley(Pos2::new(left, rect.top() + pad.y), title, p.ink);
        if let Some(detail) = detail {
            painter.galley(
                Pos2::new(left, rect.top() + pad.y + title_height + theme::SPACE_1),
                detail,
                p.ink_muted,
            );
        }
        painter.galley(
            Pos2::new(rect.right() - pad.x - time_width, rect.top() + pad.y),
            time,
            p.ink_faint,
        );
        if response.has_focus() {
            painter.rect_stroke(
                rect,
                CornerRadius::ZERO,
                Stroke::new(2.0, p.accent),
                StrokeKind::Inside,
            );
        }
    }
    response
}

/// `text` over at most `rows` lines, cut with an ellipsis at `width`.
fn wrapped(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    width: f32,
    rows: usize,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap.max_width = width.max(0.0);
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = true;
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// The count on the bell: 1 to 9 read as the number, 10 or more as 9+.
fn badge_text(count: usize) -> String {
    if count > 9 {
        "9+".to_string()
    } else {
        count.to_string()
    }
}

/// The `status-doing` count on the bell, on a `bg` ring.
fn paint_badge(ui: &egui::Ui, bell: Rect, count: usize) {
    let p = theme::palette(ui);
    let galley = ui.fonts_mut(|fonts| {
        fonts.layout_no_wrap(badge_text(count), theme::ui_label(), p.on_primary)
    });
    let width = (galley.size().x + 2.0 * theme::SPACE_2).max(16.0);
    let badge = Rect::from_min_size(
        Pos2::new(bell.right() + theme::SPACE_2 - width, bell.top() - 3.0),
        Vec2::new(width, 16.0),
    );
    let painter = ui.painter();
    painter.rect_filled(
        badge.expand(theme::SPACE_1),
        CornerRadius::same(theme::RADIUS_PILL),
        p.bg,
    );
    painter.rect_filled(
        badge,
        CornerRadius::same(theme::RADIUS_PILL),
        p.status_doing,
    );
    painter.galley(
        Pos2::new(
            badge.center().x - galley.size().x / 2.0,
            badge.center().y - galley.size().y / 2.0,
        ),
        galley,
        p.on_primary,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_keeps_the_ticket_and_the_question() {
        let row = Row {
            path: PathBuf::from("doing/a.md"),
            title: "Create test issue".into(),
            detail: "Which approach?".into(),
            since: Instant::now(),
        };
        assert_eq!(row.title, "Create test issue");
        assert_eq!(row.detail, "Which approach?");
    }

    #[test]
    fn the_badge_reads_nine_plus_from_ten_waiting() {
        assert_eq!(badge_text(1), "1");
        assert_eq!(badge_text(9), "9");
        assert_eq!(badge_text(10), "9+");
    }
}
