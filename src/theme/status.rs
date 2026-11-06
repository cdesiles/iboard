//! The columns' status icons: a shape each, so colour is never the only signal.

use egui::{Color32, Pos2, Rect, Shape, Stroke, Vec2};

use super::{Palette, STATUS_ICON};

/// Where an issue stands, as its column says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Todo,
    Ready,
    Doing,
    Done,
}

impl Status {
    pub fn color(self, p: &Palette) -> Color32 {
        match self {
            Self::Todo => p.status_todo,
            Self::Ready => p.status_ready,
            Self::Doing => p.status_doing,
            Self::Done => p.status_done,
        }
    }
}

/// Paints the icon of `status` on the 14pt box `rect`.
pub fn status_icon(painter: &egui::Painter, status: Status, rect: Rect, p: &Palette) {
    painter.extend(status_shapes(status, rect, p));
}

/// Todo: an empty circle. Ready: a circle of twelve dots. Doing: the empty
/// circle with its right half filled inside. Done: a filled circle with a
/// check in `surface`.
fn status_shapes(status: Status, rect: Rect, p: &Palette) -> Vec<Shape> {
    const RING: f32 = 5.25;
    const PEN: f32 = 1.5;
    let scale = rect.width() / STATUS_ICON;
    let centre = rect.center();
    let color = status.color(p);
    let at = |x: f32, y: f32| rect.min + scale * Vec2::new(x, y);
    let ring = Shape::circle_stroke(centre, RING * scale, Stroke::new(PEN * scale, color));
    match status {
        Status::Todo => vec![ring],
        Status::Ready => (0..12u8)
            .map(|dot| {
                let angle = f32::from(dot) * std::f32::consts::TAU / 12.0;
                let pos = centre + RING * scale * Vec2::angled(angle);
                Shape::circle_filled(pos, 0.875 * scale, color)
            })
            .collect(),
        Status::Doing => vec![ring, half_disc(centre, 3.25 * scale, color)],
        Status::Done => {
            let check = Stroke::new(PEN * scale, p.surface);
            let (a, b, c) = (at(4.4, 7.1), at(6.2, 8.9), at(9.6, 5.3));
            vec![
                Shape::circle_filled(centre, 6.0 * scale, color),
                Shape::line_segment([a, b], check),
                Shape::line_segment([b, c], check),
                Shape::circle_filled(a, check.width / 2.0, p.surface),
                Shape::circle_filled(b, check.width / 2.0, p.surface),
                Shape::circle_filled(c, check.width / 2.0, p.surface),
            ]
        }
    }
}

/// The right half of a disc.
fn half_disc(centre: Pos2, radius: f32, color: Color32) -> Shape {
    let points = (0..=16u8)
        .map(|step| {
            let angle = (f32::from(step) / 16.0 - 0.5) * std::f32::consts::PI;
            centre + radius * Vec2::angled(angle)
        })
        .collect();
    Shape::convex_polygon(points, color, Stroke::NONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes(status: Status) -> Vec<Shape> {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(STATUS_ICON));
        status_shapes(status, rect, &super::super::LIGHT)
    }

    fn circle(shape: &Shape) -> egui::epaint::CircleShape {
        let Shape::Circle(circle) = shape else {
            panic!("not a circle: {shape:?}");
        };
        *circle
    }

    #[test]
    fn each_status_has_its_own_shape() {
        let p = &super::super::LIGHT;
        let [todo] = &shapes(Status::Todo)[..] else {
            panic!("Todo is one ring");
        };
        assert_eq!(circle(todo).radius, 5.25);
        assert_eq!(circle(todo).stroke, Stroke::new(1.5, p.status_todo));
        assert_eq!(circle(todo).fill, Color32::TRANSPARENT);

        let ready = shapes(Status::Ready);
        assert_eq!(ready.len(), 12);
        assert!(ready.iter().all(|dot| {
            let dot = circle(dot);
            dot.radius == 0.875
                && dot.fill == p.status_ready
                && (dot.center.distance(Pos2::new(7.0, 7.0)) - 5.25).abs() < 0.01
        }));

        let doing = shapes(Status::Doing);
        assert_eq!(circle(&doing[0]).stroke.color, p.status_doing);
        let Shape::Path(half) = &doing[1] else {
            panic!("Doing fills a half disc");
        };
        assert!(half.points.iter().all(|point| point.x >= 7.0 - 0.01));
        assert!(half.points.iter().any(|point| point.x >= 7.0 + 3.24));

        let done = shapes(Status::Done);
        assert_eq!(circle(&done[0]).radius, 6.0);
        assert_eq!(circle(&done[0]).fill, p.status_done);
        assert_eq!(circle(&done[3]).fill, p.surface, "the check is in surface");
    }

    #[test]
    fn the_icon_fills_its_box_at_any_size() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(28.0));
        let done = status_shapes(Status::Done, rect, &super::super::LIGHT);
        assert_eq!(circle(&done[0]).radius, 12.0);
    }
}
