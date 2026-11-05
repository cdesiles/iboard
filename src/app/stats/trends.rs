//! The Trends of Project stats: the four measures over the page, a small
//! multiple each, under one shared crosshair.

use chrono::NaiveDate;
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use iboard::stats::{Cycles, Limits, Snapshot, Stats};

use super::blocks::{Blocks, Part, Phase, clock_note, dim_if_stale, failed_row};
use super::{
    Align, Color32, Duration, Layout, Measure, Pos2, Rect, RichText, Sense, Shape, Stroke, TREND_H,
    TREND_LABEL_W, TRENDS_H, Vec2, block, block_label, theme, widgets,
};

/// The strip at a chart's left that holds its axis bounds.
const AXIS_W: f32 = 40.0;
/// How far the plot stays from its chart's top, bottom and right.
const PLOT_INSET: f32 = 4.0;
const PLOT_RIGHT: f32 = 8.0;
/// The month axis under the rows.
const MONTHS_H: f32 = 14.0;
/// The closest two month names sit.
const MONTH_GAP: f32 = 28.0;
/// The rows, top to bottom: size first, then what it costs.
const ROWS: [Measure; 4] = [
    Measure::Code,
    Measure::Complexity,
    Measure::Tests,
    Measure::Cycle,
];

/// Where a chart's bounds lie: the grid lines run along its top and bottom.
fn plot_of(chart: Rect) -> Rect {
    Rect::from_min_max(
        chart.min + Vec2::new(AXIS_W, PLOT_INSET),
        chart.max - Vec2::new(PLOT_RIGHT, PLOT_INSET),
    )
}

/// The width between two commits of a page of `count`.
fn step_of(plot: Rect, count: usize) -> f32 {
    plot.width() / (count.max(2) - 1) as f32
}

/// The axis of a measure over the values it shows: size and cycle time from 0,
/// complexity on tens from 100, or lower when the project is.
fn bounds(measure: Measure, known: impl Iterator<Item = f32>) -> Option<(f32, f32)> {
    let (low, high) = known.fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    if low > high {
        return None;
    }
    Some(match measure {
        Measure::Code | Measure::Cycle => (0.0, high),
        Measure::Tests => (low, high),
        Measure::Complexity => {
            let low = ((low / 10.0).floor() * 10.0).min(100.0);
            (low, ((high / 10.0).ceil() * 10.0).max(low + 10.0))
        }
    })
}

/// The commit where each month starts, with its name.
fn month_ticks(commits: &[Snapshot]) -> Vec<(usize, String)> {
    let mut ticks = Vec::new();
    let mut month = "";
    for (at, commit) in commits.iter().enumerate() {
        let this = commit.day().get(..7).unwrap_or_default();
        if this == month {
            continue;
        }
        month = this;
        if let Ok(day) = NaiveDate::parse_from_str(commit.day(), "%Y-%m-%d") {
            ticks.push((at, day.format("%b").to_string()));
        }
    }
    ticks
}

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

/// The four trends on one axis, each on its own scale and its own job; returns
/// the commit under the pointer, shared by every row's crosshair.
pub(super) fn trends(
    ui: &mut egui::Ui,
    blocks: &mut Blocks,
    limits: &Limits,
    hovered: Option<usize>,
) -> Option<usize> {
    block(ui, |ui| {
        ui.set_min_height(TRENDS_H);
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            block_label(ui, "Trends");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let commits = blocks.loaded(Part::Head).map_or(&[][..], |s| &s.snapshots);
                let text = match hovered.and_then(|at| commits.get(at)) {
                    Some(s) => format!("{} · {}", s.short, s.day()),
                    None if commits.is_empty() => String::new(),
                    None => format!("{} commits, oldest left", commits.len()),
                };
                ui.label(RichText::new(text).font(theme::meta()).color(p.ink_muted));
            });
        });
        let mut now_hovered = None;
        for (at, measure) in ROWS.into_iter().enumerate() {
            if at > 0 {
                widgets::rule(ui);
            }
            let row = TrendRow {
                measure,
                limits,
                hovered,
            };
            if let Some(at) = row.show(ui, blocks) {
                now_hovered = Some(at);
            }
        }
        let commits = blocks.loaded(Part::Head).map_or(&[][..], |s| &s.snapshots);
        month_axis(ui, commits);
        now_hovered
    })
}

