//! The line icons of the design system (see `design/components/Icons`).

use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, Vec2};

use super::thinking;

/// The line icons of the design system, drawn on a 14pt box with round caps
/// and joins so they need no icon font.
#[derive(Clone, Copy)]
pub enum Icon {
    Plus,
    Minus,
    Refresh,
    Edit,
    /// The back chevron, pointing left.
    Back,
    /// A waste basket: delete an issue for good.
    Trash,
    /// A lidded box: move a done issue to `.archive`.
    Archive,
    /// A small down chevron, marking a control that opens a menu.
    ChevronDown,
    /// A speech bubble with a question mark: ask an agent to review.
    Review,
    /// A cross: close a dialog.
    Close,
    /// A tick: the chosen option of a select menu.
    Check,
    /// A magnifier: the board filter.
    Search,
    /// Two arrows pointing outwards: expand.
    Expand,
    /// A tag with a small plus at the lower right: add a label.
    TagPlus,
    /// A node on a line: the git commits behind an issue.
    Commit,
    /// A right-pointing triangle: start the AI build.
    Play,
    /// A rounded square: stop the AI build.
    Stop,
    /// A prompt in a rounded square: the build agent's log.
    Log,
    /// Two people: how many build agents may work at once.
    Agents,
    /// The iboard "i" rocking its card: an agent is at work.
    Thinking,
    /// A triangle with an exclamation mark: a signal is on.
    Warning,
    /// A flame: a hot spot, complex and often changed.
    Flame,
    /// Two stacked sheets: project groups.
    Layers,
    /// Six dots: something that can be dragged into place.
    Grip,
    /// Three bars: the Stats screen.
    Bars,
    /// Three dots in a row: a menu of what is left.
    More,
    /// Three narrowing lines: refine.
    Narrow,
    /// A speech bubble: ask an agent.
    Bubble,
    /// A bell: what needs the author's attention.
    Bell,
    /// A flag on a pole: cut a release.
    Release,
}

pub const ICON_SIZE: f32 = 14.0;
/// The stroke in buttons and drop zones.
const ICON_STROKE: f32 = 1.6;
/// The stroke in the 22×22 card actions.
pub const CARD_ACTION_STROKE: f32 = 1.5;

pub fn paint_icon(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    paint_icon_stroked(painter, icon, rect, color, ICON_STROKE);
}

/// [`paint_icon`] with a pen `width` wide.
pub fn paint_icon_stroked(painter: &Painter, icon: Icon, rect: Rect, color: Color32, width: f32) {
    let grid = Grid {
        painter,
        rect,
        scale: rect.width() / 24.0,
        stroke: Stroke::new(width, color),
    };
    match icon {
        Icon::Plus
        | Icon::Minus
        | Icon::Back
        | Icon::Refresh
        | Icon::Close
        | Icon::Check
        | Icon::Search
        | Icon::ChevronDown
        | Icon::Expand
        | Icon::Bell => basic_icon(&grid, icon),
        Icon::Edit | Icon::Trash | Icon::Archive | Icon::TagPlus | Icon::Commit => {
            document_icon(&grid, icon);
        }
        Icon::Play | Icon::Stop | Icon::Thinking | Icon::Log | Icon::Agents | Icon::Review => {
            agent_icon(&grid, icon);
        }
        Icon::Layers | Icon::Grip | Icon::Warning | Icon::Flame | Icon::Bars | Icon::Release => {
            project_icon(&grid, icon);
        }
        Icon::More | Icon::Narrow | Icon::Bubble => {
            menu_icon(&grid, icon);
        }
    }
}

/// Lucide's 24-unit grid laid over an icon's rect, and the pen the icons
/// draw with.
#[derive(Clone, Copy)]
struct Grid<'a> {
    painter: &'a Painter,
    rect: Rect,
    scale: f32,
    stroke: Stroke,
}

