//! Project stats, from Tools › Project Stats…: the project's code size,
//! complexity, tests and TODOs over a page of commits, browsable commit by
//! commit and page by page, with its hot spots and its agenda.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{Datelike, NaiveDate};
use iboard::stats::{self, Cycles, Hotspot, Progress, Snapshot, Stats};

use super::*;

/// Height of the trend chart.
const CHART_H: f32 = 150.0;
/// Side of one day of the agenda, and the gap between days.
const DAY: f32 = 11.0;
const DAY_GAP: f32 = 2.0;
/// Width of one figure tile.
const TILE_W: f32 = 168.0;

/// The measure the trend chart follows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Metric {
    Code,
    Complexity,
    Tests,
    Todos,
}

impl Metric {
    const ALL: [(Metric, &'static str); 4] = [
        (Metric::Code, "Code lines"),
        (Metric::Complexity, "Complexity"),
        (Metric::Tests, "Test share"),
        (Metric::Todos, "TODOs"),
    ];

    fn value(self, s: &Snapshot) -> f32 {
        match self {
            Metric::Code => s.code as f32,
            Metric::Complexity => s.mean_complexity(),
            Metric::Tests => s.test_share() * 100.0,
            Metric::Todos => s.todos as f32,
        }
    }

    fn show(self, value: f32) -> String {
        match self {
            Metric::Code | Metric::Todos => thousands(value.round() as usize),
            Metric::Complexity => format!("{value:.2}"),
            Metric::Tests => format!("{value:.1}%"),
        }
    }
}

/// The measuring under way, and how far it got.
type StatsRun = (Job<Result<Stats, String>>, Arc<Progress>);

/// The stats panel: a page of history being measured, or measured.
pub(super) struct StatsPanel {
    page: usize,
    run: Option<StatsRun>,
    stats: Option<Stats>,
    error: Option<String>,
    /// The commit shown, an index into the snapshots (oldest first).
    pub(super) selected: usize,
    metric: Metric,
}

impl StatsPanel {
    fn new(dir: &Path, root: &Path, page: usize, metric: Metric) -> Self {
        let progress = Arc::new(Progress::default());
        let shared = Arc::clone(&progress);
        let dir = dir.to_path_buf();
        let root = root.to_path_buf();
        let job = Job::spawn(move |cancel| stats::collect(&dir, &root, page, &shared, cancel));
        Self {
            page,
            run: Some((job, progress)),
            stats: None,
            error: None,
            selected: 0,
            metric,
        }
    }

    pub(super) fn is_loading(&self) -> bool {
        self.run.is_some()
    }

    pub(super) fn snapshots(&self) -> &[Snapshot] {
        self.stats.as_ref().map_or(&[], |s| &s.snapshots)
    }

    fn step(&mut self, by: isize) {
        let last = self.snapshots().len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(by).min(last);
    }
}

/// What the panel asked for this frame.
enum StatsAsk {
    Close,
    Page(usize),
}

impl IboardApp {
    /// Opens the stats panel on the project, measuring its newest commits.
    pub(super) fn open_stats(&mut self) {
        if self.session.stats.is_none() {
            self.session.stats = Some(StatsPanel::new(
                &self.located.project_dir,
                &self.located.root,
                0,
                Metric::Code,
            ));
        }
    }

    /// Takes the measures once they are in.
    pub(super) fn poll_stats(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.session.stats.as_mut() else {
            return;
        };
        let Some((job, _)) = panel.run.as_ref() else {
            return;
        };
        match job.answer() {
            None => ctx.request_repaint_after(job::POLL_EVERY),
            Some(Ok(stats)) => {
                panel.selected = stats.snapshots.len().saturating_sub(1);
                panel.stats = Some(stats);
                panel.run = None;
            }
            Some(Err(err)) => {
                panel.error = Some(err);
                panel.run = None;
            }
        }
    }

    /// `←` and `→` walk the commits; the panel owns the keyboard while up.
    pub(super) fn stats_keys(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.session.stats.as_mut().filter(|p| !p.is_loading()) else {
            return;
        };
        if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
            panel.step(-1);
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
            panel.step(1);
        }
    }

