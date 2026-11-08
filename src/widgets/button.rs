//! Buttons: text, split and icon buttons, in every kind the design system has.

use std::sync::Arc;

use egui::{
    Color32, CornerRadius, Galley, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use super::{focus_ring, galley};
use crate::theme::{self, Icon};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonKind {
    /// No edge, no fill: Refresh, the controls of the header bar.
    Header,
    /// A `line-soft` hairline, no fill: modal feet, dialogs, content blocks.
    Secondary,
    /// `primary` fill: the one filled button of a screen.
    Primary,
    /// No edge, `ink-muted`: the quiet action beside a louder one.
    Ghost,
    /// `review` label, no edge: the AI split button in the header.
    Review,
    /// `review` label on a `line-soft` hairline, away from the header.
    ReviewHairline,
    /// Filled `review`: what sends to an agent, the one main action there.
    Send,
    /// Filled `danger`: the confirming button of a destructive dialog.
    Danger,
    /// `ink-muted` at rest, `danger` on `danger-soft` only under the pointer:
    /// Delete, which still asks to confirm.
    Delete,
    /// The header's first control: a leading chevron and its key, no edge.
    Back,
    /// A button whose popup or pane is open: `accent` on `accent-soft`.
    Open,
    /// A disabled [`Self::Header`], [`Self::Ghost`] or [`Self::Review`].
    DisabledBare,
    /// A disabled [`Self::Secondary`].
    DisabledHairline,
    /// A disabled filled button, on `surface-sunken`.
    DisabledFilled,
}

impl ButtonKind {
    /// This kind while `enabled`, else its disabled look, which keeps the
    /// shape and drops to `ink-faint`.
    #[must_use]
    pub fn when(self, enabled: bool) -> Self {
        if enabled {
            return self;
        }
        match self {
            Self::Primary | Self::Send | Self::Danger | Self::DisabledFilled => {
                Self::DisabledFilled
            }
            Self::Secondary | Self::ReviewHairline | Self::Open | Self::DisabledHairline => {
                Self::DisabledHairline
            }
            Self::Header
            | Self::Ghost
            | Self::Review
            | Self::Delete
            | Self::Back
            | Self::DisabledBare => Self::DisabledBare,
        }
    }
}

/// How a button kind paints: at rest, and under the pointer.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Look {
    fill: Color32,
    hover_fill: Color32,
    edge: Color32,
    ink: Color32,
    hover_ink: Color32,
    key: Color32,
}

/// What share of a filled button's label colour its key hint takes.
const FILLED_KEY: f32 = 0.72;

fn button_look(p: &theme::Palette, kind: ButtonKind) -> Look {
    let none = Color32::TRANSPARENT;
    let plain = |fill, hover_fill, edge, ink: Color32| Look {
        fill,
        hover_fill,
        edge,
        ink,
        hover_ink: ink,
        key: p.ink_muted,
    };
    let filled = |fill, hover_fill, ink: Color32| Look {
        key: ink.gamma_multiply(FILLED_KEY),
        ..plain(fill, hover_fill, none, ink)
    };
    match kind {
        ButtonKind::Header | ButtonKind::Back => plain(none, p.surface_hover, none, p.ink),
        ButtonKind::Secondary => plain(none, p.surface_hover, p.line_soft, p.ink),
        ButtonKind::Primary => filled(p.primary, p.primary_hover, p.on_primary),
        ButtonKind::Ghost => Look {
            hover_ink: p.ink,
            ..plain(none, p.surface_hover, none, p.ink_muted)
        },
        ButtonKind::Review => plain(none, p.review_soft, none, p.review),
        ButtonKind::ReviewHairline => plain(none, p.review_soft, p.line_soft, p.review),
        ButtonKind::Send => filled(p.review, p.review, p.on_review),
        ButtonKind::Danger => filled(p.danger, p.danger, p.on_danger),
        ButtonKind::Delete => Look {
            hover_ink: p.danger,
            ..plain(none, p.danger_soft, none, p.ink_muted)
        },
        ButtonKind::Open => plain(p.accent_soft, p.accent_soft, p.accent, p.accent),
        ButtonKind::DisabledBare => plain(none, none, none, p.ink_faint),
        ButtonKind::DisabledHairline => plain(none, none, p.line_soft, p.ink_faint),
        ButtonKind::DisabledFilled => plain(p.surface_sunken, p.surface_sunken, none, p.ink_faint),
    }
}

