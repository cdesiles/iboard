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
    /// `review` text and edge on `surface`: the AI Review button.
    Review,
    /// Filled `danger`: the confirming button of a destructive dialog.
    Danger,
    /// `danger` text and edge on `surface`: Delete in the Reader and Editor
    /// header, which still asks to confirm.
    DangerOutline,
    /// A secondary button with a leading chevron and its key in `ink-muted`:
    /// Board, at the far left of the Reader and Editor header.
    Back,
    /// Keeps the shape but drops to a `line` edge and `ink-faint`.
    Disabled,
    /// A secondary button whose popup is open (Add label): `accent` edge and
    /// text on `accent-soft`.
    Open,
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

/// The fill, hover fill, edge and ink of a button kind.
fn button_colors(p: &theme::Palette, kind: ButtonKind) -> (Color32, Color32, Color32, Color32) {
    match kind {
        ButtonKind::Secondary | ButtonKind::Back => {
            (p.surface, p.surface_hover, p.line_strong, p.ink)
        }
        ButtonKind::Primary => (p.accent, p.accent, p.accent, p.on_accent),
        ButtonKind::Ghost => (
            Color32::TRANSPARENT,
            p.surface_hover,
            Color32::TRANSPARENT,
            p.ink_muted,
        ),
        ButtonKind::Review => (p.surface, p.review_soft, p.review, p.review),
        ButtonKind::Danger => (p.danger, p.danger, p.danger, p.on_danger),
        ButtonKind::DangerOutline => (p.surface, p.danger_soft, p.danger, p.danger),
        ButtonKind::Disabled => (p.surface, p.surface, p.line, p.ink_faint),
        ButtonKind::Open => (p.accent_soft, p.accent_soft, p.accent, p.accent),
    }
}

/// The laid-out content of a button: its label and shortcut galleys, and its
/// width.
struct ButtonContent {
    icon: Option<Icon>,
    text: Option<Arc<Galley>>,
    hint: Option<Arc<Galley>>,
    width: f32,
}

const BUTTON_GAP: f32 = 6.0;

/// The Back button sits its chevron closer to the edge and to the label.
const BACK_PAD_LEFT: f32 = 4.0;
const BACK_GAP: f32 = 4.0;

/// Left padding and icon-to-label gap of a button kind.
fn button_insets(kind: ButtonKind) -> (f32, f32) {
    if kind == ButtonKind::Back {
        (BACK_PAD_LEFT, BACK_GAP)
    } else {
        (theme::SPACE_3, BUTTON_GAP)
    }
}

fn button_content(
    ui: &Ui,
    kind: ButtonKind,
    icon: Option<Icon>,
    label: &str,
    shortcut: Option<&str>,
) -> ButtonContent {
    let ink = button_colors(theme::palette(ui), kind).3;
    let text = (!label.is_empty()).then(|| galley(ui, label, theme::ui_button(), ink));
    let hint = shortcut.map(|s| galley(ui, s, theme::meta(), ink));
    let (pad_left, icon_gap) = button_insets(kind);
    let width = if text.is_none() && hint.is_none() {
        theme::CONTROL_H
    } else {
        let mut w = pad_left + theme::SPACE_3;
        if icon.is_some() {
            w += theme::ICON_SIZE + icon_gap;
        }
        if let Some(text) = &text {
            w += text.size().x;
        }
        if let Some(hint) = &hint {
            w += BUTTON_GAP + hint.size().x;
        }
        w
    };
    ButtonContent {
        icon,
        text,
        hint,
        width,
    }
}