    pub(super) fn stats_window(&mut self, ui: &egui::Ui) {
        let Some(panel) = self.session.stats.as_mut() else {
            return;
        };
        let p = theme::palette(ui);
        let screen = ui.ctx().content_rect();
        let width = (screen.width() - 80.0).clamp(480.0, 980.0);
        let height = (screen.height() - 120.0).max(320.0);
        let mut ask = None;
        let modal = egui::Modal::new(egui::Id::new("stats"))
            .frame(widgets::popover_frame(ui))
            .backdrop_color(p.scrim)
            .show(ui.ctx(), |ui| {
                ui.set_width(width);
                ui.style_mut().interaction.selectable_labels = false;
                if widgets::modal_header(ui, "Project stats", |ui| page_label(ui, panel)) {
                    ask = Some(StatsAsk::Close);
                }
                widgets::rule(ui);
                Frame::new()
                    .inner_margin(Margin::same(theme::SPACE_5 as i8))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                if let Some(page) = stats_body(ui, panel) {
                                    ask = Some(StatsAsk::Page(page));
                                }
                            });
                    });
                widgets::rule(ui);
                stats_footer(ui, panel.stats.as_ref());
            });
        let metric = panel.metric;
        match ask {
            Some(StatsAsk::Close) => self.session.stats = None,
            Some(StatsAsk::Page(page)) => {
                self.session.stats = Some(StatsPanel::new(
                    &self.located.project_dir,
                    &self.located.root,
                    page,
                    metric,
                ));
            }
            None if modal.should_close() => self.session.stats = None,
            None => {}
        }
    }
}

/// The commits the page covers, beside the title.
fn page_label(ui: &mut egui::Ui, panel: &StatsPanel) {
    let p = theme::palette(ui);
    let first = panel.page * stats::PAGE;
    let text = match panel.snapshots() {
        [] => format!("from commit {}", first + 1),
        shown => format!("commits {}–{} before HEAD", first + 1, first + shown.len()),
    };
    ui.label(RichText::new(text).font(theme::meta()).color(p.ink_muted));
}

/// The panel's body; returns the page to read instead, when asked.
fn stats_body(ui: &mut egui::Ui, panel: &mut StatsPanel) -> Option<usize> {
    let p = theme::palette(ui);
    if let Some((job, progress)) = &panel.run {
        loading(ui, progress, job.elapsed());
        return None;
    }
    if let Some(error) = &panel.error {
        ui.label(
            RichText::new(format!("Couldn't measure this project: {error}"))
                .font(theme::ui_body())
                .color(p.danger),
        );
        return None;
    }
    let stats = panel.stats.as_ref()?;
    if stats.snapshots.is_empty() {
        ui.label(RichText::new("No commits on this page.").color(p.ink_muted));
        return (panel.page > 0).then(|| panel.page - 1);
    }
    let older = stats.older;
    let mut page = None;
    ui.horizontal(|ui| {
        if let Some(metric) = widgets::segmented(ui, panel.metric, &Metric::ALL) {
            panel.metric = metric;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled_ui(panel.page > 0, |ui| {
                if widgets::button(ui, ButtonKind::Ghost, None, "Newer commits", None).clicked() {
                    page = Some(panel.page - 1);
                }
            });
            ui.add_enabled_ui(older, |ui| {
                let label = format!("Older {}", stats::PAGE);
                if widgets::button(ui, ButtonKind::Ghost, Some(Icon::Back), &label, None).clicked()
                {
                    page = Some(panel.page + 1);
                }
            });
        });
    });
    ui.add_space(theme::SPACE_3);
    if let Some(at) = chart(ui, panel.snapshots(), panel.selected, panel.metric) {
        panel.selected = at;
    }
    ui.add_space(theme::SPACE_3);
    commit_line(ui, panel);
    ui.add_space(theme::SPACE_4);
    let stats = panel.stats.as_ref()?;
    let (Some(base), Some(shown)) = (stats.snapshots.first(), stats.snapshots.get(panel.selected))
    else {
        return page;
    };
    tiles(ui, base, shown);
    ui.add_space(theme::SPACE_5);
    tops(ui, shown);
    ui.add_space(theme::SPACE_5);
    hotspots(ui, &stats.hotspots);
    ui.add_space(theme::SPACE_5);
    cycle_time(ui, &stats.cycles);
    ui.add_space(theme::SPACE_5);
    if let Some(newest) = stats.snapshots.last() {
        agenda(ui, &stats.agenda, newest.day());
    }
    page
}

