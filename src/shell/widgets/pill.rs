//! Pills and chips: status and label pills, filter tokens, chips and tags.

use egui::{
    Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use super::{galley, hover_fade};
use crate::domains::preferences::labels::LabelStyle;
use crate::shell::theme::{self, Icon};

/// An 18pt pill with a dot and a word, outlined in its state color.
pub fn pill(ui: &mut Ui, word: &str, color: Color32) -> Response {
    let p = theme::palette(ui);
    let text = galley(
        ui,
        word,
        FontId::new(11.0, theme::ui_button().family),
        color,
    );
    let size = Vec2::new(7.0 + 6.0 + 4.0 + text.size().x + 7.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, word));
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

/// An 18pt label pill, lowercased, in `meta`: `ink-muted` on `surface-sunken`
/// with a 6pt dot in the label's colour, no dot when it has none. One that
/// stands out is tinted with its colour instead, its text in that colour,
/// Medium, or in `ink` when too faint to read on the tint. Under the pointer,
/// `ink` on `surface-hover`.
pub fn label_pill(ui: &mut Ui, name: &str, style: LabelStyle) -> Response {
    sensing_label_pill(ui, name, style, false, Sense::click())
}

/// A [`label_pill`] that may be the active filter: then `accent` text on
/// `accent-soft`.
pub fn label_pill_lit(ui: &mut Ui, name: &str, style: LabelStyle, active: bool) -> Response {
    sensing_label_pill(ui, name, style, active, Sense::click())
}

/// A [`label_pill`] that `tab` skips, for where the mouse is its only use.
pub fn unfocusable_label_pill(ui: &mut Ui, name: &str, style: LabelStyle) -> Response {
    sensing_label_pill(ui, name, style, false, Sense::CLICK)
}

fn sensing_label_pill(
    ui: &mut Ui,
    name: &str,
    style: LabelStyle,
    active: bool,
    sense: Sense,
) -> Response {
    let p = theme::palette(ui);
    let name = name.to_lowercase();
    let color = style.color.map(|color| theme::label_color(color, p));
    let tint = color.filter(|_| style.stand_out);
    let dot = color.filter(|_| !style.stand_out);
    let font = if tint.is_some() {
        theme::ui_button()
    } else {
        theme::meta()
    };
    let lead = if dot.is_some() { 6.0 + 6.0 + 5.0 } else { 7.0 };
    let width = galley(ui, &name, font.clone(), p.ink).size().x;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(lead + width + 7.0, 18.0), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &name));
    if ui.is_rect_visible(rect) {
        let fade = hover_fade(ui.ctx(), response.id, response.hovered());
        let (fill, ink) = label_look(p, tint, active, fade);
        let text = galley(ui, &name, font, ink);
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_PILL), fill);
        if let Some(dot) = dot {
            painter.circle_filled(Pos2::new(rect.min.x + 9.0, rect.center().y), 3.0, dot);
        }
        let pos = Pos2::new(rect.min.x + lead, rect.center().y - text.size().y / 2.0);
        painter.galley(pos, text, ink);
    }
    response
}