/// The shared time axis under the rows: each month's name at its first commit.
fn month_axis(ui: &mut egui::Ui, commits: &[Snapshot]) {
    let p = theme::palette(ui);
    let gap = ui.spacing().item_spacing.x;
    let (row, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), MONTHS_H), Sense::hover());
    let chart = Rect::from_min_max(row.min + Vec2::new(TREND_LABEL_W + gap, 0.0), row.max);
    let plot = plot_of(chart);
    let step = step_of(plot, commits.len());
    let mut last = f32::MIN;
    for (at, name) in month_ticks(commits) {
        let x = plot.left() + step * at as f32;
        if x - last < MONTH_GAP {
            continue;
        }
        last = x;
        ui.painter().text(
            Pos2::new(x, row.center().y),
            egui::Align2::CENTER_CENTER,
            name,
            theme::meta(),
            p.ink_muted,
        );
    }
}

/// One row of the Trends, a block of its own.
struct TrendRow<'a> {
    measure: Measure,
    limits: &'a Limits,
    hovered: Option<usize>,
}

impl TrendRow<'_> {
    fn show(&self, ui: &mut egui::Ui, blocks: &mut Blocks) -> Option<usize> {
        let measure = self.measure;
        let part = Part::Trend(measure);
        let phase = blocks.phase(part);
        let clock = blocks.clock(part);
        let row = ui.horizontal(|ui| {
            dim_if_stale(ui, &phase);
            match &phase {
                Phase::Loaded { stats, .. } => {
                    let (values, extra) = series(stats, measure);
                    let shown = Labelled {
                        measure,
                        values: &values,
                        extra: extra.as_deref(),
                        at: self.hovered,
                    };
                    trend_label(ui, &shown, clock);
                    let limit = (measure == Measure::Complexity).then_some(self.limits.complexity);
                    let chart = Chart {
                        measure,
                        values: &values,
                        extra: extra.as_deref(),
                        limit,
                        hovered: self.hovered,
                    };
                    chart.show(ui)
                }
                Phase::Waiting { pulse } => {
                    trend_label(ui, &Labelled::empty(measure), clock);
                    chart_skeleton(ui, *pulse);
                    None
                }
                Phase::Failed(error) => {
                    trend_label(ui, &Labelled::empty(measure), None);
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), TREND_H),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_height(TREND_H);
                            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                            failed_row(ui, blocks, part, error);
                        },
                    );
                    None
                }
            }
        });
        blocks.settle(ui, part, &row.response);
        row.inner
    }
}

/// What a row's label says.
struct Labelled<'a> {
    measure: Measure,
    values: &'a [Option<f32>],
    extra: Option<&'a [Option<f32>]>,
    at: Option<usize>,
}

impl Labelled<'_> {
    fn empty(measure: Measure) -> Self {
        Self {
            measure,
            values: &[],
            extra: None,
            at: None,
        }
    }
}

/// A trend's name, its value at the hovered commit, its value now and any p85;
/// nothing but the name and the clock until it has values.
fn trend_label(ui: &mut egui::Ui, row: &Labelled, clock: Option<Duration>) {
    let p = theme::palette(ui);
    let measure = row.measure;
    let newest = |values: &[Option<f32>]| values.iter().rev().find_map(|v| *v);
    let now = newest(row.values);
    let at = row.at.and_then(|at| row.values.get(at).copied().flatten());
    let shown = at.or(now);
    ui.allocate_ui_with_layout(
        Vec2::new(TREND_LABEL_W, TREND_H),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_width(TREND_LABEL_W);
            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(measure.name())
                        .font(theme::ui_small())
                        .color(p.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    clock_note(ui, clock);
                });
            });
            if row.values.is_empty() {
                return;
            }
            let value = shown.map_or("–".to_string(), |v| measure.show(v));
            ui.label(
                RichText::new(value)
                    .font(egui::FontId::new(18.0, theme::ui_title().family))
                    .color(p.ink),
            );
            let Some(now) = now else {
                return;
            };
            let mut note = format!("now {}", measure.show(now));
            if let Some(p85) = row.extra.and_then(newest) {
                note = format!("p85 {} · {note}", measure.show(p85));
            }
            ui.label(RichText::new(note).font(theme::meta()).color(p.ink_muted));
        },
    );
}

/// The grid lines at a plot's bounds.
fn grid(painter: &egui::Painter, plot: Rect, color: Color32) {
    for y in [plot.top(), plot.bottom()] {
        painter.hline(plot.x_range(), y, Stroke::new(1.0, color));
    }
}