/// Paints a button's box and content into `rect`.
fn paint_button(
    ui: &Ui,
    rect: Rect,
    response: &Response,
    kind: ButtonKind,
    content: ButtonContent,
    radius: CornerRadius,
) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let p = theme::palette(ui);
    let (fill, hover_fill, edge, ink) = button_colors(p, kind);
    let hovered = response.hovered();
    let ink = if kind == ButtonKind::Ghost && hovered {
        p.ink
    } else {
        ink
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        radius,
        if hovered { hover_fill } else { fill },
        Stroke::new(1.0, edge),
        StrokeKind::Inside,
    );

    let ButtonContent {
        icon, text, hint, ..
    } = content;
    let center_y = rect.center().y;
    let (pad_left, icon_gap) = button_insets(kind);
    let mut x = if text.is_none() && hint.is_none() {
        rect.center().x - theme::ICON_SIZE / 2.0
    } else {
        rect.min.x + pad_left
    };
    if let Some(icon) = icon {
        let icon_rect = Rect::from_min_size(
            Pos2::new(x, center_y - theme::ICON_SIZE / 2.0),
            Vec2::splat(theme::ICON_SIZE),
        );
        theme::paint_icon(painter, icon, icon_rect, ink);
        x += theme::ICON_SIZE + icon_gap;
    }
    if let Some(text) = text {
        let size = text.size();
        painter.galley_with_override_text_color(
            Pos2::new(x, center_y - size.y / 2.0),
            text.clone(),
            ink,
        );
        x += size.x + BUTTON_GAP;
    }
    if let Some(hint) = hint {
        let size = hint.size();
        let hint_ink = if kind == ButtonKind::Back {
            p.ink_muted
        } else {
            ink
        };
        painter.galley_with_override_text_color(
            Pos2::new(x, center_y - size.y / 2.0),
            hint,
            hint_ink,
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
    let content = button_content(ui, kind, icon, label, shortcut);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(content.width, theme::CONTROL_H), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    paint_button(
        ui,
        rect,
        &response,
        kind,
        content,
        CornerRadius::same(theme::RADIUS_SM),
    );
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// Width of the caret half of a [`split_button`].
const SPLIT_CARET_W: f32 = 22.0;

/// A button with a caret that opens a menu (the Review split button). Returns
/// the main part's response, then the caret's; their ids derive from `id`.
pub fn split_button(
    ui: &mut Ui,
    id: egui::Id,
    kind: ButtonKind,
    icon: Option<Icon>,
    label: &str,
    shortcut: Option<&str>,
    main_enabled: bool,
) -> (Response, Response) {
    let main_kind = if main_enabled {
        kind
    } else {
        ButtonKind::Disabled
    };
    let content = button_content(ui, main_kind, icon, label, shortcut);
    let size = Vec2::new(content.width + SPLIT_CARET_W, theme::CONTROL_H);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let (main_rect, caret_rect) = rect.split_left_right_at_x(rect.min.x + content.width);
    let main = if main_enabled {
        ui.interact(main_rect, id.with("main"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        ui.interact(main_rect, id.with("main"), Sense::hover())
    };
    let caret = ui
        .interact(caret_rect, id.with("caret"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let r = theme::RADIUS_SM;
    paint_button(
        ui,
        main_rect,
        &main,
        main_kind,
        content,
        CornerRadius {
            nw: r,
            sw: r,
            ne: 0,
            se: 0,
        },
    );
    // The caret shares the main part's right edge as its divider.
    let caret_content = button_content(ui, kind, Some(Icon::ChevronDown), "", None);
    paint_button(
        ui,
        caret_rect.with_min_x(caret_rect.min.x - 1.0),
        &caret,
        kind,
        caret_content,
        CornerRadius {
            nw: 0,
            sw: 0,
            ne: r,
            se: r,
        },
    );
    focus_ring(ui, &main, theme::RADIUS_SM);
    focus_ring(ui, &caret, theme::RADIUS_SM);
    (main, caret)
}

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

/// The size of a per-card action button.
pub const CARD_ACTION: f32 = 22.0;

/// What a card action turns to on its own hover.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    /// Delete: `danger` on hover only.
    Danger,
    /// Review: drawn in the reserved `review` hue, rest and hover.
    Review,
    /// The trigger of an open popup (Add label): `accent` edge and icon on
    /// `accent-soft`, hovered or not.
    Open,
}

/// An icon-only 22×22 card action with a caller-chosen id, so tests can find
/// it: no edge at rest, `surface-hover` and a `line-strong` edge on its own
/// hover. A `Danger` action (delete) turns `danger` only then.
pub fn icon_button(ui: &mut Ui, icon: Icon, id: egui::Id, hover: &str, tone: Tone) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(CARD_ACTION), Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hover);
    if ui.is_rect_visible(rect) {
        let p = theme::palette(ui);
        let hovered = response.hovered() || tone == Tone::Open;
        let ink = match (hovered, tone) {
            (_, Tone::Review) => p.review,
            (_, Tone::Open) => p.accent,
            (true, Tone::Danger) => p.danger,
            (true, Tone::Plain) => p.ink,
            _ => p.ink_muted,
        };
        if hovered {
            let edge = match tone {
                Tone::Danger => p.danger,
                Tone::Review => p.review,
                Tone::Open => p.accent,
                Tone::Plain => p.line_strong,
            };
            ui.painter().rect(
                rect,
                CornerRadius::same(theme::RADIUS_SM),
                match tone {
                    Tone::Review => p.review_soft,
                    Tone::Open => p.accent_soft,
                    _ => p.surface_hover,
                },
                Stroke::new(1.0, edge),
                StrokeKind::Inside,
            );
        }
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

/// One tab of the [`column_tabs`] segmented track.
pub struct ColumnTab<'a> {
    pub name: &'a str,
    pub count: usize,
    /// The column's status colour: its dot and the active underline.
    pub color: Color32,
    pub active: bool,
}

/// A galley with letter spacing, for the uppercase `ui-label` of a column tab.
fn tracked_galley(ui: &Ui, text: &str, font: FontId, color: Color32, tracking: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font,
            color,
            extra_letter_spacing: tracking,
            ..Default::default()
        },
    );
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// What [`column_tabs`] reported: the tab that was clicked, and where each tab
/// was drawn (so the caller can make them drop targets).
pub struct ColumnTabs {
    pub clicked: Option<usize>,
    pub tab_rects: Vec<Rect>,
}

/// The `surface-sunken` segmented track of ColumnFocus column tabs: each tab
/// has a status dot, its uppercase `ui-label` name and its count. The active
/// tab sits on `surface` with a 1pt `line-strong` ring and a 2pt status
/// underline.
pub fn column_tabs(ui: &mut Ui, tabs: &[ColumnTab]) -> ColumnTabs {
    let p = theme::palette(ui);
    let pad = 2.0;
    let gap = 2.0;
    let text_gap = 6.0;
    let side = 8.0;
    let dot = 8.0;
    let tab_h = 22.0;

    let names: Vec<Arc<Galley>> = tabs
        .iter()
        .map(|tab| {
            tracked_galley(
                ui,
                &tab.name.to_uppercase(),
                theme::ui_label(),
                p.ink,
                theme::LABEL_TRACKING,
            )
        })
        .collect();
    let counts: Vec<Arc<Galley>> = tabs
        .iter()
        .map(|tab| galley(ui, &tab.count.to_string(), theme::meta(), p.ink_muted))
        .collect();
    let widths: Vec<f32> = names
        .iter()
        .zip(&counts)
        .map(|(name, count)| {
            side + dot + text_gap + name.size().x + text_gap + count.size().x + side
        })
        .collect();
    let total =
        pad * 2.0 + widths.iter().sum::<f32>() + gap * (tabs.len().saturating_sub(1)) as f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total, tab_h + pad * 2.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            p.surface_sunken,
            Stroke::new(1.0, p.line_strong),
            StrokeKind::Inside,
        );
    }

    let mut clicked = None;
    let mut tab_rects = Vec::with_capacity(tabs.len());
    let mut x = rect.min.x + pad;
    for (index, tab) in tabs.iter().enumerate() {
        let tab_rect = Rect::from_min_size(
            Pos2::new(x, rect.min.y + pad),
            Vec2::new(widths[index], tab_h),
        );
        tab_rects.push(tab_rect);
        let response = ui
            .interact(
                tab_rect,
                egui::Id::new(("coltab", tab.name)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            clicked = Some(index);
        }
        if ui.is_rect_visible(tab_rect) {
            let painter = ui.painter();
            if tab.active {
                painter.rect(
                    tab_rect,
                    CornerRadius::same(3),
                    p.surface,
                    Stroke::new(1.0, p.line_strong),
                    StrokeKind::Inside,
                );
                painter.line_segment(
                    [
                        Pos2::new(tab_rect.min.x + 1.0, tab_rect.max.y - 1.0),
                        Pos2::new(tab_rect.max.x - 1.0, tab_rect.max.y - 1.0),
                    ],
                    Stroke::new(2.0, tab.color),
                );
            }
            let ink = if tab.active || response.hovered() {
                p.ink
            } else {
                p.ink_muted
            };
            let cy = tab_rect.center().y;
            painter.circle_filled(
                Pos2::new(tab_rect.min.x + side + dot / 2.0, cy),
                dot / 2.0,
                tab.color,
            );
            let name = &names[index];
            let name_x = tab_rect.min.x + side + dot + text_gap;
            painter.galley(
                Pos2::new(name_x, cy - name.size().y / 2.0),
                name.clone(),
                ink,
            );
            let count = &counts[index];
            painter.galley(
                Pos2::new(
                    tab_rect.max.x - side - count.size().x,
                    cy - count.size().y / 2.0,
                ),
                count.clone(),
                if tab.active { p.ink_muted } else { p.ink_faint },
            );
        }
        x += widths[index] + gap;
    }
    ColumnTabs { clicked, tab_rects }
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

/// An 18pt label chip: the preset colour as a soft tint behind its lowercased
/// name, in `meta`. A colour too faint to read on the tint falls back to `ink`;
/// a `None` colour (an unknown label) is neutral: `surface-sunken` on
/// `ink-muted`, so the chip is legible in both themes.
pub fn label_pill(ui: &mut Ui, name: &str, color: Option<Color32>) -> Response {
    let p = theme::palette(ui);
    let name = name.to_lowercase();
    let (fill, ink) = match color {
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
        painter.galley(rect.center() - text.size() / 2.0, text, ink);
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

/// The width `text` takes in `font`, for laying out a row before drawing it.
pub fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    galley(ui, text, font, Color32::PLACEHOLDER).size().x
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
                ("bug", Some(Color32::from_rgb(0xb1, 0x2f, 0x22))),
                ("spike", None),
                ("faint", Some(Color32::from_rgb(0xf5, 0xf5, 0xf5))),
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