/// How far a hover has faded in under `id`: over `motion-hover` going in, at
/// once going out. A keyboard selection never comes through here.
pub fn hover_fade(ctx: &egui::Context, id: egui::Id, hovered: bool) -> f32 {
    let secs = if hovered {
        theme::motion(ctx, theme::MOTION_HOVER)
    } else {
        0.0
    };
    ctx.animate_bool_with_time(id.with("hover-fade"), hovered, secs)
}

/// The ui's painter, without the fade egui puts on a disabled ui: a disabled
/// look is already what a disabled button looks like.
fn unfaded_painter(ui: &Ui) -> egui::Painter {
    let mut painter = ui.painter().clone();
    if !ui.is_enabled() {
        painter.set_opacity(painter.opacity() / ui.visuals().disabled_alpha());
    }
    painter
}

/// The laid-out content of a button: its label and shortcut galleys, and its
/// width.
struct ButtonContent {
    icon: Option<Icon>,
    text: Option<Arc<Galley>>,
    hint: Option<Arc<Galley>>,
    /// How far out the shortcut is: it only shows while ⌘ is held.
    reveal: f32,
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
    let ink = button_look(theme::palette(ui), kind).ink;
    let text = (!label.is_empty()).then(|| galley(ui, label, theme::ui_button(), ink));
    // An icon button keeps its key in its tooltip.
    let reveal = match (shortcut, &text) {
        (Some(_), Some(_)) => super::key_hints(ui.ctx()),
        _ => 0.0,
    };
    let hint = shortcut
        .filter(|_| reveal > 0.0)
        .map(|s| galley(ui, s, theme::meta_mono(), ink));
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
            w += reveal * (BUTTON_GAP + hint.size().x);
        }
        w
    };
    ButtonContent {
        icon,
        text,
        hint,
        reveal,
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
    let look = button_look(theme::palette(ui), kind);
    let fade = hover_fade(ui.ctx(), response.id, response.hovered());
    let ink = look.ink.lerp_to_gamma(look.hover_ink, fade);
    let painter = unfaded_painter(ui);
    painter.rect(
        rect,
        radius,
        look.fill.lerp_to_gamma(look.hover_fill, fade),
        Stroke::new(1.0, look.edge),
        StrokeKind::Inside,
    );

    let ButtonContent {
        icon,
        text,
        hint,
        reveal,
        ..
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
        theme::paint_icon(&painter, icon, icon_rect, ink);
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
        // Sliding out, the key comes from behind the right padding.
        let shown = rect.with_max_x(rect.max.x - theme::SPACE_3);
        painter
            .with_clip_rect(shown.intersect(painter.clip_rect()))
            .galley_with_override_text_color(
                Pos2::new(x, center_y - size.y / 2.0),
                hint,
                look.key.gamma_multiply(reveal),
            );
    }
}

/// How wide a [`button`] with no icon and no shortcut draws `label`.
pub fn text_button_width(ui: &Ui, kind: ButtonKind, label: &str) -> f32 {
    button_content(ui, kind, None, label, None).width
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
    button_at_least(ui, kind, icon, label, shortcut, 0.0)
}

/// A [`button`] as wide as the room there is, its content at its left: the
/// ProjectMenu's Stats row.
pub fn wide_button(
    ui: &mut Ui,
    kind: ButtonKind,
    icon: Option<Icon>,
    label: &str,
    shortcut: Option<&str>,
) -> Response {
    let width = ui.available_width();
    button_at_least(ui, kind, icon, label, shortcut, width)
}