/// The first run's wait: which commit is being read, out of how many.
fn loading(ui: &mut egui::Ui, progress: &Progress, elapsed: Duration) {
    let p = theme::palette(ui);
    let (done, total) = progress.get();
    let text = if total == 0 {
        "Listing commits…".to_string()
    } else {
        format!("Processing commit {}/{total}", (done + 1).min(total))
    };
    ui.add_space(theme::SPACE_6);
    ui.label(RichText::new(text).font(theme::ui_strong()).color(p.ink));
    ui.add_space(theme::SPACE_3);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
    let radius = CornerRadius::same(3);
    ui.painter().rect_filled(rect, radius, p.surface_sunken);
    let share = if total == 0 {
        0.0
    } else {
        done as f32 / total as f32
    };
    let mut filled = rect;
    filled.set_width(rect.width() * share);
    ui.painter().rect_filled(filled, radius, p.accent);
    ui.add_space(theme::SPACE_3);
    ui.label(
        RichText::new(format!(
            "{} · each commit is read once, then kept for next time.",
            stopwatch(elapsed)
        ))
        .font(theme::meta())
        .color(p.ink_muted),
    );
    ui.add_space(theme::SPACE_6);
}

/// The metric across the page, oldest left; returns the commit clicked.
fn chart(
    ui: &mut egui::Ui,
    snapshots: &[Snapshot],
    selected: usize,
    metric: Metric,
) -> Option<usize> {
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), CHART_H), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Trend across commits")
    });
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS_MD),
        p.surface_sunken,
        Stroke::new(1.0, p.line),
        StrokeKind::Inside,
    );
    let values: Vec<f32> = snapshots.iter().map(|s| metric.value(s)).collect();
    let (low, high) = values
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let span = (high - low).max(f32::EPSILON);
    let plot = rect.shrink2(Vec2::new(theme::SPACE_5, theme::SPACE_6));
    let step = plot.width() / (values.len().max(2) - 1) as f32;
    let point = |at: usize, v: f32| {
        Pos2::new(
            plot.left() + step * at as f32,
            plot.bottom() - (v - low) / span * plot.height(),
        )
    };
    let line: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(at, v)| point(at, *v))
        .collect();
    painter.add(Shape::line(line.clone(), Stroke::new(1.5, p.accent)));
    for at in [0, values.len() - 1] {
        painter.circle_filled(line[at], 2.5, p.accent);
    }
    let font = theme::meta();
    let corner = |text: String, pos: Pos2, align: egui::Align2| {
        painter.text(pos, align, text, font.clone(), p.ink_muted);
    };
    corner(
        metric.show(high),
        rect.left_top() + Vec2::splat(6.0),
        egui::Align2::LEFT_TOP,
    );
    corner(
        metric.show(low),
        rect.left_bottom() + Vec2::new(6.0, -6.0),
        egui::Align2::LEFT_BOTTOM,
    );
    let nearest = |pos: Pos2| {
        ((pos.x - plot.left()) / step)
            .round()
            .clamp(0.0, (values.len() - 1) as f32) as usize
    };
    if let Some(at) = response.hover_pos().map(nearest) {
        painter.vline(line[at].x, rect.y_range(), Stroke::new(1.0, p.line_strong));
        let s = &snapshots[at];
        response.clone().on_hover_text_at_pointer(format!(
            "{} · {} · {}\n{}",
            s.short,
            s.day(),
            metric.show(values[at]),
            s.subject
        ));
    }
    if let Some(&shown) = line.get(selected) {
        painter.vline(shown.x, rect.y_range(), Stroke::new(1.5, p.accent));
        painter.circle(shown, 4.5, p.surface, Stroke::new(2.0, p.accent));
    }
    let clicked = response.clicked();
    response
        .interact_pointer_pos()
        .filter(|_| clicked)
        .map(nearest)
}

