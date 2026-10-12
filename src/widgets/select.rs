//! The select: a field that opens a menu of choices.

use egui::{CornerRadius, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::{focus_ring, galley_truncated};
use crate::theme::{self, Icon};

/// A full-width 24pt select: the chosen value, an optional `meta` hint and a
/// chevron. `open` draws the focus ring while its menu is showing.
pub fn select(ui: &mut Ui, id: egui::Id, value: &str, hint: Option<&str>, open: bool) -> Response {
    let width = ui.available_width();
    select_sized(ui, id, value, hint, open, width)
}

/// A select of a fixed `width`, for the ColumnFocus focus bar where the full
/// available width is not wanted.
pub fn select_sized(
    ui: &mut Ui,
    id: egui::Id,
    value: &str,
    hint: Option<&str>,
    open: bool,
    width: f32,
) -> Response {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            if response.hovered() {
                p.surface_hover
            } else {
                p.surface
            },
            Stroke::new(1.0, p.line_strong),
            StrokeKind::Inside,
        );
        let chevron = Rect::from_min_size(
            Pos2::new(
                rect.max.x - 6.0 - theme::ICON_SIZE,
                rect.center().y - theme::ICON_SIZE / 2.0,
            ),
            Vec2::splat(theme::ICON_SIZE),
        );
        theme::paint_icon(painter, Icon::ChevronDown, chevron, p.ink_muted);
        let mut right = chevron.min.x - theme::SPACE_3;
        if let Some(hint) = hint.filter(|hint| !hint.is_empty()) {
            let max = (right - rect.min.x - 80.0).max(0.0);
            let hint = galley_truncated(ui, hint, theme::meta(), p.ink_muted, max);
            right -= hint.size().x;
            painter.galley(
                Pos2::new(right, rect.center().y - hint.size().y / 2.0),
                hint,
                p.ink_muted,
            );
            right -= theme::SPACE_3;
        }
        let left = rect.min.x + theme::SPACE_3;
        let value = galley_truncated(ui, value, theme::ui_body(), p.ink, right - left);
        painter.galley(
            Pos2::new(left, rect.center().y - value.size().y / 2.0),
            value,
            p.ink,
        );
    }
    if open {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            CornerRadius::same(theme::RADIUS_SM + 1),
            Stroke::new(2.0, p.accent),
            StrokeKind::Outside,
        );
    } else {
        focus_ring(ui, &response, theme::RADIUS_SM);
    }
    response
}
