//! The Trends of Project stats: the four measures over the page, a small
//! multiple each, under one shared crosshair.

use iboard::stats::{Cycles, Limits, Stats};

use super::{
    Align, Layout, Measure, Pos2, Rect, RichText, Sense, Shape, Stroke, TREND_H, TREND_LABEL_W,
    Vec2, block, block_label, theme,
};

/// The strip at a chart's right that holds its axis bounds.
const AXIS_W: f32 = 56.0;

/// A measure at each commit of the page, oldest first; cycle time is the
/// median (and p85) of the issues closed up to that commit, in days.
fn series(stats: &Stats, measure: Measure) -> (Vec<Option<f32>>, Option<Vec<Option<f32>>>) {
    if measure != Measure::Cycle {
        let values = stats
            .snapshots
            .iter()
            .map(|s| Some(measure.at(s)))
            .collect();
        return (values, None);
    }
    let mut closed = Cycles::default();
    let mut median = Vec::new();
    let mut p85 = Vec::new();
    for snapshot in &stats.snapshots {
        closed.cycles.extend(
            stats
                .cycles
                .cycles
                .iter()
                .filter(|c| c.short == snapshot.short)
                .cloned(),
        );
        let days = |seconds: i64| seconds as f32 / 86_400.0;
        median.push(closed.percentile(0.5).map(days));
        p85.push(closed.percentile(0.85).map(days));
    }
    (median, Some(p85))
}

/// The four trends on one axis, each on its own scale; returns the commit
/// under the pointer, shared by every row's crosshair.
pub(super) fn trends(
    ui: &mut egui::Ui,
    stats: &Stats,
    limits: &Limits,
    hovered: Option<usize>,
) -> Option<usize> {
    block(ui, |ui| {
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            block_label(ui, "Trends");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let text = match hovered.and_then(|at| stats.snapshots.get(at)) {
                    Some(s) => format!("{} · {}", s.short, s.day()),
                    None => format!("{} commits, oldest left", stats.snapshots.len()),
                };
                ui.label(RichText::new(text).font(theme::meta()).color(p.ink_muted));
            });
        });
        let mut now_hovered = None;
        for measure in Measure::ALL {
            let (values, extra) = series(stats, measure);
            ui.horizontal(|ui| {
                trend_label(ui, measure, &values, hovered);
                let limit = (measure == Measure::Complexity).then_some(limits.complexity);
                let chart = Chart {
                    values: &values,
                    extra: extra.as_deref(),
                    limit,
                    from_zero: measure == Measure::Code,
                    hovered,
                };
                if let Some(at) = chart.show(ui, measure) {
                    now_hovered = Some(at);
                }
            });
        }
        now_hovered
    })
}

/// A trend's name, its value at the hovered commit, and its value now.
fn trend_label(ui: &mut egui::Ui, measure: Measure, values: &[Option<f32>], at: Option<usize>) {
    let p = theme::palette(ui);
    let now = values.iter().rev().find_map(|v| *v);
    let shown = at.and_then(|at| values.get(at).copied().flatten()).or(now);
    ui.allocate_ui_with_layout(
        Vec2::new(TREND_LABEL_W, TREND_H),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_width(TREND_LABEL_W);
            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
            ui.label(
                RichText::new(measure.name())
                    .font(theme::ui_small())
                    .color(p.ink),
            );
            let value = shown.map_or("–".to_string(), |v| measure.show(v));
            ui.label(
                RichText::new(value)
                    .font(egui::FontId::new(18.0, theme::ui_title().family))
                    .color(p.ink),
            );
            if let Some(now) = now {
                ui.label(
                    RichText::new(format!("now {}", measure.show(now)))
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
            }
        },
    );
}

/// One small multiple: a line (or an area from 0), an optional dashed second
/// line and limit, and the shared crosshair.
struct Chart<'a> {
    values: &'a [Option<f32>],
    extra: Option<&'a [Option<f32>]>,
    limit: Option<f32>,
    from_zero: bool,
    hovered: Option<usize>,
}