/// The commit shown, and the buttons that walk to its neighbours.
fn commit_line(ui: &mut egui::Ui, panel: &mut StatsPanel) {
    let p = theme::palette(ui);
    let count = panel.snapshots().len();
    ui.horizontal(|ui| {
        ui.add_enabled_ui(panel.selected > 0, |ui| {
            if widgets::button(ui, ButtonKind::Secondary, None, "Previous", Some("←")).clicked() {
                panel.step(-1);
            }
        });
        ui.add_enabled_ui(panel.selected + 1 < count, |ui| {
            if widgets::button(ui, ButtonKind::Secondary, None, "Next", Some("→")).clicked() {
                panel.step(1);
            }
        });
        ui.add_space(theme::SPACE_3);
        let Some(s) = panel.snapshots().get(panel.selected) else {
            return;
        };
        ui.label(
            RichText::new(&s.short)
                .font(theme::ui_strong())
                .color(p.ink),
        );
        ui.label(
            RichText::new(format!(
                "{} · {} · {}/{count}",
                s.day(),
                s.author,
                panel.selected + 1
            ))
            .font(theme::meta())
            .color(p.ink_muted),
        );
        ui.add(
            egui::Label::new(
                RichText::new(&s.subject)
                    .font(theme::ui_body())
                    .color(p.ink),
            )
            .truncate(),
        );
    });
}

/// Which way a measure going up goes.
#[derive(Clone, Copy)]
enum Up {
    Better,
    Worse,
    Neutral,
}

/// The figures at the commit shown, each with its change since the page's
/// oldest commit.
fn tiles(ui: &mut egui::Ui, base: &Snapshot, s: &Snapshot) {
    let since = format!("since {}", base.short);
    let tiles = [
        (
            "Code lines",
            thousands(s.code),
            format!("{} files", thousands(s.files)),
            delta(s.code as f32 - base.code as f32, 0, Up::Neutral),
        ),
        (
            "Complexity",
            format!("{:.2}", s.mean_complexity()),
            format!("per function · {} in all", thousands(s.complexity)),
            delta(s.mean_complexity() - base.mean_complexity(), 2, Up::Worse),
        ),
        (
            "Test share",
            format!("{:.1}%", s.test_share() * 100.0),
            format!("{} tests · lines of test code", thousands(s.tests)),
            delta((s.test_share() - base.test_share()) * 100.0, 1, Up::Better),
        ),
        (
            "TODO · FIXME",
            thousands(s.todos),
            format!("{:.1} per 1000 lines", s.todo_density()),
            delta(s.todos as f32 - base.todos as f32, 0, Up::Worse),
        ),
        (
            "Large files",
            thousands(s.large_files),
            format!("over {} code lines", stats::LARGE_FILE),
            delta(s.large_files as f32 - base.large_files as f32, 0, Up::Worse),
        ),
        (
            "Comments",
            format!("{:.0}%", s.comment_ratio() * 100.0),
            "comment lines per code line".to_string(),
            delta(
                (s.comment_ratio() - base.comment_ratio()) * 100.0,
                0,
                Up::Neutral,
            ),
        ),
    ];
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_3);
        for (label, value, detail, change) in tiles {
            let line = change_line(ui, change, &since);
            tile(ui, label, &value, &detail, Some(line));
        }
    });
}

/// A change and how it reads, or `None` when nothing changed.
fn delta(change: f32, decimals: usize, up: Up) -> Option<(String, Up, bool)> {
    let shown = format!("{change:+.decimals$}");
    let zero = format!("{:+.decimals$}", 0.0);
    (shown != zero && shown != format!("-{}", &zero[1..])).then_some((shown, up, change > 0.0))
}

