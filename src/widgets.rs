//! Controls of the design system that egui does not draw the same way:
//! buttons, the Auto checkbox, status pills, keyboard hints and breadcrumbs.
//! See `design/components/*/README.md` for what each one is for.

use egui::{
    Color32, CornerRadius, FontId, Galley, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2, WidgetText,
};
use std::sync::Arc;

use crate::theme::{self, Icon};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// `surface` fill, `line-strong` edge: Refresh, Discard.
    Secondary,
    /// `accent` fill: the one main action of a screen.
    Primary,
    /// No edge, `ink-muted`: navigation and icon buttons.
    Ghost,
}

fn galley(ui: &Ui, text: &str, font: FontId, color: Color32) -> Arc<Galley> {
    WidgetText::from(egui::RichText::new(text).font(font).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    )
}

/// Paints the 2pt focus ring outside a focused control.
fn focus_ring(ui: &Ui, response: &Response, radius: u8) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            CornerRadius::same(radius + 2),
            Stroke::new(2.0, theme::palette(ui).accent),
            StrokeKind::Outside,
        );
    }
}

/// A 24pt button with an optional icon and an inline shortcut hint. An empty
/// label makes a 24×24 icon button.
pub fn button(
    ui: &mut Ui,
    kind: ButtonKind,
    icon: Option<Icon>,
    label: &str,
    shortcut: Option<&str>,
) -> Response {
    let p = theme::palette(ui);
    let (fill, hover_fill, edge, ink) = match kind {
        ButtonKind::Secondary => (p.surface, p.surface_hover, p.line_strong, p.ink),
        ButtonKind::Primary => (p.accent, p.accent, p.accent, p.on_accent),
        ButtonKind::Ghost => (
            Color32::TRANSPARENT,
            p.surface_hover,
            Color32::TRANSPARENT,
            p.ink_muted,
        ),
    };

    let text = (!label.is_empty()).then(|| galley(ui, label, theme::ui_button(), ink));
    let hint = shortcut.map(|s| galley(ui, s, theme::meta(), ink));
    let gap = 6.0;
    let width = if text.is_none() && hint.is_none() {
        theme::CONTROL_H
    } else {
        let mut w = 2.0 * theme::SPACE_3;
        if icon.is_some() {
            w += theme::ICON_SIZE + gap;
        }
        if let Some(text) = &text {
            w += text.size().x;
        }
        if let Some(hint) = &hint {
            w += gap + hint.size().x;
        }
        w
    };

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let ink = if kind == ButtonKind::Ghost && hovered {
            p.ink
        } else {
            ink
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            if hovered { hover_fill } else { fill },
            Stroke::new(1.0, edge),
            StrokeKind::Inside,
        );

        let center_y = rect.center().y;
        let mut x = if text.is_none() && hint.is_none() {
            rect.center().x - theme::ICON_SIZE / 2.0
        } else {
            rect.min.x + theme::SPACE_3
        };
        if let Some(icon) = icon {
            let icon_rect = Rect::from_min_size(
                Pos2::new(x, center_y - theme::ICON_SIZE / 2.0),
                Vec2::splat(theme::ICON_SIZE),
            );
            theme::paint_icon(painter, icon, icon_rect, ink);
            x += theme::ICON_SIZE + gap;
        }
        if let Some(text) = text {
            let size = text.size();
            painter.galley_with_override_text_color(
                Pos2::new(x, center_y - size.y / 2.0),
                text.clone(),
                ink,
            );
            x += size.x + gap;
        }
        if let Some(hint) = hint {
            let size = hint.size();
            painter.galley_with_override_text_color(
                Pos2::new(x, center_y - size.y / 2.0),
                hint,
                ink,
            );
        }
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// An icon-only 24×24 ghost button with a caller-chosen id, so tests can find
/// it. Used for the per-card actions (delete, archive).
pub fn icon_button(ui: &mut Ui, icon: Icon, id: egui::Id, hover: &str) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(theme::CONTROL_H), Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hover);
    if ui.is_rect_visible(rect) {
        let p = theme::palette(ui);
        let hovered = response.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
        }
        let ink = if hovered { p.ink } else { p.ink_muted };
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(theme::ICON_SIZE));
        theme::paint_icon(ui.painter(), icon, icon_rect, ink);
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// The 14pt Auto checkbox: `accent` fill and an `on-accent` tick when checked.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, label: &str) -> Response {
    let p = theme::palette(ui);
    let text = galley(ui, label, theme::ui_button(), p.ink);
    let box_size = 14.0;
    let width = box_size + 6.0 + text.size().x;
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let box_rect = Rect::from_min_size(
            Pos2::new(rect.min.x, rect.center().y - box_size / 2.0),
            Vec2::splat(box_size),
        );
        if *checked {
            painter.rect_filled(box_rect, CornerRadius::same(3), p.accent);
            let at = |x: f32, y: f32| box_rect.min + Vec2::new(x, y);
            painter.line(
                vec![at(3.5, 7.0), at(6.0, 9.5), at(10.5, 4.5)],
                Stroke::new(2.0, p.on_accent),
            );
        } else {
            painter.rect(
                box_rect,
                CornerRadius::same(3),
                p.surface,
                Stroke::new(1.0, p.line_strong),
                StrokeKind::Inside,
            );
        }
        let pos = Pos2::new(box_rect.max.x + 6.0, rect.center().y - text.size().y / 2.0);
        painter.galley(pos, text, p.ink);
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

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