/// A label pill's fill and text, `fade` of the way to its hovered look.
fn label_look(
    p: &theme::Palette,
    tint: Option<Color32>,
    active: bool,
    fade: f32,
) -> (Color32, Color32) {
    if active {
        return (p.accent_soft, p.accent);
    }
    let (fill, ink) = match tint {
        Some(color) => {
            let fill = theme::label_tint(color, p);
            let ink = if theme::contrast_ratio(color, fill) >= 4.5 {
                color
            } else {
                p.ink
            };
            (fill, ink)
        }
        None => (p.surface_sunken, p.ink_muted),
    };
    (
        fill.lerp_to_gamma(p.surface_hover, fade),
        ink.lerp_to_gamma(p.ink, fade),
    )
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
    let name = format!("Remove {text}");
    let response = ui
        .interact(
            remove_rect,
            ui.scope_id().with(("filter-token", text)),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(name.as_str());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &name));
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
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
    if ui.is_rect_visible(rect) {
        let lit = current || response.hovered();
        let (fill, edge, ink) = if lit {
            (p.accent_soft, p.accent, p.accent)
        } else {
            (p.surface, p.line_soft, p.ink)
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

/// An 18pt tag in `meta`: no edge, on `surface-sunken`. Clickable when the
/// caller acts on it.
pub fn tag(ui: &mut Ui, text: &str) -> Response {
    let p = theme::palette(ui);
    let fills = [p.surface_sunken, p.surface_hover];
    tag_with(ui, text, fills, Color32::TRANSPARENT, p.ink_muted)
}

/// The tag of what Review wrote: `review` on `review-soft`.
pub fn review_tag(ui: &mut Ui, text: &str) -> Response {
    let p = theme::palette(ui);
    let fills = [p.review_soft, p.review_soft];
    tag_with(ui, text, fills, Color32::TRANSPARENT, p.review)
}

/// `fills`: at rest, and under the pointer.
fn tag_with(ui: &mut Ui, text: &str, fills: [Color32; 2], edge: Color32, ink: Color32) -> Response {
    let label = galley(ui, text, theme::meta(), ink);
    let size = Vec2::new(6.0 + label.size().x + 6.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            fills[usize::from(response.hovered())],
            Stroke::new(1.0, edge),
            StrokeKind::Inside,
        );
        let pos = rect.center() - label.size() / 2.0;
        ui.painter().galley(pos, label, ink);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::preferences::labels::{LabelColor, Rgb};
    use crate::kernel::labels::Hue;

    const RED: Option<LabelColor> = Some(LabelColor::Preset(Hue::Red));

    struct Paint {
        fill: Color32,
        dot: Option<Color32>,
        ink: Color32,
        rect: Rect,
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx
    }

    /// What a frame paints for a light-theme pill, at `time`, the pointer at
    /// `pointer`.
    fn paint(
        ctx: &egui::Context,
        style: LabelStyle,
        active: bool,
        time: f64,
        pointer: Pos2,
    ) -> Paint {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
            time: Some(time),
            events: vec![egui::Event::PointerMoved(pointer)],
            ..Default::default()
        };
        let mut rect = Rect::NOTHING;
        let mut output = ctx.run_ui(raw, |ui| {
            rect = label_pill_lit(ui, "bug", style, active).rect;
        });
        output.textures_delta.clear();
        let mut painted = Paint {
            fill: Color32::TRANSPARENT,
            dot: None,
            ink: Color32::TRANSPARENT,
            rect,
        };
        for clipped in &output.shapes {
            match &clipped.shape {
                egui::Shape::Rect(shape) => {
                    assert_eq!(shape.stroke.width, 0.0, "a label pill has no edge");
                    painted.fill = shape.fill;
                }
                egui::Shape::Circle(circle) => painted.dot = Some(circle.fill),
                egui::Shape::Text(text) => painted.ink = text.fallback_color,
                _ => {}
            }
        }
        painted
    }

    const AWAY: Pos2 = Pos2::new(390.0, 190.0);

    #[test]
    fn a_label_is_neutral_with_a_dot_or_tinted_when_it_stands_out() {
        let ctx = context();
        let p = &theme::LIGHT;
        let look = |color, stand_out| {
            let style = LabelStyle { color, stand_out };
            let paint = paint(&ctx, style, false, 0.0, AWAY);
            (paint.fill, paint.dot, paint.ink)
        };
        assert_eq!(
            look(RED, false),
            (p.surface_sunken, Some(p.label(Hue::Red)), p.ink_muted)
        );
        assert_eq!(
            look(None, false),
            (p.surface_sunken, None, p.ink_muted),
            "no colour, no dot"
        );
        assert_eq!(
            look(RED, true),
            (
                theme::label_tint(p.label(Hue::Red), p),
                None,
                p.label(Hue::Red)
            )
        );
        assert_eq!(
            look(None, true),
            (p.surface_sunken, None, p.ink_muted),
            "nothing to tint with"
        );
        let faint = Some(LabelColor::Own(Rgb(0xf5, 0xf5, 0xf5)));
        assert_eq!(look(faint, true).2, p.ink, "too faint to read on its tint");
    }

    #[test]
    fn a_preset_follows_the_theme_and_an_own_colour_does_not() {
        let own = LabelColor::Own(Rgb(1, 2, 3));
        let preset = LabelColor::Preset(Hue::Red);
        assert_eq!(
            theme::label_color(own, &theme::LIGHT),
            theme::label_color(own, &theme::DARK)
        );
        assert_eq!(
            theme::label_color(preset, &theme::LIGHT),
            theme::LIGHT.label(Hue::Red)
        );
        assert_eq!(
            theme::label_color(preset, &theme::DARK),
            theme::DARK.label(Hue::Red)
        );
    }

    #[test]
    fn every_preset_reads_in_both_themes() {
        for p in [&theme::LIGHT, &theme::DARK] {
            for hue in Hue::ALL {
                let color = theme::label_color(LabelColor::Preset(hue), p);
                let name = hue.name();
                assert!(
                    theme::contrast_ratio(color, p.surface_sunken) >= 4.5,
                    "{name} dot"
                );
                assert!(
                    theme::contrast_ratio(color, theme::label_tint(color, p)) >= 4.5,
                    "{name} on its tint"
                );
            }
        }
    }

    #[test]
    fn a_label_fades_to_ink_under_the_pointer_and_is_accent_as_the_active_filter() {
        let ctx = context();
        let p = &theme::LIGHT;
        let style = LabelStyle {
            color: RED,
            stand_out: true,
        };
        let rest = paint(&ctx, style, false, 0.0, AWAY);
        assert_eq!(rest.ink, p.label(Hue::Red));
        let over = rest.rect.center();
        paint(&ctx, style, false, 0.02, over);
        let rising = paint(&ctx, style, false, 0.04, over);
        assert_ne!(rising.fill, p.surface_hover, "still fading in");
        assert_ne!(rising.fill, rest.fill, "already fading in");
        for step in 0..8 {
            paint(&ctx, style, false, 0.06 + f64::from(step) * 0.02, over);
        }
        let hovered = paint(&ctx, style, false, 0.3, over);
        assert_eq!((hovered.fill, hovered.ink), (p.surface_hover, p.ink));

        let active = paint(&ctx, style, true, 0.32, AWAY);
        assert_eq!((active.fill, active.ink), (p.accent_soft, p.accent));
    }
}