impl Grid<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(
            self.rect.min.x + x * self.scale,
            self.rect.min.y + y * self.scale,
        )
    }

    fn points(&self, points: &[(f32, f32)]) -> Vec<Pos2> {
        points.iter().map(|&(x, y)| self.at(x, y)).collect()
    }

    /// epaint's lines end square and meet mitred: a dot of the pen's width on
    /// each point rounds them. A see-through pen would show the overlap.
    fn round(&self, points: &[Pos2]) {
        if !self.stroke.color.is_opaque() || self.painter.opacity() < 1.0 {
            return;
        }
        for point in points {
            self.painter
                .circle_filled(*point, self.stroke.width / 2.0, self.stroke.color);
        }
    }

    fn line(&self, points: &[(f32, f32)]) {
        let points = self.points(points);
        self.round(&points);
        self.painter.line(points, self.stroke);
    }

    fn closed(&self, points: &[(f32, f32)]) {
        let points = self.points(points);
        self.round(&points);
        self.painter
            .add(egui::Shape::closed_line(points, self.stroke));
    }

    fn segment(&self, (x0, y0): (f32, f32), (x1, y1): (f32, f32)) {
        let points = [self.at(x0, y0), self.at(x1, y1)];
        self.round(&points);
        self.painter.line_segment(points, self.stroke);
    }

    fn circle(&self, (x, y): (f32, f32), radius: f32) {
        let center = self.at(x, y);
        self.painter
            .circle_stroke(center, radius * self.scale, self.stroke);
    }

    fn dot(&self, (x, y): (f32, f32), radius: f32) {
        let center = self.at(x, y);
        self.painter
            .circle_filled(center, radius * self.scale, self.stroke.color);
    }

    fn square(&self, (x0, y0): (f32, f32), (x1, y1): (f32, f32)) {
        let square = Rect::from_min_max(self.at(x0, y0), self.at(x1, y1));
        let radius = CornerRadius::same((2.0 * self.scale).round() as u8);
        self.painter
            .rect_stroke(square, radius, self.stroke, egui::StrokeKind::Middle);
    }
}

/// A cubic Bézier from `from` to `to` through `control1` and `control2`, as
/// `steps` segments.
fn curve(
    (x0, y0): (f32, f32),
    (x1, y1): (f32, f32),
    (x2, y2): (f32, f32),
    (x3, y3): (f32, f32),
    steps: usize,
) -> Vec<(f32, f32)> {
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let s = 1.0 - t;
            (
                s * s * s * x0 + 3.0 * s * s * t * x1 + 3.0 * s * t * t * x2 + t * t * t * x3,
                s * s * s * y0 + 3.0 * s * s * t * y1 + 3.0 * s * t * t * y2 + t * t * t * y3,
            )
        })
        .collect()
}

/// An arc of `radius` around `center` from `from` to `to` degrees, both ends
/// included. The degrees turn clockwise on screen, as SVG's do with y down.
fn arc((cx, cy): (f32, f32), radius: f32, from: f32, to: f32, steps: usize) -> Vec<(f32, f32)> {
    (0..=steps)
        .map(|i| {
            let turn = (from + (to - from) * i as f32 / steps as f32).to_radians();
            (cx + radius * turn.cos(), cy + radius * turn.sin())
        })
        .collect()
}

/// The plain controls: add, remove, go back, refresh, close, pick, search,
/// open a menu, expand.
fn basic_icon(g: &Grid, icon: Icon) {
    match icon {
        Icon::Plus => {
            g.segment((12.0, 5.0), (12.0, 19.0));
            g.segment((5.0, 12.0), (19.0, 12.0));
        }
        Icon::Minus => g.segment((5.0, 12.0), (19.0, 12.0)),
        Icon::Back => g.line(&[(15.0, 18.0), (9.0, 12.0), (15.0, 6.0)]),
        Icon::Refresh => {
            // A clockwise 300° arc with an arrow head where it ends, top right.
            let center = g.at(12.0, 12.0);
            let radius = 8.0 * g.scale;
            let end_angle = 300.0_f32.to_radians();
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = (i as f32 / 24.0) * end_angle;
                    center + radius * Vec2::new(t.cos(), t.sin())
                })
                .collect();
            let end = *points.last().unwrap_or(&center);
            g.round(&[points[0], end]);
            g.painter.line(points, g.stroke);
            let direction = Vec2::new(-end_angle.sin(), end_angle.cos());
            let head = 5.0 * g.scale;
            let barb = |angle: f32| {
                end - head * (egui::emath::Rot2::from_angle(angle.to_radians()) * direction)
            };
            g.round(&[barb(40.0), barb(-40.0)]);
            g.painter.line(vec![barb(40.0), end, barb(-40.0)], g.stroke);
        }
        Icon::Close => {
            g.segment((18.0, 6.0), (6.0, 18.0));
            g.segment((6.0, 6.0), (18.0, 18.0));
        }
        Icon::Check => g.line(&[(20.0, 6.0), (9.0, 17.0), (4.0, 12.0)]),
        Icon::Search => {
            g.circle((11.0, 11.0), 7.0);
            g.segment((20.0, 20.0), (16.0, 16.0));
        }
        Icon::ChevronDown => g.line(&[(6.0, 9.5), (12.0, 15.0), (18.0, 9.5)]),
        Icon::Expand => {
            // Lucide's maximize-2: two corner brackets and a diagonal each.
            g.line(&[(15.0, 3.0), (21.0, 3.0), (21.0, 9.0)]);
            g.line(&[(9.0, 21.0), (3.0, 21.0), (3.0, 15.0)]);
            g.segment((21.0, 3.0), (14.0, 10.0));
            g.segment((3.0, 21.0), (10.0, 14.0));
        }
        Icon::Bell => {
            // Lucide's bell: a dome that flares into a foot rounded at both
            // ends, and the clapper hanging under it.
            let mut body = arc((4.0, 16.0), 1.0, 222.4, 90.0, 6);
            body.extend(arc((20.0, 16.0), 1.0, 90.0, -42.3, 6));
            body.extend(curve(
                (20.74, 15.327),
                (19.41, 13.956),
                (18.0, 12.499),
                (18.0, 8.0),
                12,
            ));
            body.extend(arc((12.0, 8.0), 6.0, 0.0, -180.0, 24));
            body.extend(curve(
                (6.0, 8.0),
                (6.0, 12.499),
                (4.589, 13.956),
                (3.262, 15.326),
                12,
            ));
            g.line(&body);
            g.line(&arc((12.0, 20.0), 2.0, 150.0, 30.0, 12));
        }
        _ => {}
    }
}

