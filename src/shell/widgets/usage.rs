//! The usage meter: the ring, the one trigger of the status bar, and the
//! gauge with its even-pace tick. See `design/components/UsageMeter`.

use egui::{CornerRadius, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::{dashed_rect, focus_ring, galley, theme};

/// The diameter of a subscription meter's ring, in the status bar and in its
/// popover.
pub const RING: f32 = 14.0;
/// The ring's stroke, as the design sets it.
const RING_STROKE: f32 = 1.75;

/// A subscription meter's ring: `share` of it in `accent`, the rest the
/// track. `None` leaves the track dashed; it never makes a share up. `warn`
/// turns the arc `status-doing` for a plan past its even pace.
pub fn ring(ui: &mut Ui, share: Option<f32>, warn: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(RING), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_ring(ui, rect, share, warn);
    }
    response
}

/// The one usage trigger: the board's spend, then a [`ring`] and the closest
/// window's figure, its hit area a control tall and a `space-3` wider each
/// side. `spend` is the figure, whether anything was spent today, and the line
/// VoiceOver reads for it. `share` is the share of the window closest to its
/// cap, `None` dashing the ring; `figure` is `None` when no plan has a window.
/// `detail` is what VoiceOver reads of the windows, and only an `open` popover
/// fills the trigger. It has no tooltip and no hover: every figure is one
/// click away.
pub fn usage_pill(
    ui: &mut Ui,
    spend: Option<(&str, bool, &str)>,
    share: Option<f32>,
    warn: bool,
    figure: Option<&str>,
    detail: &str,
    open: bool,
) -> Response {
    let p = theme::palette(ui);
    let ink = if warn { p.status_doing } else { p.ink };
    let pad = theme::SPACE_3;
    let (spend, line) = match spend {
        Some((figure, lit, line)) => {
            let color = if lit { p.ink } else { p.ink_muted };
            (Some(galley(ui, figure, theme::meta(), color)), Some(line))
        }
        None => (None, None),
    };
    let figure = figure.map(|figure| galley(ui, figure, theme::meta(), ink));
    let mut width = 2.0 * pad;
    if let Some(spend) = &spend {
        width += spend.size().x;
    }
    if let Some(figure) = &figure {
        width += RING + theme::SPACE_2 + figure.size().x;
        if spend.is_some() {
            width += theme::SPACE_3;
        }
    }
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let name = match line {
        Some(line) if detail.is_empty() => line.to_string(),
        Some(line) => format!("{line}\n{detail}"),
        None => detail.to_string(),
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &name));
    if ui.is_rect_visible(rect) {
        if open {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
        }
        let painter = ui.painter();
        let mut x = rect.min.x + pad;
        if let Some(spend) = &spend {
            painter.galley(
                Pos2::new(x, rect.center().y - spend.size().y / 2.0),
                spend.clone(),
                p.ink,
            );
            x += spend.size().x + theme::SPACE_3;
        }
        if let Some(figure) = &figure {
            let ring = Rect::from_min_size(
                Pos2::new(x, rect.center().y - RING / 2.0),
                Vec2::splat(RING),
            );
            paint_ring(ui, ring, share, warn);
            painter.galley(
                Pos2::new(
                    ring.max.x + theme::SPACE_2,
                    rect.center().y - figure.size().y / 2.0,
                ),
                figure.clone(),
                ink,
            );
        }
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// Draws a ring's track and, when known, the share that fills it from the
/// top, clockwise. An unknown share leaves a dashed track; 0% an empty solid
/// one.
fn paint_ring(ui: &Ui, rect: Rect, share: Option<f32>, warn: bool) {
    let p = theme::palette(ui);
    let radius = rect.width() / 2.0 - RING_STROKE / 2.0;
    let center = rect.center();
    let track = Stroke::new(RING_STROKE, p.line_strong);
    let Some(share) = share else {
        let points = circle(center, radius, 64);
        ui.painter()
            .extend(egui::Shape::dashed_line(&points, track, 6.25, 6.25));
        return;
    };
    ui.painter().circle_stroke(center, radius, track);
    let share = share.clamp(0.0, 1.0);
    let color = if warn { p.status_doing } else { p.accent };
    let stroke = Stroke::new(RING_STROKE, color);
    if share >= 1.0 {
        ui.painter().circle_stroke(center, radius, stroke);
    } else if share > 0.0 {
        let start = -std::f32::consts::FRAC_PI_2;
        let steps = 32;
        let points: Vec<Pos2> = (0..=steps)
            .map(|step| {
                let angle = start + std::f32::consts::TAU * share * step as f32 / steps as f32;
                center + Vec2::angled(angle) * radius
            })
            .collect();
        ui.painter().add(egui::Shape::line(points, stroke));
    }
}

/// A point every `1 / steps` around a circle, closed, from the top clockwise.
fn circle(center: Pos2, radius: f32, steps: usize) -> Vec<Pos2> {
    let start = -std::f32::consts::FRAC_PI_2;
    (0..=steps)
        .map(|step| {
            let angle = start + std::f32::consts::TAU * step as f32 / steps as f32;
            center + Vec2::angled(angle) * radius
        })
        .collect()
}

/// The height of a usage row's gauge, and of the model split's under it.
pub const GAUGE_H: f32 = 6.0;
pub const GAUGE_SLIM_H: f32 = 3.0;

/// How far a gauge's pace tick reaches above and below it.
const GAUGE_TICK: f32 = 3.0;

/// A usage gauge: the share of a cap used on a `surface-sunken` track with a
/// `line` edge, and an optional `ink` tick at an even pace. A share past the
/// tick is `status-doing`; an unknown one is a dashed outline. `what` names
/// the window ("the week") or "the tokens", and makes the label VoiceOver
/// reads.
pub fn gauge(
    ui: &mut Ui,
    share: Option<f32>,
    tick: Option<f32>,
    height: f32,
    what: &str,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let label = gauge_label(share, tick, what);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &label));
    if ui.is_rect_visible(rect) {
        paint_gauge(ui, rect, share, tick);
    }
    response
}

