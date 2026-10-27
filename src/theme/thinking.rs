//! The Thinking mark: the iboard "i" whose card-dot rocks while an agent
//! works (see `design/components/Thinking`).

use egui::epaint::{CornerRadius, PathShape};
use egui::{Color32, Painter, Pos2, Rect, Stroke, Theme, Vec2};

use super::{DARK, LIGHT, reduced_motion};

/// One rock of the card, there and back.
const PERIOD: f64 = 1.6;
const REST_DEG: f32 = -8.0;
const SWING_DEG: f32 = 13.0;
const LIFT: f32 = 2.5;

/// The glyph's box on the logo's 64 grid, cropped to the i.
const VIEW: Rect = Rect {
    min: Pos2::new(20.0, 4.0),
    max: Pos2::new(46.0, 55.0),
};
const STEM: Rect = Rect {
    min: Pos2::new(23.0, 27.0),
    max: Pos2::new(35.0, 54.0),
};
const CARD: Rect = Rect {
    min: Pos2::new(22.5, 9.0),
    max: Pos2::new(42.5, 22.0),
};

/// Paints the mark in `rect`: the stem in `stem`, the card in `card`. At
/// `time` (seconds) the card is somewhere in its rock; without one it rests
/// at −8°, which is simply the logo.
pub fn thinking_mark(
    painter: &Painter,
    rect: Rect,
    time: Option<f64>,
    stem: Color32,
    card: Color32,
) {
    let scale = (rect.height() / VIEW.height()).min(rect.width() / VIEW.width());
    let origin = rect.center() - VIEW.center().to_vec2() * scale;
    let at = |point: Pos2| origin + point.to_vec2() * scale;
    painter.rect_filled(
        Rect::from_min_max(at(STEM.min), at(STEM.max)),
        CornerRadius::same((3.0 * scale).round() as u8),
        stem,
    );

    let swing = time.map_or(0.0, |time| {
        let phase = (time % PERIOD / PERIOD) as f32;
        let half = 1.0 - (2.0 * phase - 1.0).abs();
        half * half * (3.0 - 2.0 * half)
    });
    let angle = (REST_DEG + SWING_DEG * swing).to_radians();
    let pivot = Pos2::new(CARD.center().x, CARD.max.y);
    let (sin, cos) = angle.sin_cos();
    let points = rounded_rect(CARD, 3.5)
        .into_iter()
        .map(|point| {
            let v = point - pivot;
            let turned = Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
            at(pivot + turned - Vec2::new(0.0, LIFT * swing))
        })
        .collect();
    painter.add(PathShape::convex_polygon(points, card, Stroke::NONE));
}

/// The mark as a 14pt icon, rocking unless motion is reduced.
pub(super) fn icon(painter: &Painter, rect: Rect) {
    let ctx = painter.ctx();
    let p = if ctx.theme() == Theme::Dark {
        &DARK
    } else {
        &LIGHT
    };
    let time = (!reduced_motion(ctx)).then(|| ctx.input(|i| i.time));
    if time.is_some() {
        ctx.request_repaint();
    }
    thinking_mark(painter, rect, time, p.ink, p.accent);
}

/// The outline of `rect` with corners of `radius`, clockwise.
pub(super) fn rounded_rect(rect: Rect, radius: f32) -> Vec<Pos2> {
    const STEPS: usize = 4;
    let corners = [
        (
            Pos2::new(rect.max.x - radius, rect.min.y + radius),
            -90.0_f32,
        ),
        (Pos2::new(rect.max.x - radius, rect.max.y - radius), 0.0),
        (Pos2::new(rect.min.x + radius, rect.max.y - radius), 90.0),
        (Pos2::new(rect.min.x + radius, rect.min.y + radius), 180.0),
    ];
    corners
        .into_iter()
        .flat_map(|(center, start)| {
            (0..=STEPS).map(move |step| {
                let angle = (start + 90.0 * step as f32 / STEPS as f32).to_radians();
                center + Vec2::new(angle.cos(), angle.sin()) * radius
            })
        })
        .collect()
}
