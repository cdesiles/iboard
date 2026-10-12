//! Controls of the design system that egui does not draw the same way:
//! buttons, selects, menus, pills, column tabs, the pieces dialogs share, the
//! Auto checkbox, keyboard hints and breadcrumbs. See
//! `design/components/*/README.md` for what each one is for.

use egui::{
    Color32, CornerRadius, FontId, Galley, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2, WidgetText,
};
use std::sync::Arc;

use crate::theme::{self, Icon};

mod button;
mod dialog;
mod menu;
mod pill;
mod select;
mod tabs;

pub use button::{ButtonKind, CARD_ACTION, Tone, button, icon_button, split_button};
pub use dialog::{
    FooterClick, field_label, focus_ring, help, modal_footer, modal_header, paint_focus_ring,
    popover_frame, rule, segmented, text_field,
};
pub use menu::{MenuOption, menu_group, menu_note, menu_option, menu_row, menu_separator};
pub use pill::{chip, filter_token, label_pill, pill, tag};
pub use select::{select, select_sized};
pub use tabs::{ColumnTab, ColumnTabs, column_tabs};

fn galley(ui: &Ui, text: &str, font: FontId, color: Color32) -> Arc<Galley> {
    WidgetText::from(egui::RichText::new(text).font(font).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    )
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
    // A 24pt hit area, however small the text.
    let size = Vec2::new(normal.size().x + 12.0, theme::CONTROL_H);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let text_rect = Rect::from_center_size(rect.center(), normal.size());
        let color = if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
            // Underlined 3pt below the baseline-ish bottom of the text.
            let y = text_rect.max.y + 1.0;
            ui.painter().line_segment(
                [Pos2::new(text_rect.min.x, y), Pos2::new(text_rect.max.x, y)],
                Stroke::new(1.0, p.ink),
            );
            p.ink
        } else {
            p.ink_muted
        };
        ui.painter()
            .galley_with_override_text_color(text_rect.min, normal, color);
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// A galley clipped to `max_width`, ending in an ellipsis when it overflows.
fn galley_truncated(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    WidgetText::from(egui::RichText::new(text).font(font).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        max_width,
        egui::FontSelection::Default,
    )
}

/// The width `text` takes in `font`, for laying out a row before drawing it.
pub fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    galley(ui, text, font, Color32::PLACEHOLDER).size().x
}

/// Tallest a ghost tile stretches to fill its cell.
const GHOST_MAX_H: f32 = 96.0;

/// A ghost tile for a section the issue lacks: a dashed `line-strong` edge,
/// `+` and the heading in `ui-strong` `ink-muted`, then a `ui-small` hint. On
/// hover it turns solid `accent` on `accent-soft`.
pub fn ghost_section(ui: &mut Ui, heading: &str, hint: Option<&str>) -> Response {
    let p = theme::palette(ui);
    let pad = Vec2::new(theme::SPACE_4, theme::SPACE_3);
    let width = ui.available_width();
    let hint = hint.map(|hint| {
        let mut job = egui::text::LayoutJob::simple(
            hint.to_string(),
            theme::ui_small(),
            p.ink_muted,
            width - 2.0 * pad.x,
        );
        job.wrap.max_rows = 2;
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    });
    let head_h = 20.0;
    // Fills a grid cell, so tiles in a row line up whatever their hint.
    let height = (2.0 * pad.y + head_h + hint.as_ref().map_or(0.0, |h| 2.0 + h.size().y))
        .max(ui.available_height().min(GHOST_MAX_H));
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let lit = response.hovered() || response.has_focus();
        let ink = if lit { p.accent } else { p.ink_muted };
        let painter = ui.painter();
        let radius = CornerRadius::same(theme::RADIUS_SM);
        if lit {
            painter.rect(
                rect,
                radius,
                p.accent_soft,
                Stroke::new(1.0, p.accent),
                StrokeKind::Inside,
            );
        } else {
            let r = rect.shrink(0.5);
            let corners = [
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
            ];
            for i in 0..4 {
                painter.extend(egui::Shape::dashed_line(
                    &[corners[i], corners[(i + 1) % 4]],
                    Stroke::new(1.0, p.line_strong),
                    4.0,
                    3.0,
                ));
            }
        }
        let head_center = rect.min.y + pad.y + head_h / 2.0;
        let icon = Rect::from_center_size(
            Pos2::new(rect.min.x + pad.x + theme::ICON_SIZE / 2.0, head_center),
            Vec2::splat(theme::ICON_SIZE),
        );
        theme::paint_icon(painter, Icon::Plus, icon, ink);
        let title = galley(ui, heading, theme::ui_strong(), ink);
        painter.galley(
            Pos2::new(icon.max.x + 6.0, head_center - title.size().y / 2.0),
            title,
            ink,
        );
        if let Some(hint) = hint {
            painter.galley(
                Pos2::new(rect.min.x + pad.x, rect.min.y + pad.y + head_h + 2.0),
                hint,
                p.ink_muted,
            );
        }
    }
    response
}

/// A 48pt `status-done` meter on a `surface-sunken` track, `done` of `total`.
pub fn meter(ui: &mut Ui, done: usize, total: usize) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(48.0, 4.0), Sense::hover());
    let painter = ui.painter();
    let radius = CornerRadius::same(2);
    painter.rect(
        rect,
        radius,
        p.surface_sunken,
        Stroke::new(1.0, p.line),
        StrokeKind::Inside,
    );
    if total > 0 && done > 0 {
        let share = (done as f32 / total as f32).min(1.0);
        let filled = Rect::from_min_size(rect.min, Vec2::new(rect.width() * share, rect.height()));
        painter.rect_filled(filled, radius, p.status_done);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::Rgb;

    #[test]
    fn a_menu_row_renders_its_two_lines() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
            ..Default::default()
        };
        ctx.run_ui(raw, |ui| {
            let current = menu_row(ui, 300.0, "moon-cull", "~/code/moon-cull", true);
            assert!(current.rect.width() > 0.0 && current.rect.height() > 0.0);
            let other = menu_row(ui, 300.0, "other", "~/code/other", false);
            assert!(other.rect.width() > 0.0);
        })
        .textures_delta
        .clear();
    }

    #[test]
    fn a_label_pill_renders_known_unknown_and_faint_colours() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
            ..Default::default()
        };
        ctx.run_ui(raw, |ui| {
            // A preset colour, an unknown label and a nearly-white colour that
            // must fall back to readable ink all draw at the same 18pt size.
            for (name, color) in [
                ("bug", Some(Rgb(0xb1, 0x2f, 0x22))),
                ("spike", None),
                ("faint", Some(Rgb(0xf5, 0xf5, 0xf5))),
            ] {
                let response = label_pill(ui, name, color);
                assert_eq!(response.rect.height(), 18.0);
                assert!(response.rect.width() > 0.0);
            }
        })
        .textures_delta
        .clear();
    }
}
