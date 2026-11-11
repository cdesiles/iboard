//! The Thinking mark: the iboard "i" walking a square while an agent works
//! (see `design/components/Thinking`).

use egui::epaint::CornerRadius;
use egui::{Color32, Painter, Pos2, Rect, Theme, Vec2};

use super::{DARK, LIGHT, reduced_motion};

/// One quarter of the walk; four make the full square.
const QUARTER: f64 = 1.6;
const GRID: f32 = 44.0;
const THICK: f32 = 12.0;
const LONG: f32 = 27.0;
const RADIUS: f32 = 3.0;

/// Paints the mark in the square centred in `rect`: the stem in `stem`, the
/// dot in `card`. At `time` (seconds) the blocks are somewhere in their walk;
/// without one the i stands still.
pub fn thinking_mark(
    painter: &Painter,
    rect: Rect,
    time: Option<f64>,
    stem: Color32,
    card: Color32,
) {
    let (quarter, phase) = time.map_or((0, 0.0), |time| {
        let quarters = time / QUARTER;
        (quarters.floor() as i64 % 4, quarters.fract() as f32)
    });
    let (stem_rect, card_rect) = pose(phase);
    let scale = rect.width().min(rect.height()) / GRID;
    let origin = rect.center() - Vec2::splat(GRID / 2.0 * scale);
    let place = |block: Rect| {
        let turned = Rect::from_two_pos(turn(block.min, quarter), turn(block.max, quarter));
        Rect::from_min_max(
            origin + turned.min.to_vec2() * scale,
            origin + turned.max.to_vec2() * scale,
        )
    };
    let radius = CornerRadius::same((RADIUS * scale).round() as u8);
    painter.rect_filled(place(stem_rect), radius, stem);
    painter.rect_filled(place(card_rect), radius, card);
}

/// The stem and the dot on the 44 grid at `phase` of a quarter, before it
/// turns: the i standing at 0, the next i lying along the top at 1.
fn pose(phase: f32) -> (Rect, Rect) {
    let lead = THICK + (GRID - THICK) * ease(phase / 0.5);
    let hand_over = ease((phase - 0.64) / 0.36);
    let card_width = if phase < 0.64 {
        lead.min(LONG)
    } else {
        LONG + (THICK - LONG) * hand_over
    };
    let card_right = if phase < 0.64 { lead } else { GRID };
    let card = Rect::from_min_max(
        Pos2::new(card_right - card_width, 0.0),
        Pos2::new(card_right, THICK),
    );

    let tuck = ease((phase - 0.38) / 0.32);
    let stem_top = (GRID - LONG) * (1.0 - tuck);
    let stem_height = LONG + (THICK - LONG) * tuck;
    let stem_width = THICK + (LONG - THICK) * hand_over;
    let stem = Rect::from_min_size(Pos2::new(0.0, stem_top), Vec2::new(stem_width, stem_height));
    (stem, card)
}

/// `point` on the 44 grid turned `quarter` quarter turns clockwise about its
/// centre.
fn turn(point: Pos2, quarter: i64) -> Pos2 {
    (0..quarter).fold(point, |p, _| Pos2::new(GRID - p.y, p.x))
}

/// CSS's `cubic-bezier(.65,0,.35,1)` at `x`, clamped to 0..1.
fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let bezier = |a: f32, b: f32, t: f32| {
        3.0 * a * t * (1.0 - t).powi(2) + 3.0 * b * t * t * (1.0 - t) + t.powi(3)
    };
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let mid = (low + high) / 2.0;
        if bezier(0.65, 0.35, mid) < x {
            low = mid;
        } else {
            high = mid;
        }
    }
    bezier(0.0, 1.0, (low + high) / 2.0)
}

/// The mark as a 14pt icon, walking unless motion is reduced.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_ends_on_the_next_i() {
        let (stem, card) = pose(0.0);
        let (end_stem, end_card) = pose(1.0);
        let turned = |r: Rect| Rect::from_two_pos(turn(r.min, 1), turn(r.max, 1));
        assert!((turned(stem).min - end_stem.min).length() < 0.01);
        assert!((turned(stem).max - end_stem.max).length() < 0.01);
        assert!((turned(card).min - end_card.min).length() < 0.01);
        assert!((turned(card).max - end_card.max).length() < 0.01);
    }

    #[test]
    fn the_blocks_never_touch() {
        for step in 0..=100 {
            let (stem, card) = pose(step as f32 / 100.0);
            assert!(!stem.intersects(card), "they touch at {step}%");
        }
    }
}