/// What can be done to an issue: edit, delete, archive, label it, see its
/// commits.
fn document_icon(g: &Grid, icon: Icon) {
    match icon {
        Icon::Edit => g.line(&[
            (17.0, 3.0),
            (21.0, 7.0),
            (7.5, 20.5),
            (2.0, 22.0),
            (3.5, 16.5),
            (17.0, 3.0),
        ]),
        Icon::Trash => trash_icon(g),
        Icon::Archive => {
            // A lidded box; the handle becomes a short line.
            g.line(&[(2.0, 3.0), (22.0, 3.0), (22.0, 8.0), (2.0, 8.0), (2.0, 3.0)]);
            g.line(&[(4.0, 8.0), (4.0, 21.0), (20.0, 21.0), (20.0, 8.0)]);
            g.segment((10.0, 12.0), (14.0, 12.0));
        }
        Icon::TagPlus => {
            // Lucide's tag, shrunk to leave the lower right to a small plus.
            g.line(&[
                (2.5, 2.5),
                (11.0, 2.5),
                (19.0, 10.5),
                (10.5, 19.0),
                (2.5, 11.0),
                (2.5, 2.5),
            ]);
            g.dot((7.0, 7.0), 1.3);
            g.segment((19.0, 15.0), (19.0, 23.0));
            g.segment((15.0, 19.0), (23.0, 19.0));
        }
        Icon::Commit => {
            // Lucide's git-commit: a node on a vertical line.
            g.segment((12.0, 2.5), (12.0, 8.5));
            g.segment((12.0, 15.5), (12.0, 21.5));
            g.circle((12.0, 12.0), 3.5);
        }
        _ => {}
    }
}

/// The ribs' share of the bin's stroke: at full weight, 2.3pt apart at 14pt,
/// they clog.
const TRASH_RIB_WEIGHT: f32 = 0.625;

/// iboard's soft bin: a flat-topped knob on the lid, a lightly tapered body,
/// two long ribs.
fn trash_icon(g: &Grid) {
    g.segment((4.5, 7.0), (19.5, 7.0));
    let mut knob = vec![(9.4, 7.0)];
    for (center, from) in [(10.3, 1.0), (13.7, 1.5)] {
        for i in 0..=4 {
            let turn = std::f32::consts::FRAC_PI_2 * (from + i as f32 / 4.0);
            knob.push((center + 0.9 * turn.cos(), 4.9 + 0.9 * turn.sin()));
        }
    }
    knob.push((14.6, 7.0));
    g.line(&knob);
    g.line(&[(6.25, 7.0), (6.9, 21.0), (17.1, 21.0), (17.75, 7.0)]);
    let ribs = Grid {
        stroke: Stroke::new(g.stroke.width * TRASH_RIB_WEIGHT, g.stroke.color),
        ..*g
    };
    ribs.segment((10.0, 9.6), (10.0, 17.9));
    ribs.segment((14.0, 9.6), (14.0, 17.9));
}

