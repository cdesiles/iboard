//! Menus: their options, group heads, two-line rows, separators and notes.

use egui::{Color32, CornerRadius, FontId, Frame, Margin, Pos2, Rect, Response, Sense, Ui, Vec2};

use super::{focus_ring, galley, galley_truncated, help, rule};
use crate::theme::{self, Icon};

/// One 24pt row of a select menu: a tick when `checked`, the name (in mono
/// for model IDs, `ink-muted` when `muted`), and a `meta` note at the right.
/// `active` is the keyboard highlight, an `accent-soft` fill.
pub struct MenuOption<'a> {
    pub name: &'a str,
    pub mono: bool,
    pub muted: bool,
    pub meta: &'a str,
    pub checked: bool,
    pub active: bool,
}

pub fn menu_option(ui: &mut Ui, option: MenuOption<'_>) -> Response {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let fill = if option.active {
            p.accent_soft
        } else if response.hovered() {
            p.surface_hover
        } else {
            Color32::TRANSPARENT
        };
        painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), fill);
        let check = Rect::from_min_size(
            Pos2::new(
                rect.min.x + theme::SPACE_3,
                rect.center().y - theme::ICON_SIZE / 2.0,
            ),
            Vec2::splat(theme::ICON_SIZE),
        );
        if option.checked {
            theme::paint_icon(painter, Icon::Check, check, p.accent);
        }
        let mut right = rect.max.x - theme::SPACE_3;
        if !option.meta.is_empty() {
            let meta = galley(ui, option.meta, theme::meta(), p.ink_muted);
            right -= meta.size().x;
            painter.galley(
                Pos2::new(right, rect.center().y - meta.size().y / 2.0),
                meta,
                p.ink_muted,
            );
            right -= theme::SPACE_3;
        }
        let left = check.max.x + theme::SPACE_3;
        let font = if option.mono {
            FontId::monospace(12.0)
        } else {
            theme::ui_body()
        };
        let color = if option.muted { p.ink_muted } else { p.ink };
        let name = galley_truncated(ui, option.name, font, color, (right - left).max(0.0));
        painter.galley(
            Pos2::new(left, rect.center().y - name.size().y / 2.0),
            name,
            color,
        );
    }
    response
}

/// The uppercase heading of a group of menu options: "FAVORITES".
pub fn menu_group(ui: &mut Ui, text: &str) {
    let p = theme::palette(ui);
    ui.add_space(theme::SPACE_2);
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(theme::SPACE_3 as i8, 0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text.to_uppercase())
                    .font(theme::ui_label())
                    .color(p.ink_muted),
            );
        });
}

/// Height of a two-line [`menu_row`].
const MENU_ROW_H: f32 = 38.0;

/// A full-width popup row: a title, a muted detail line under it, and a
/// `surface-hover` fill on hover. `current` tints the title with `accent`, so
/// the project already open stands out in the recent list.
pub fn menu_row(ui: &mut Ui, width: f32, title: &str, detail: &str, current: bool) -> Response {
    let p = theme::palette(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, MENU_ROW_H), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
        }
        let inner = rect.shrink2(Vec2::new(theme::SPACE_3, theme::SPACE_2));
        let title_color = if current { p.accent } else { p.ink };
        let title = galley_truncated(ui, title, theme::ui_strong(), title_color, inner.width());
        let detail = galley_truncated(ui, detail, theme::meta(), p.ink_faint, inner.width());
        let gap = theme::SPACE_1;
        let top = inner.center().y - (title.size().y + detail.size().y + gap) / 2.0;
        let painter = ui.painter();
        painter.galley(Pos2::new(inner.min.x, top), title.clone(), title_color);
        painter.galley(
            Pos2::new(inner.min.x, top + title.size().y + gap),
            detail,
            p.ink_faint,
        );
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// The `line` rule between groups of a menu.
pub fn menu_separator(ui: &mut Ui) {
    ui.add_space(theme::SPACE_2);
    rule(ui);
    ui.add_space(theme::SPACE_2);
}

/// A line of text inside a menu: loading, or why the list failed.
pub fn menu_note(ui: &mut Ui, text: &str, color: Color32) {
    Frame::new()
        .inner_margin(Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_2 as i8,
        ))
        .show(ui, |ui| help(ui, text, color));
}