fn tile(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    detail: &str,
    line: Option<(String, Color32)>,
) {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(Margin::same(theme::SPACE_4 as i8))
        .show(ui, |ui| {
            ui.set_width(TILE_W);
            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
            ui.label(
                RichText::new(label.to_uppercase())
                    .font(theme::ui_label())
                    .color(p.ink_muted),
            );
            ui.label(RichText::new(value).font(theme::ui_title()).color(p.ink));
            ui.add(
                egui::Label::new(RichText::new(detail).font(theme::meta()).color(p.ink_muted))
                    .truncate(),
            );
            if let Some((text, color)) = line {
                ui.label(RichText::new(text).font(theme::meta()).color(color));
            }
        });
}

/// A tile's change since the page's oldest commit, coloured by how it reads.
fn change_line(
    ui: &egui::Ui,
    change: Option<(String, Up, bool)>,
    since: &str,
) -> (String, Color32) {
    let p = theme::palette(ui);
    match change {
        Some((shown, up, rose)) => {
            let color = match (up, rose) {
                (Up::Neutral, _) => p.ink_muted,
                (Up::Better, true) | (Up::Worse, false) => p.status_done,
                _ => p.danger,
            };
            (format!("{shown} {since}"), color)
        }
        None => ("unchanged".to_string(), p.ink_faint),
    }
}

/// The largest and most complex files, and the languages, at the commit.
fn tops(ui: &mut egui::Ui, s: &Snapshot) {
    ui.columns(3, |columns| {
        ranked(&mut columns[0], "Largest files", &s.largest, "lines");
        ranked(&mut columns[1], "Most complex files", &s.complex, "");
        ranked(&mut columns[2], "Languages", &s.languages, "lines");
    });
}

fn heading(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(RichText::new(text).font(theme::ui_strong()).color(p.ink));
    ui.add_space(theme::SPACE_2);
}

fn ranked(ui: &mut egui::Ui, title: &str, rows: &[(String, usize)], unit: &str) {
    let p = theme::palette(ui);
    heading(ui, title);
    for (name, count) in rows {
        ui.horizontal(|ui| {
            let figure = format!("{} {unit}", thousands(*count));
            let figure_w = widgets::text_width(ui, &figure, theme::meta());
            ui.add_sized(
                Vec2::new(
                    (ui.available_width() - figure_w - theme::SPACE_3).max(0.0),
                    16.0,
                ),
                egui::Label::new(RichText::new(name).font(theme::meta()).color(p.ink)).truncate(),
            )
            .on_hover_text(name);
            ui.label(
                RichText::new(figure.trim_end())
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        });
    }
}

/// The files that change most, weighed by their complexity: where debt costs.
fn hotspots(ui: &mut egui::Ui, spots: &[Hotspot]) {
    let p = theme::palette(ui);
    heading(ui, "Hot spots");
    ui.label(
        RichText::new("Files changed most on this page, by changes × complexity today.")
            .font(theme::meta())
            .color(p.ink_muted),
    );
    ui.add_space(theme::SPACE_2);
    let top = spots
        .iter()
        .map(|s| s.changes * s.complexity)
        .max()
        .unwrap_or(1)
        .max(1);
    for spot in spots {
        ui.horizontal(|ui| {
            let (bar, _) = ui.allocate_exact_size(Vec2::new(80.0, 6.0), Sense::hover());
            ui.painter()
                .rect_filled(bar, CornerRadius::same(3), p.surface_sunken);
            let mut filled = bar;
            filled.set_width(bar.width() * (spot.changes * spot.complexity) as f32 / top as f32);
            ui.painter()
                .rect_filled(filled, CornerRadius::same(3), p.status_doing);
            ui.label(RichText::new(&spot.path).font(theme::meta()).color(p.ink));
            ui.label(
                RichText::new(format!(
                    "{} changes · complexity {} · {} lines",
                    spot.changes,
                    spot.complexity,
                    thousands(spot.code)
                ))
                .font(theme::meta())
                .color(p.ink_muted),
            );
        });
    }
}

/// How long the issues closed on this page took, and the slowest of them.
fn cycle_time(ui: &mut egui::Ui, cycles: &Cycles) {
    let p = theme::palette(ui);
    heading(ui, "Cycle time");
    ui.label(
        RichText::new(
            "From an issue's first build run, or its first commit on the board, \
             to the commit that closes it.",
        )
        .font(theme::meta())
        .color(p.ink_muted),
    );
    ui.add_space(theme::SPACE_2);
    let (Some(median), Some(p85)) = (cycles.percentile(0.5), cycles.percentile(0.85)) else {
        ui.label(
            RichText::new("No commit on this page closes an issue whose start is known.")
                .font(theme::meta())
                .color(p.ink_faint),
        );
        return;
    };
    let unknown = match cycles.unknown {
        0 => String::new(),
        n => format!(", {n} more of unknown start"),
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_3);
        let closed = cycles.cycles.len();
        let tiles = [
            (
                "Median",
                stats::span(median),
                format!("over {closed} issues{unknown}"),
            ),
            (
                "85th percentile",
                stats::span(p85),
                "85% close within".to_string(),
            ),
        ];
        for (label, value, detail) in tiles {
            tile(ui, label, &value, &detail, None);
        }
    });
    ui.add_space(theme::SPACE_3);
    for cycle in cycles.cycles.iter().take(5) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(stats::span(cycle.seconds))
                    .font(theme::ui_strong())
                    .color(p.ink),
            );
            ui.label(RichText::new(&cycle.issue).font(theme::meta()).color(p.ink));
            ui.label(
                RichText::new(format!("closed in {}", cycle.short))
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        });
    }
}