/// An agent's work: start and stop the build, watch it think, read its log,
/// ask it for a review.
fn agent_icon(g: &Grid, icon: Icon) {
    match icon {
        // Lucide's play.
        Icon::Play => g.closed(&[(6.0, 3.0), (20.0, 12.0), (6.0, 21.0)]),
        Icon::Stop => g.square((5.0, 5.0), (19.0, 19.0)),
        Icon::Thinking => thinking::icon(g.painter, g.rect),
        Icon::Agents => {
            // Lucide's users: one head and shoulders, another head and a
            // shoulder behind them.
            g.circle((9.0, 7.0), 4.0);
            g.line(&[
                (16.0, 21.0),
                (16.0, 19.0),
                (15.2, 17.3),
                (13.5, 16.5),
                (6.5, 16.5),
                (4.8, 17.3),
                (4.0, 19.0),
                (4.0, 21.0),
            ]);
            g.circle((17.0, 7.5), 3.2);
            g.line(&[(20.0, 21.0), (20.0, 19.0), (19.3, 17.4), (18.0, 16.7)]);
        }
        Icon::Log => {
            // Lucide's square-terminal.
            g.square((3.0, 3.0), (21.0, 21.0));
            g.line(&[(7.0, 8.0), (10.0, 11.0), (7.0, 14.0)]);
            g.segment((12.0, 15.0), (17.0, 15.0));
        }
        Icon::Review => {
            // A speech bubble (Lucide's message-circle-question): a circle
            // with a tail at the lower left, and a question mark inside.
            let center = g.at(11.5, 11.0);
            let radius = 7.6 * g.scale;
            let bubble: Vec<Pos2> = (0..=40)
                .map(|i| {
                    let turn = std::f32::consts::TAU * (i as f32 / 40.0);
                    center + radius * Vec2::new(turn.cos(), turn.sin())
                })
                .collect();
            g.painter.line(bubble, g.stroke);
            g.line(&[(5.4, 16.6), (3.6, 20.6), (8.4, 19.2)]);
            g.line(&[
                (9.6, 9.4),
                (10.2, 8.2),
                (11.8, 7.8),
                (13.4, 8.6),
                (13.5, 10.2),
                (11.8, 11.3),
                (11.7, 12.6),
            ]);
            g.dot((11.7, 15.0), 1.0);
        }
        _ => {}
    }
}

/// The rows of the header's menus.
fn menu_icon(g: &Grid, icon: Icon) {
    match icon {
        Icon::More => {
            for x in [5.0, 12.0, 19.0] {
                g.painter
                    .circle_filled(g.at(x, 12.0), 1.5 * g.scale, g.stroke.color);
            }
        }
        Icon::Narrow => {
            g.line(&[(4.0, 6.0), (20.0, 6.0)]);
            g.line(&[(7.0, 12.0), (17.0, 12.0)]);
            g.line(&[(10.0, 18.0), (14.0, 18.0)]);
        }
        Icon::Bubble => {
            g.closed(&[
                (4.0, 5.0),
                (20.0, 5.0),
                (20.0, 17.0),
                (9.0, 17.0),
                (4.0, 21.0),
            ]);
        }
        _ => {}
    }
}

/// The project's marks: the ProjectMenu footer's Stats, groups and Reorder
/// grip, the Stats screen's signals and hot spots, and Done's release flag.
fn project_icon(g: &Grid, icon: Icon) {
    match icon {
        Icon::Release => {
            g.line(&[(5.0, 22.0), (5.0, 3.0)]);
            g.line(&[
                (5.0, 3.0),
                (19.0, 3.0),
                (15.0, 8.0),
                (19.0, 13.0),
                (5.0, 13.0),
                (5.0, 3.0),
            ]);
        }
        Icon::Bars => {
            g.line(&[(5.0, 20.0), (5.0, 12.0)]);
            g.line(&[(12.0, 20.0), (12.0, 4.0)]);
            g.line(&[(19.0, 20.0), (19.0, 15.0)]);
        }
        Icon::Layers => {
            g.closed(&[(12.0, 3.0), (21.0, 8.0), (12.0, 13.0), (3.0, 8.0)]);
            g.line(&[(3.0, 13.0), (12.0, 18.0), (21.0, 13.0)]);
        }
        Icon::Grip => {
            let dots = [
                (9.0, 6.0),
                (15.0, 6.0),
                (9.0, 12.0),
                (15.0, 12.0),
                (9.0, 18.0),
                (15.0, 18.0),
            ];
            for dot in dots {
                g.dot(dot, 1.6);
            }
        }
        Icon::Warning => {
            g.closed(&[(12.0, 3.5), (21.5, 20.0), (2.5, 20.0)]);
            g.segment((12.0, 9.5), (12.0, 13.5));
            g.dot((12.0, 16.8), 0.9);
        }
        Icon::Flame => g.closed(&[
            (12.0, 2.5),
            (16.5, 8.0),
            (19.0, 13.5),
            (17.5, 18.5),
            (12.0, 21.5),
            (6.5, 18.5),
            (5.0, 13.5),
            (7.5, 9.0),
            (9.5, 12.0),
        ]),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::Shape;

    fn dots(width: f32, color: Color32) -> Vec<f32> {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(ICON_SIZE));
            paint_icon_stroked(ui.painter(), Icon::Close, rect, color, width);
        });
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Circle(circle) => Some(circle.radius),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_line_ends_round_at_the_pens_width() {
        assert_eq!(
            dots(CARD_ACTION_STROKE, Color32::WHITE),
            [0.75; 4],
            "both ends of the cross's two strokes"
        );
        assert!(
            dots(ICON_STROKE, Color32::WHITE.gamma_multiply(0.5)).is_empty(),
            "a see-through pen would show the overlap"
        );
    }
}