/// The Line chart placeholder: its grid at once, and from `pulse` on one soft
/// 4pt curve.
fn chart_skeleton(ui: &mut egui::Ui, pulse: bool) {
    const POINTS: usize = 32;
    let p = theme::palette(ui);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), TREND_H), Sense::hover());
    let plot = plot_of(rect);
    let painter = ui.painter_at(rect);
    grid(&painter, plot, p.line);
    if !pulse {
        return;
    }
    let curve = (0..POINTS)
        .map(|at| {
            let t = at as f32 / (POINTS - 1) as f32;
            let wave = (t * std::f32::consts::TAU * 1.25 + 0.6).sin();
            Pos2::new(
                plot.left() + t * plot.width(),
                plot.center().y + plot.height() * (0.24 * wave - 0.14 * t),
            )
        })
        .collect();
    let stroke = Stroke::new(4.0, theme::skeleton_fill(ui.ctx(), p));
    painter.add(Shape::line(curve, stroke));
}

/// The area under a run down to `base`, one quad per segment: epaint fills
/// only convex polygons, and an area under a line rarely is one.
fn area_mesh(run: &[Pos2], base: f32, fill: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    let mut vertex = |pos: Pos2| {
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color: fill,
        });
        mesh.vertices.len() as u32 - 1
    };
    let corners: Vec<(u32, u32)> = run
        .iter()
        .map(|point| (vertex(*point), vertex(Pos2::new(point.x, base))))
        .collect();
    for pair in corners.windows(2) {
        let ((top_a, low_a), (top_b, low_b)) = (pair[0], pair[1]);
        mesh.add_triangle(top_a, low_a, top_b);
        mesh.add_triangle(top_b, low_a, low_b);
    }
    mesh
}

/// Whether a limit's label goes under its line at `y` in `plot`: the series
/// runs above the line where the label would sit, or the top leaves no room,
/// so long as the bottom does.
fn label_under(run: &[Pos2], span: egui::Rangef, y: f32, height: f32, plot: Rect) -> bool {
    let crosses = run.windows(2).any(|pair| {
        let (a, b) = (pair[0], pair[1]);
        a.x <= span.max && b.x >= span.min && a.y.min(b.y) < y
    });
    (crosses || y - height < plot.top()) && y + height <= plot.bottom()
}

/// One small multiple: a line (or an area from 0), an optional dashed second
/// line and limit, and the shared crosshair.
struct Chart<'a> {
    measure: Measure,
    values: &'a [Option<f32>],
    extra: Option<&'a [Option<f32>]>,
    limit: Option<f32>,
    hovered: Option<usize>,
}

