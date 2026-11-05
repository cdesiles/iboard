//! Menus: their options, group heads, two-line rows, separators and notes.

use std::ops::Range;

use egui::text::LayoutJob;
use egui::{
    Color32, CornerRadius, FontId, Frame, Painter, Pos2, Rect, Response, RichText, Sense, Stroke,
    StrokeKind, TextFormat, Ui, Vec2,
};

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
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            ui.is_enabled(),
            option.checked,
            option.name,
        )
    });
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

/// A menu's group head, "PROJECTS 12": the title in uppercase `ui-label`,
/// then the count, if any, in `meta`, both `ink-muted`.
pub fn menu_head(ui: &mut Ui, title: &str, count: &str) {
    let p = theme::palette(ui);
    let title = RichText::new(title.to_uppercase())
        .font(theme::ui_label())
        .color(p.ink_muted);
    if count.is_empty() {
        ui.label(title);
        return;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        ui.label(title);
        ui.label(RichText::new(count).font(theme::meta()).color(p.ink_muted));
    });
}

/// The [`menu_head`] of a group of menu options, in line with their names:
/// "FAVORITES".
pub fn menu_group(ui: &mut Ui, text: &str) {
    ui.add_space(theme::SPACE_2);
    Frame::new()
        .inner_margin(Vec2::new(theme::SPACE_3, 0.0))
        .show(ui, |ui| menu_head(ui, text, ""));
}

/// A lifted row's slight tilt, as the design system draws it.
const LIFTED_TILT_DEG: f32 = -0.6;

/// Height of a two-line [`menu_row`].
pub const MENU_ROW_H: f32 = 40.0;

/// What sets a [`menu_row`] apart.
#[derive(Clone, Default)]
pub struct RowLook {
    /// What is on screen, like the open project: the title in `accent`.
    pub current: bool,
    /// The keyboard's row: an `accent-soft` fill.
    pub active: bool,
    /// The bytes of the title a filter matched, on `accent-soft`.
    pub mark: Option<Range<usize>>,
    /// A row of a list being reordered.
    pub drag: Option<Drag>,
}

/// A row that drags: a 6-dot grip at its left, `ink-muted` on hover, and a
/// `grab` cursor.
#[derive(Clone, Copy)]
pub enum Drag {
    Grip,
    /// The slot of the row being dragged.
    Placeholder,
}

impl RowLook {
    /// The slot a dragged row left behind is faded.
    fn fade(&self, color: Color32) -> Color32 {
        match self.drag {
            Some(Drag::Placeholder) => color.gamma_multiply(0.35),
            _ => color,
        }
    }
}

/// A full-width popup row: a title, a muted detail line under it, and a
/// `surface-hover` fill on hover; its [`RowLook`] adds the rest.
pub fn menu_row(ui: &mut Ui, width: f32, title: &str, detail: &str, look: RowLook) -> Response {
    let p = theme::palette(ui);
    let (sense, cursor) = match look.drag {
        Some(_) => (Sense::click_and_drag(), egui::CursorIcon::Grab),
        None => (Sense::click(), egui::CursorIcon::PointingHand),
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, MENU_ROW_H), sense);
    let response = response.on_hover_cursor(cursor);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let fill = if look.active {
            p.accent_soft
        } else if hovered {
            p.surface_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), look.fade(fill));
        paint_row(ui, ui.painter(), rect, (title, detail), &look, hovered);
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// The [`menu_row`] being dragged, raised and tilted under the pointer like a
/// dragged card.
pub fn menu_row_lifted(ui: &Ui, (title, detail): (&str, &str), pointer: Pos2) {
    let p = theme::palette(ui);
    let width = ui.max_rect().width();
    let rect = Rect::from_min_size(
        Pos2::new(ui.max_rect().left(), pointer.y - MENU_ROW_H / 2.0),
        Vec2::new(width, MENU_ROW_H),
    );
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("menu_row_lifted"));
    let painter = ui.ctx().layer_painter(layer);
    let shadow = ui.visuals().popup_shadow;
    painter.add(shadow.as_shape(rect, CornerRadius::same(theme::RADIUS_SM)));
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS_SM),
        p.surface,
        Stroke::new(1.0, p.line_strong),
        StrokeKind::Inside,
    );
    let look = RowLook {
        drag: Some(Drag::Grip),
        ..RowLook::default()
    };
    paint_row(ui, &painter, rect, (title, detail), &look, true);
    super::tilt_layer(ui.ctx(), layer, rect.center(), LIFTED_TILT_DEG);
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
}

/// A row's grip, if it drags, then its title, the marked part on
/// `accent-soft`, and its detail under it.
fn paint_row(
    ui: &Ui,
    painter: &Painter,
    rect: Rect,
    (title, detail): (&str, &str),
    look: &RowLook,
    hovered: bool,
) {
    let p = theme::palette(ui);
    let mut inner = rect.shrink2(Vec2::new(theme::SPACE_3, theme::SPACE_2));
    if look.drag.is_some() {
        let grip = Rect::from_min_size(
            Pos2::new(inner.left() - 2.0, rect.center().y - theme::ICON_SIZE / 2.0),
            Vec2::splat(theme::ICON_SIZE),
        );
        let color = if hovered { p.ink_muted } else { p.ink_faint };
        theme::paint_icon(painter, Icon::Grip, grip, look.fade(color));
        inner.min.x = grip.right() + theme::SPACE_2;
    }
    let color = look.fade(if look.current { p.accent } else { p.ink });
    let plain = TextFormat::simple(theme::ui_strong(), color);
    let mut job = LayoutJob::default();
    match look
        .mark
        .clone()
        .filter(|range| title.get(range.clone()).is_some())
    {
        Some(range) => {
            job.append(&title[..range.start], 0.0, plain.clone());
            let marked = TextFormat {
                background: p.accent_soft,
                ..plain.clone()
            };
            job.append(&title[range.clone()], 0.0, marked);
            job.append(&title[range.end..], 0.0, plain);
        }
        None => job.append(title, 0.0, plain),
    }
    job.wrap.max_width = inner.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let title = ui.fonts_mut(|f| f.layout_job(job));
    let detail_color = look.fade(p.ink_faint);
    let detail = ui.fonts_mut(|f| {
        let mut job = LayoutJob::simple_singleline(detail.to_string(), theme::meta(), detail_color);
        job.wrap.max_width = inner.width();
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        f.layout_job(job)
    });
    let gap = theme::SPACE_1;
    let top = inner.center().y - (title.size().y + detail.size().y + gap) / 2.0;
    painter.galley(Pos2::new(inner.min.x, top), title.clone(), color);
    painter.galley(
        Pos2::new(inner.min.x, top + title.size().y + gap),
        detail,
        detail_color,
    );
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
        .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
        .show(ui, |ui| help(ui, text, color));
}
