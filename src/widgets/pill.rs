//! Pills and chips: status and label pills, filter tokens, chips and tags.

use egui::{
    Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use super::galley;
use crate::labels::Rgb;
use crate::theme::{self, Icon};

/// An 18pt pill with a dot and a word, outlined in its state color.
pub fn pill(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let p = theme::palette(ui);
    let text = galley(
        ui,
        text,
        FontId::new(11.0, theme::ui_button().family),
        color,
    );
    let size = Vec2::new(7.0 + 6.0 + 4.0 + text.size().x + 7.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(theme::RADIUS_PILL),
            p.surface,
            Stroke::new(1.0, color),
            StrokeKind::Inside,
        );
        painter.circle_filled(Pos2::new(rect.min.x + 10.0, rect.center().y), 3.0, color);
        let pos = Pos2::new(rect.min.x + 17.0, rect.center().y - text.size().y / 2.0);
        painter.galley(pos, text, color);
    }
    response
}

/// An 18pt label chip: the preset colour as a soft tint behind its lowercased
/// name, in `meta`. A colour too faint to read on the tint falls back to `ink`;
/// a `None` colour (an unknown label) is neutral: `surface-sunken` on
/// `ink-muted`, so the chip is legible in both themes.
pub fn label_pill(ui: &mut Ui, name: &str, color: Option<Rgb>) -> Response {
    let p = theme::palette(ui);
    let name = name.to_lowercase();
    let (fill, ink) = match color.map(theme::rgb) {
        Some(color) => {
            let fill = theme::tint(color, p.surface);
            let ink = if theme::contrast_ratio(color, fill) >= 4.5 {
                color
            } else {
                p.ink
            };
            (fill, ink)
        }
        None => (p.surface_sunken, p.ink_muted),
    };
    let text = galley(ui, &name, FontId::new(11.0, theme::ui_button().family), ink);
    let size = Vec2::new(7.0 + text.size().x + 7.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_PILL), fill);
        let pos = rect.center() - text.size() / 2.0 - Vec2::new(0.0, 1.0);
        painter.galley(pos, text, ink);
    }
    response
}

/// A picked filter keyword: `accent` on `accent-soft`, with a × that removes
/// it. Returns the ×'s response.
pub fn filter_token(ui: &mut Ui, text: &str) -> Response {
    let p = theme::palette(ui);
    let label = galley(ui, text, theme::meta(), p.accent);
    let remove = 14.0;
    let size = Vec2::new(6.0 + label.size().x + 2.0 + remove + 2.0, 18.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let remove_rect = Rect::from_center_size(
        Pos2::new(rect.max.x - 2.0 - remove / 2.0, rect.center().y),
        Vec2::splat(remove),
    );
    let response = ui
        .interact(
            remove_rect,
            ui.id().with(("filter-token", text)),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Remove {text}"));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.accent_soft);
        if response.hovered() {
            painter.rect_filled(remove_rect, CornerRadius::same(3), p.surface);
        }
        let pos = Pos2::new(rect.min.x + 6.0, rect.center().y - label.size().y / 2.0);
        painter.galley(pos, label, p.accent);
        let icon = Rect::from_center_size(remove_rect.center(), Vec2::splat(10.0));
        theme::paint_icon(painter, Icon::Close, icon, p.accent);
    }
    response
}

/// An 18pt suggestion chip: `line-strong` edge, `accent` on `accent-soft`
/// when hovered or reached with the arrow keys (`current`).
pub fn chip(ui: &mut Ui, text: &str, current: bool) -> Response {
    let p = theme::palette(ui);
    let label = galley(ui, text, theme::meta(), p.ink);
    let size = Vec2::new(7.0 + label.size().x + 7.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let lit = current || response.hovered();
        let (fill, edge, ink) = if lit {
            (p.accent_soft, p.accent, p.accent)
        } else {
            (p.surface, p.line_strong, p.ink)
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(theme::RADIUS_PILL),
            fill,
            Stroke::new(1.0, edge),
            StrokeKind::Inside,
        );
        let pos = rect.center() - label.size() / 2.0;
        painter.galley(pos, label, ink);
        if current {
            painter.rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(theme::RADIUS_PILL + 2),
                Stroke::new(2.0, p.accent),
                StrokeKind::Outside,
            );
        }
    }
    response
}

/// An 18pt tag in `meta`: `line-strong` edge on `surface`, or `accent` on
/// `accent-soft` when `inferred`. Clickable when the caller acts on it.
pub fn tag(ui: &mut Ui, text: &str, inferred: bool) -> Response {
    let p = theme::palette(ui);
    let (fill, edge, ink) = if inferred {
        (p.accent_soft, Color32::TRANSPARENT, p.accent)
    } else {
        (p.surface, p.line_strong, p.ink_muted)
    };
    let label = galley(ui, text, theme::meta(), ink);
    let size = Vec2::new(6.0 + label.size().x + 6.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() && !inferred {
            p.surface_hover
        } else {
            fill
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            fill,
            Stroke::new(1.0, edge),
            StrokeKind::Inside,
        );
        let pos = rect.center() - label.size() / 2.0;
        ui.painter().galley(pos, label, ink);
    }
    response
}