/// Draws a key cap (`⌘S`, `esc`) at `pos` (left, vertically centred) and
/// returns its width. Used where the hint sits on top of another widget.
pub fn paint_kbd(ui: &Ui, key: &str, left_center: Pos2) -> f32 {
    let p = theme::palette(ui);
    let text = galley(ui, key, theme::meta(), p.ink_muted);
    let size = Vec2::new((text.size().x + 8.0).max(18.0), 16.0);
    let rect = Rect::from_min_size(Pos2::new(left_center.x, left_center.y - size.y / 2.0), size);
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS_SM),
        p.surface,
        Stroke::new(1.0, p.line_strong),
        StrokeKind::Inside,
    );
    // The heavier bottom edge that makes it read as a key.
    painter.line_segment(
        [
            Pos2::new(rect.min.x + 2.0, rect.max.y - 1.0),
            Pos2::new(rect.max.x - 2.0, rect.max.y - 1.0),
        ],
        Stroke::new(1.0, p.line_strong),
    );
    let text_pos = rect.center() - text.size() / 2.0;
    painter.galley(text_pos, text, p.ink_muted);
    size.x
}

/// A key cap followed by what it does, as a hint row in the status bar.
pub fn hint(ui: &mut Ui, key: &str, action: &str) {
    let p = theme::palette(ui);
    let text = galley(ui, key, theme::meta(), p.ink_muted);
    let key_width = (text.size().x + 8.0).max(18.0);
    ui.spacing_mut().item_spacing.x = theme::SPACE_2;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(key_width, 16.0), Sense::hover());
    paint_kbd(ui, key, Pos2::new(rect.min.x, rect.center().y));
    ui.label(
        egui::RichText::new(action)
            .font(theme::meta())
            .color(p.ink_muted),
    );
}

/// A breadcrumb segment that links back: `ink-muted`, `surface-hover` on hover.
pub fn crumb(ui: &mut Ui, text: &str, font: FontId) -> Response {
    let p = theme::palette(ui);
    let normal = galley(ui, text, font.clone(), p.ink_muted);
    let padding = Vec2::new(6.0, 3.0);
    let (rect, response) = ui.allocate_exact_size(normal.size() + 2.0 * padding, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let color = if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
            p.ink
        } else {
            p.ink_muted
        };
        ui.painter()
            .galley_with_override_text_color(rect.min + padding, normal, color);
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}
