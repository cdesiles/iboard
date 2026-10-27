//! The iboard wordmark, drawn from its parts like
//! `design/components/Logo/files/iboard-wordmark-*.svg`, so it follows the theme.

use egui::epaint::PathShape;
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

use super::thinking::rounded_rect;

/// The SVG's viewBox.
const VIEW: Rect = Rect {
    min: Pos2::new(0.0, -2.0),
    max: Pos2::new(142.0, 42.0),
};
const WEIGHT: f32 = 6.0;
const RING: f32 = 9.0;
const STEMS: [(f32, f32, f32); 5] = [
    (0.0, 16.0, 40.0),
    (11.0, 3.0, 40.0),
    (87.0, 16.0, 40.0),
    (98.0, 16.0, 40.0),
    (136.0, 3.0, 40.0),
];
const RINGS: [f32; 4] = [23.0, 52.0, 81.0, 130.0];
const CARD: Rect = Rect {
    min: Pos2::new(-0.5, 3.5),
    max: Pos2::new(8.5, 10.5),
};
const CARD_PIVOT: Pos2 = Pos2::new(4.0, 7.0);

/// Width over height.
pub const WORDMARK_ASPECT: f32 = 142.0 / 44.0;

/// Paints "iboard" in `rect`: the letters in `ink`, the i's card in `card`.
pub fn wordmark(painter: &Painter, rect: Rect, ink: Color32, card: Color32) {
    let scale = (rect.height() / VIEW.height()).min(rect.width() / VIEW.width());
    let origin = rect.center() - VIEW.center().to_vec2() * scale;
    let at = |x: f32, y: f32| origin + Vec2::new(x, y) * scale;
    for (x, top, bottom) in STEMS {
        painter.rect_filled(
            Rect::from_min_max(at(x, top), at(x + WEIGHT, bottom)),
            0.0,
            ink,
        );
    }
    let stroke = Stroke::new(WEIGHT * scale, ink);
    for x in RINGS {
        painter.circle_stroke(at(x, 28.0), RING * scale, stroke);
    }
    let shoulder = (0..=8)
        .map(|step| {
            let angle = (180.0 + 90.0 * step as f32 / 8.0).to_radians();
            at(110.0 + RING * angle.cos(), 28.0 + RING * angle.sin())
        })
        .chain([at(115.0, 19.0)])
        .collect();
    painter.add(PathShape::line(shoulder, stroke));

    let (sin, cos) = (-8.0_f32).to_radians().sin_cos();
    let points = rounded_rect(CARD, 1.6)
        .into_iter()
        .map(|point| {
            let v = point - CARD_PIVOT;
            let turned = CARD_PIVOT + Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
            at(turned.x, turned.y)
        })
        .collect();
    painter.add(PathShape::convex_polygon(points, card, Stroke::NONE));
}
