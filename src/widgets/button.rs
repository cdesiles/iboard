//! Buttons: text, split and icon buttons, in every kind the design system has.

use std::sync::Arc;

use egui::{
    Color32, CornerRadius, Galley, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use super::{focus_ring, galley};
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
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
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
    // The tooltip ends with its key ("Delete  Del"); the name is what comes first.
    let name = hover.split("  ").next().unwrap_or(hover);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
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