impl Chart<'_> {
    fn show(&self, ui: &mut egui::Ui) -> Option<usize> {
        let p = theme::palette(ui);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), TREND_H), Sense::hover());
        let name = format!("{} trend", self.measure.name());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &name));
        let painter = ui.painter_at(rect.expand(4.0));
        let plot = plot_of(rect);
        grid(&painter, plot, p.line);
        let known = self
            .values
            .iter()
            .chain(self.extra.into_iter().flatten())
            .flatten()
            .copied()
            .chain(self.limit);
        let Some((low, high)) = bounds(self.measure, known) else {
            painter.text(
                plot.center(),
                egui::Align2::CENTER_CENTER,
                "No issue closed on this page yet",
                theme::meta(),
                p.ink_faint,
            );
            return None;
        };
        let span = (high - low).max(f32::EPSILON);
        let step = step_of(plot, self.values.len());
        let point = |at: usize, v: f32| {
            Pos2::new(
                plot.left() + step * at as f32,
                plot.bottom() - (v - low) / span * plot.height(),
            )
        };
        for (value, y) in [(high, plot.top()), (low, plot.bottom())] {
            let text = if value == 0.0 && self.measure == Measure::Cycle {
                "0".to_string()
            } else {
                self.measure.show(value)
            };
            painter.text(
                Pos2::new(plot.left() - 6.0, y),
                egui::Align2::RIGHT_CENTER,
                text,
                theme::meta(),
                p.ink_muted,
            );
        }
        let main = runs(self.values, &point);
        if let Some(limit) = self.limit {
            let line = main.iter().flatten().copied().collect::<Vec<_>>();
            limit_line(&painter, plot, point(0, limit).y, limit, &line, p);
        }
        if let Some(extra) = self.extra {
            second_line(&painter, &runs(extra, &point), plot, p);
        }
        for run in main {
            if self.measure == Measure::Code && run.len() > 1 {
                let fill = p.accent.gamma_multiply(0.14);
                painter.add(area_mesh(&run, plot.bottom(), fill));
            }
            painter.add(Shape::line(run, Stroke::new(2.0, p.accent)));
        }
        if let Some(at) = self.hovered.filter(|at| *at < self.values.len()) {
            let x = plot.left() + step * at as f32;
            painter.vline(x, rect.top()..=plot.bottom(), Stroke::new(1.0, p.ink_faint));
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

/// The dashed limit across the plot, its label on the left clear of `line`.
fn limit_line(
    painter: &egui::Painter,
    plot: Rect,
    y: f32,
    limit: f32,
    line: &[Pos2],
    p: &theme::Palette,
) {
    painter.extend(Shape::dashed_line(
        &[Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
        Stroke::new(1.5, p.status_doing),
        2.0,
        3.0,
    ));
    let text = format!("refactor above {limit:.0}");
    let galley = painter.layout_no_wrap(text, theme::meta(), p.status_doing);
    let left = plot.left() + 4.0;
    let size = galley.size();
    let span = egui::Rangef::new(left, left + size.x);
    let top = if label_under(line, span, y, size.y + 2.0, plot) {
        y + 2.0
    } else {
        y - size.y - 2.0
    };
    painter.galley(Pos2::new(left, top), galley, p.status_doing);
}

/// The dashed p85, named at its newest point.
fn second_line(painter: &egui::Painter, runs: &[Vec<Pos2>], plot: Rect, p: &theme::Palette) {
    for run in runs {
        painter.extend(Shape::dashed_line(
            run,
            Stroke::new(1.5, p.ink_faint),
            4.0,
            3.0,
        ));
    }
    let Some(last) = runs.last().and_then(|run| run.last()) else {
        return;
    };
    let (y, anchor) = if last.y - 14.0 < plot.top() {
        (last.y + 3.0, egui::Align2::RIGHT_TOP)
    } else {
        (last.y - 3.0, egui::Align2::RIGHT_BOTTOM)
    };
    painter.text(
        Pos2::new(plot.right(), y),
        anchor,
        "p85",
        theme::meta(),
        p.ink_muted,
    );
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

    #[test]
    fn each_measure_has_its_axis() {
        let values = |list: &'static [f32]| list.iter().copied();
        let complexity = Measure::Complexity;
        assert_eq!(
            bounds(complexity, values(&[118.0, 138.0, 130.0])),
            Some((100.0, 140.0))
        );
        assert_eq!(
            bounds(complexity, values(&[64.0, 130.0])),
            Some((60.0, 130.0))
        );
        assert_eq!(
            bounds(Measure::Code, values(&[900.0, 1200.0])),
            Some((0.0, 1200.0))
        );
        assert_eq!(
            bounds(Measure::Cycle, values(&[2.0, 5.0])),
            Some((0.0, 5.0))
        );
        assert_eq!(
            bounds(Measure::Tests, values(&[20.0, 25.0])),
            Some((20.0, 25.0))
        );
        assert_eq!(bounds(Measure::Cycle, values(&[])), None);
    }

    #[test]
    fn an_area_is_quads_down_to_its_base_however_the_line_bends() {
        let run = [
            Pos2::new(0.0, 10.0),
            Pos2::new(10.0, 40.0),
            Pos2::new(20.0, 5.0),
        ];
        let mesh = area_mesh(&run, 50.0, Color32::WHITE);
        assert!(mesh.is_valid());
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.indices.len(), 12);
        // The dip at x = 10 stays out of the fill: no triangle spans over it.
        for triangle in mesh.indices.chunks(3) {
            let xs = triangle.iter().map(|at| mesh.vertices[*at as usize].pos.x);
            let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(l, h), x| (l.min(x), h.max(x)));
            assert!(hi - lo <= 10.0);
        }
    }

    #[test]
    fn a_limit_label_goes_under_when_the_series_runs_above_it() {
        let span = egui::Rangef::new(0.0, 50.0);
        let plot = Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 60.0));
        let below = [Pos2::new(0.0, 40.0), Pos2::new(100.0, 40.0)];
        assert!(!label_under(&below, span, 30.0, 14.0, plot));
        let above = [Pos2::new(0.0, 20.0), Pos2::new(100.0, 40.0)];
        assert!(label_under(&above, span, 30.0, 14.0, plot));
        let above_further_on = [Pos2::new(60.0, 40.0), Pos2::new(100.0, 10.0)];
        assert!(!label_under(&above_further_on, span, 30.0, 14.0, plot));
        assert!(label_under(&below, span, 10.0, 14.0, plot), "no room above");
        assert!(
            !label_under(&above, span, 50.0, 14.0, plot),
            "no room under"
        );
    }

    #[test]
    fn the_month_axis_names_each_months_first_commit() {
        let commits: Vec<Snapshot> = ["2026-09-29", "2026-09-30", "2026-10-01", "2026-11-02"]
            .map(|day| Snapshot {
                date: format!("{day}T10:00:00+00:00"),
                ..Snapshot::default()
            })
            .to_vec();
        assert_eq!(
            month_ticks(&commits),
            [
                (0, "Sep".to_string()),
                (2, "Oct".to_string()),
                (3, "Nov".to_string())
            ]
        );
    }
}