/// Commits per day, a column per week, up to the page's newest commit.
fn agenda(ui: &mut egui::Ui, days: &BTreeMap<String, usize>, newest: &str) {
    let p = theme::palette(ui);
    heading(ui, "Agenda");
    let Ok(end) = NaiveDate::parse_from_str(newest, "%Y-%m-%d") else {
        return;
    };
    let weeks = (stats::AGENDA_DAYS / 7) as usize;
    let last_monday = end - chrono::Duration::days(i64::from(end.weekday().num_days_from_monday()));
    let first = last_monday - chrono::Duration::weeks(weeks as i64 - 1);
    let busiest = days.values().copied().max().unwrap_or(1).max(1);
    let cell = DAY + DAY_GAP;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(cell * weeks as f32, cell * 7.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let mut hovered = None;
    for week in 0..weeks {
        for weekday in 0..7 {
            let day = first + chrono::Duration::days((week * 7 + weekday) as i64);
            if day > end {
                continue;
            }
            let key = day.format("%Y-%m-%d").to_string();
            let count = days.get(&key).copied().unwrap_or(0);
            let min = rect.min + Vec2::new(week as f32 * cell, weekday as f32 * cell);
            let square = Rect::from_min_size(min, Vec2::splat(DAY));
            let fill = if count == 0 {
                p.surface_sunken
            } else {
                p.accent_soft
                    .lerp_to_gamma(p.accent, (count as f32 / busiest as f32).max(0.2))
            };
            painter.rect_filled(square, CornerRadius::same(2), fill);
            if response.hover_pos().is_some_and(|pos| square.contains(pos)) {
                hovered = Some((key, count));
            }
        }
    }
    if let Some((day, count)) = hovered {
        response.on_hover_text_at_pointer(format!("{day}: {count} commits"));
    }
    let total: usize = days.values().sum();
    ui.label(
        RichText::new(format!(
            "{total} commits from {} to {newest}",
            first.format("%Y-%m-%d")
        ))
        .font(theme::meta())
        .color(p.ink_muted),
    );
}

fn stats_footer(ui: &mut egui::Ui, stats: Option<&Stats>) {
    let p = theme::palette(ui);
    let kept = stats
        .and_then(|s| s.cache.as_ref())
        .map(|file| format!(" Kept in {}.", display_path(file)))
        .unwrap_or_default();
    Frame::new()
        .inner_margin(Margin::symmetric(
            theme::SPACE_5 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "Read from the source at each commit: complexity is McCabe's, \
                         estimated; test share is test lines over code lines, not run \
                         coverage.{kept}"
                    ))
                    .font(theme::meta())
                    .color(p.ink_muted),
                )
                .wrap(),
            );
        });
}

/// `12345` as `12,345`.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (at, c) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