fn button_at_least(
    ui: &mut Ui,
    kind: ButtonKind,
    icon: Option<Icon>,
    label: &str,
    shortcut: Option<&str>,
    min_width: f32,
) -> Response {
    let kind = kind.when(ui.is_enabled());
    let content = button_content(ui, kind, icon, label, shortcut);
    let width = content.width.max(min_width);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
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

/// A [`button`] with no icon that puts `action` in `picked` when clicked.
pub fn action_button<A>(
    ui: &mut Ui,
    picked: &mut Option<A>,
    kind: ButtonKind,
    label: &str,
    shortcut: Option<&str>,
    action: A,
) {
    if button(ui, kind, None, label, shortcut).clicked() {
        *picked = Some(action);
    }
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
    let main_kind = kind.when(main_enabled);
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
    main.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, main_enabled, label));
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
    let caret_content = button_content(ui, kind, Some(Icon::ChevronDown), "", None);
    paint_button(
        ui,
        caret_rect,
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
    ui.painter().vline(
        caret_rect.min.x,
        rect.shrink2(Vec2::new(0.0, theme::SPACE_2)).y_range(),
        Stroke::new(1.0, theme::palette(ui).line_soft),
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
    /// A question a build waits on: `status-doing` on its own pale tint,
    /// rest and hover, so the card's pin reads before the pointer reaches it.
    Doing,
    /// The trigger of an open popup (Add label): `accent` icon on
    /// `accent-soft`, hovered or not.
    Open,
    /// 20×20, as Todo's `+`.
    Ghost,
}

/// The size of a [`Tone::Ghost`] icon button.
const GHOST_ACTION: f32 = 20.0;

/// An icon-only 22×22 card action with a caller-chosen id, so tests can find
/// it: never an edge, `surface-hover` fading in on its own hover. A `Danger`
/// action (delete) turns `danger` only then. Disabled, it is `ink-faint`
/// whatever its tone.
pub fn icon_button(ui: &mut Ui, icon: Icon, id: egui::Id, hover: &str, tone: Tone) -> Response {
    let size = if tone == Tone::Ghost {
        GHOST_ACTION
    } else {
        CARD_ACTION
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hover);
    // The tooltip ends with its key ("Delete  Del"); the name is what comes first.
    let name = hover.split("  ").next().unwrap_or(hover);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), name));
    if ui.is_rect_visible(rect) {
        let p = theme::palette(ui);
        let fade = if matches!(tone, Tone::Open | Tone::Doing) {
            1.0
        } else {
            hover_fade(ui.ctx(), response.id, response.hovered())
        };
        let (fill, rest, lit) = match tone {
            Tone::Review => (p.review_soft, p.review, p.review),
            Tone::Doing => (
                p.status_doing.gamma_multiply(0.16),
                p.status_doing,
                p.status_doing,
            ),
            Tone::Open => (p.accent_soft, p.accent, p.accent),
            Tone::Danger => (p.surface_hover, p.ink_muted, p.danger),
            Tone::Plain | Tone::Ghost => (p.surface_hover, p.ink_muted, p.ink),
        };
        let ink = if ui.is_enabled() {
            rest.lerp_to_gamma(lit, fade)
        } else {
            p.ink_faint
        };
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            fill.gamma_multiply(fade),
        );
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(theme::ICON_SIZE));
        let painter = unfaded_painter(ui);
        if size == CARD_ACTION {
            theme::paint_icon_stroked(&painter, icon, icon_rect, ink, theme::CARD_ACTION_STROKE);
        } else {
            theme::paint_icon(&painter, icon, icon_rect, ink);
        }
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Event;
    use egui::epaint::Shape;

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        theme::apply(&ctx, crate::general::Appearance::Light);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        ctx
    }

    /// One pass at `time` with the pointer at `pointer`; returns the shapes.
    fn pass(
        ctx: &egui::Context,
        time: f64,
        pointer: Pos2,
        body: impl FnMut(&mut Ui),
    ) -> Vec<Shape> {
        let input = egui::RawInput {
            time: Some(time),
            predicted_dt: 0.02,
            events: vec![Event::PointerMoved(pointer)],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, body);
        output.textures_delta.clear();
        output.shapes.into_iter().map(|s| s.shape).collect()
    }

    fn box_of(shapes: &[Shape]) -> (Color32, Color32) {
        shapes
            .iter()
            .find_map(|shape| match shape {
                Shape::Rect(rect) => Some((rect.fill, rect.stroke.color)),
                _ => None,
            })
            .expect("a button paints a box")
    }

    #[test]
    fn where_a_button_sits_decides_its_edge() {
        let p = &theme::LIGHT;
        let none = Color32::TRANSPARENT;
        for (kind, edge) in [
            (ButtonKind::Header, none),
            (ButtonKind::Back, none),
            (ButtonKind::Ghost, none),
            (ButtonKind::Review, none),
            (ButtonKind::Delete, none),
            (ButtonKind::Secondary, p.line_soft),
            (ButtonKind::ReviewHairline, p.line_soft),
            (ButtonKind::Primary, none),
        ] {
            assert_eq!(button_look(p, kind).edge, edge, "{kind:?}");
        }
    }

    #[test]
    fn the_primary_is_ink_and_only_delete_under_the_pointer_is_red() {
        let p = &theme::LIGHT;
        let primary = button_look(p, ButtonKind::Primary);
        assert_eq!(
            (primary.fill, primary.hover_fill, primary.ink),
            (p.primary, p.primary_hover, p.on_primary)
        );
        assert_eq!(primary.key, p.on_primary.gamma_multiply(0.72));
        let delete = button_look(p, ButtonKind::Delete);
        assert_eq!(
            (delete.ink, delete.fill),
            (p.ink_muted, Color32::TRANSPARENT)
        );
        assert_eq!(
            (delete.hover_ink, delete.hover_fill),
            (p.danger, p.danger_soft)
        );
    }

    #[test]
    fn a_disabled_button_keeps_its_shape_and_drops_to_ink_faint() {
        let p = &theme::LIGHT;
        let none = Color32::TRANSPARENT;
        for (kind, fill, edge) in [
            (ButtonKind::Primary, p.surface_sunken, none),
            (ButtonKind::Send, p.surface_sunken, none),
            (ButtonKind::Secondary, none, p.line_soft),
            (ButtonKind::ReviewHairline, none, p.line_soft),
            (ButtonKind::Header, none, none),
            (ButtonKind::Review, none, none),
        ] {
            let look = button_look(p, kind.when(false));
            assert_eq!((look.fill, look.edge, look.ink), (fill, edge, p.ink_faint));
            assert_eq!(look.hover_fill, fill, "{kind:?} has no hover");
            assert_eq!(kind.when(true), kind);
        }
    }

    #[test]
    fn a_disabled_ui_draws_the_disabled_look_at_full_strength() {
        let ctx = context();
        let shapes = pass(&ctx, 0.0, Pos2::new(900.0, 900.0), |ui| {
            ui.add_enabled_ui(false, |ui| {
                button(ui, ButtonKind::Primary, None, "New issue", None);
            });
        });
        let p = &theme::LIGHT;
        assert_eq!(box_of(&shapes), (p.surface_sunken, Color32::TRANSPARENT));
        let ink = shapes.iter().find_map(|shape| match shape {
            Shape::Text(text) => text.override_text_color,
            _ => None,
        });
        assert_eq!(ink, Some(p.ink_faint));
    }

    /// The fill of a secondary button at the window's corner, in one pass.
    fn button_fill(ctx: &egui::Context, time: f64, pointer: Pos2) -> Color32 {
        let shapes = pass(ctx, time, pointer, |ui| {
            button(ui, ButtonKind::Secondary, None, "Close", None);
        });
        box_of(&shapes).0
    }

    #[test]
    fn hover_fades_in_over_motion_hover_and_out_at_once() {
        let ctx = context();
        let away = Pos2::new(900.0, 900.0);
        let mut over = Pos2::ZERO;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            over = button(ui, ButtonKind::Secondary, None, "Close", None)
                .rect
                .center();
        })
        .textures_delta
        .clear();
        let p = &theme::LIGHT;
        assert_eq!(button_fill(&ctx, 0.0, away), Color32::TRANSPARENT);
        button_fill(&ctx, 0.02, over);
        let rising = button_fill(&ctx, 0.04, over);
        assert_ne!(rising, p.surface_hover, "still fading in");
        assert_ne!(rising, Color32::TRANSPARENT, "already fading in");
        for step in 0..8 {
            button_fill(&ctx, 0.06 + f64::from(step) * 0.02, over);
        }
        assert_eq!(button_fill(&ctx, 0.3, over), p.surface_hover);
        button_fill(&ctx, 0.32, away);
        assert_eq!(
            button_fill(&ctx, 0.34, away),
            Color32::TRANSPARENT,
            "out at once"
        );
    }

    #[test]
    fn reduced_motion_lands_a_hover_in_one_frame() {
        let ctx = context();
        theme::set_reduced_motion(&ctx, true);
        let fade = |hovered| {
            let mut fade = 0.0;
            pass(&ctx, 0.0, Pos2::ZERO, |ui| {
                fade = hover_fade(ui.ctx(), egui::Id::unique("row"), hovered);
            });
            fade
        };
        assert_eq!(fade(false), 0.0);
        assert_eq!(fade(true), 1.0);
    }

    #[test]
    fn a_card_action_never_draws_an_edge() {
        let ctx = context();
        let id = egui::Id::unique("action");
        let mut over = Pos2::ZERO;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            over = icon_button(ui, Icon::Trash, id, "Delete", Tone::Danger)
                .rect
                .center();
        })
        .textures_delta
        .clear();
        let mut last = Vec::new();
        for frame in 0..10 {
            last = pass(&ctx, f64::from(frame) * 0.05, over, |ui| {
                icon_button(ui, Icon::Trash, id, "Delete", Tone::Danger);
            });
        }
        assert_eq!(box_of(&last).0, theme::LIGHT.surface_hover);
        let edged = last.iter().any(|shape| {
            matches!(shape, Shape::Rect(rect) if rect.stroke.width > 0.0 && rect.stroke.color.a() > 0)
        });
        assert!(!edged, "no edge, even on its own hover");
    }
}