/// What a gauge's label says: its share and, when it has one, the even pace.
fn gauge_label(share: Option<f32>, tick: Option<f32>, what: &str) -> String {
    let Some(share) = share else {
        return "No cap set".to_string();
    };
    let share = share.clamp(0.0, 1.0) * 100.0;
    match tick {
        Some(tick) => format!(
            "{share:.0}% of {what} used, an even pace is {:.0}%",
            tick.clamp(0.0, 1.0) * 100.0
        ),
        None => format!("{share:.0}% of {what}"),
    }
}

fn paint_gauge(ui: &Ui, rect: Rect, share: Option<f32>, tick: Option<f32>) {
    let p = theme::palette(ui);
    let radius = CornerRadius::same((rect.height() / 2.0).min(f32::from(theme::RADIUS_SM)) as u8);
    let painter = ui.painter();
    let Some(share) = share else {
        dashed_rect(painter, rect, theme::RADIUS_SM, p.line_strong);
        return;
    };
    let share = share.clamp(0.0, 1.0);
    painter.rect_filled(rect, radius, p.surface_sunken);
    let warn = tick.is_some_and(|tick| share > tick);
    if share > 0.0 {
        let filled = Rect::from_min_size(rect.min, Vec2::new(rect.width() * share, rect.height()));
        let color = if warn { p.status_doing } else { p.accent };
        painter.rect_filled(filled, radius, color);
    }
    painter.rect_stroke(rect, radius, Stroke::new(1.0, p.line), StrokeKind::Inside);
    if let Some(tick) = tick {
        let x = rect.min.x + rect.width() * tick.clamp(0.0, 1.0);
        painter.line_segment(
            [
                Pos2::new(x, rect.min.y - GAUGE_TICK),
                Pos2::new(x, rect.max.y + GAUGE_TICK),
            ],
            Stroke::new(2.0, p.ink),
        );
    }
}