impl Chart<'_> {
    fn show(&self, ui: &mut egui::Ui, measure: Measure) -> Option<usize> {
        let p = theme::palette(ui);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), TREND_H), Sense::hover());
        let name = format!("{} trend", measure.name());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &name));
        let painter = ui.painter_at(rect.expand(4.0));
        let plot = Rect::from_min_max(
            rect.min + Vec2::new(4.0, 6.0),
            rect.max - Vec2::new(AXIS_W, 6.0),
        );
        let known = self
            .values
            .iter()
            .chain(self.extra.into_iter().flatten())
            .flatten()
            .copied()
            .chain(self.limit);
        let (mut low, high) =
            known.fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
        if self.from_zero {
            low = 0.0;
        }
        for y in [rect.top(), rect.bottom()] {
            painter.hline(rect.x_range(), y, Stroke::new(1.0, p.line));
        }
        if low > high {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No issue closed on this page yet",
                theme::meta(),
                p.ink_faint,
            );
            return None;
        }
        let span = (high - low).max(f32::EPSILON);
        let step = plot.width() / (self.values.len().max(2) - 1) as f32;
        let point = |at: usize, v: f32| {
            Pos2::new(
                plot.left() + step * at as f32,
                plot.bottom() - (v - low) / span * plot.height(),
            )
        };
        if let Some(limit) = self.limit {
            let y = point(0, limit).y;
            let dash = Stroke::new(1.0, p.status_doing);
            painter.extend(Shape::dashed_line(
                &[Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                dash,
                4.0,
                3.0,
            ));
            painter.text(
                Pos2::new(rect.left() + 2.0, y - 2.0),
                egui::Align2::LEFT_BOTTOM,
                format!("refactor {limit:.0}"),
                theme::meta(),
                p.status_doing,
            );
        }
        if let Some(extra) = self.extra {
            for run in runs(extra, &point) {
                painter.extend(Shape::dashed_line(
                    &run,
                    Stroke::new(1.0, p.ink_faint),
                    4.0,
                    3.0,
                ));
            }
        }
        for run in runs(self.values, &point) {
            if self.from_zero && run.len() > 1 {
                let mut area = run.clone();
                area.push(Pos2::new(run[run.len() - 1].x, plot.bottom()));
                area.push(Pos2::new(run[0].x, plot.bottom()));
                let fill = p.accent.gamma_multiply(0.12);
                painter.add(egui::epaint::PathShape {
                    points: area,
                    closed: true,
                    fill,
                    stroke: egui::epaint::PathStroke::NONE,
                });
            }
            painter.add(Shape::line(run, Stroke::new(2.0, p.accent)));
        }
        painter.text(
            rect.right_top() + Vec2::new(-2.0, 2.0),
            egui::Align2::RIGHT_TOP,
            measure.show(high),
            theme::meta(),
            p.ink_muted,
        );
        painter.text(
            rect.right_bottom() + Vec2::new(-2.0, -2.0),
            egui::Align2::RIGHT_BOTTOM,
            measure.show(low),
            theme::meta(),
            p.ink_muted,
        );
        if let Some(at) = self.hovered.filter(|at| *at < self.values.len()) {
            let x = plot.left() + step * at as f32;
            painter.vline(x, rect.y_range(), Stroke::new(1.0, p.line_strong));
            if let Some(v) = self.values[at] {
                painter.circle(point(at, v), 4.0, p.accent, Stroke::new(2.0, p.surface));
            }
        }
        response.hover_pos().map(|pos| {
            ((pos.x - plot.left()) / step)
                .round()
                .clamp(0.0, (self.values.len() - 1) as f32) as usize
        })
    }
}

/// The unbroken stretches of a series, as points.
fn runs(values: &[Option<f32>], point: &impl Fn(usize, f32) -> Pos2) -> Vec<Vec<Pos2>> {
    let mut runs: Vec<Vec<Pos2>> = vec![Vec::new()];
    for (at, value) in values.iter().enumerate() {
        match value {
            Some(v) => runs
                .last_mut()
                .into_iter()
                .for_each(|run| run.push(point(at, *v))),
            None => runs.push(Vec::new()),
        }
    }
    runs.retain(|run| !run.is_empty());
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use iboard::stats::{Cycle, Snapshot};

    fn snapshot(short: &str, code: usize, complexity: usize, test_code: usize) -> Snapshot {
        Snapshot {
            short: short.into(),
            code,
            complexity,
            test_code,
            ..Snapshot::default()
        }
    }

    fn cycle(short: &str, days: i64) -> Cycle {
        Cycle {
            issue: format!("{short}.md"),
            short: short.into(),
            seconds: days * 86_400,
        }
    }

    #[test]
    fn cycle_time_follows_the_issues_closed_so_far() {
        let stats = Stats {
            snapshots: vec![
                snapshot("a", 1, 0, 0),
                snapshot("b", 1, 0, 0),
                snapshot("c", 1, 0, 0),
            ],
            cycles: Cycles {
                cycles: vec![cycle("b", 2), cycle("c", 4)],
                unknown: 0,
            },
            ..Stats::default()
        };
        let (median, p85) = series(&stats, Measure::Cycle);
        assert_eq!(median, [None, Some(2.0), Some(4.0)]);
        assert_eq!(p85.unwrap(), [None, Some(2.0), Some(4.0)]);
    }
}
