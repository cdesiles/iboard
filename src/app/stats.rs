//! Project stats, from Tools › Project Stats…: what a page of commits says
//! about the cost of changing the project. Four figures, their trends on one
//! shared axis, a Refactor watch, the biggest files and the Agenda (see
//! `design/components/StatsScreen`).

use std::sync::Arc;

use iboard::stats::{self, Cycles, FileStat, Progress, Snapshot, Stats};

use super::agenda::{self, Agenda};
use super::*;

/// A trend's chart height, and the width of its name and value beside it.
const TREND_H: f32 = 64.0;
const TREND_LABEL_W: f32 = 150.0;
/// The strip at a chart's right that holds its axis bounds.
const AXIS_W: f32 = 56.0;
/// Narrower than this, the Refactor watch goes under the trends.
const SIDE_BY_SIDE: f32 = 760.0;
/// Complexity per thousand code lines above which a refactor is due.
const COMPLEXITY_LIMIT: f32 = 130.0;
/// A rise in complexity per kLOC over the page that turns its signal on.
const COMPLEXITY_GROWTH: f32 = 0.05;
/// A fall of the test share, in points, that turns its signal on.
const TESTS_SLIDE: f32 = 3.0;
/// How much slower the page's newer issues may close before cycle time drifts.
const CYCLE_DRIFT: f32 = 1.5;
/// The files listed before "Show all".
const FILES_SHOWN: usize = 8;
/// Placeholders wait this long, and the Thinking mark and clock this long.
const SKELETON_AFTER: Duration = Duration::from_millis(300);
const CLOCK_AFTER: Duration = Duration::from_secs(2);

/// One of the four measures, as a tile and as a trend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Measure {
    Cycle,
    Complexity,
    Tests,
    Code,
}

impl Measure {
    const ALL: [Measure; 4] = [
        Measure::Cycle,
        Measure::Complexity,
        Measure::Tests,
        Measure::Code,
    ];

    fn name(self) -> &'static str {
        match self {
            Measure::Cycle => "Median cycle time",
            Measure::Complexity => "Complexity",
            Measure::Tests => "Test share",
            Measure::Code => "Lines of code",
        }
    }

    fn at(self, s: &Snapshot) -> f32 {
        match self {
            Measure::Cycle => 0.0,
            Measure::Complexity => s.complexity_per_kloc(),
            Measure::Tests => s.test_share() * 100.0,
            Measure::Code => s.code as f32,
        }
    }

    fn show(self, value: f32) -> String {
        match self {
            Measure::Cycle => stats::span((value * 86_400.0).round() as i64),
            Measure::Complexity => format!("{value:.0}"),
            Measure::Tests => format!("{value:.1}%"),
            Measure::Code => thousands(value.round() as usize),
        }
    }
}

/// How the Biggest files table is sorted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FileSort {
    Lines,
    Complexity,
    Changes,
}

impl FileSort {
    const ALL: [(FileSort, &'static str); 3] = [
        (FileSort::Lines, "Lines"),
        (FileSort::Complexity, "Complexity"),
        (FileSort::Changes, "Changes"),
    ];
}

/// The measuring under way, and how far it got.
type StatsRun = (Job<Result<Stats, String>>, Arc<Progress>);

/// The stats panel: a page of history being measured, or measured.
pub(super) struct StatsPanel {
    page: usize,
    run: Option<StatsRun>,
    stats: Option<Stats>,
    error: Option<String>,
    /// The commit under the trends' shared crosshair, an index into the
    /// snapshots.
    pub(super) hovered: Option<usize>,
    sort: FileSort,
    all_files: bool,
    pub(super) agenda: Agenda,
}

impl StatsPanel {
    fn new(dir: &Path, root: &Path, page: usize, range: agenda::Range) -> Self {
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
            hovered: None,
            sort: FileSort::Lines,
            all_files: false,
            agenda: Agenda::new(range),
        }
    }

    pub(super) fn is_loading(&self) -> bool {
        self.run.is_some()
    }

    pub(super) fn snapshots(&self) -> &[Snapshot] {
        self.stats.as_ref().map_or(&[], |s| &s.snapshots)
    }
}

/// What the panel asked for this frame.
enum StatsAsk {
    Close,
    Page(usize),
    DraftRefactor(String, String),
}

impl IboardApp {
    /// Opens the stats panel on the project, measuring its newest commits.
    pub(super) fn open_stats(&mut self) {
        if self.session.stats.is_none() {
            let (project, root) = self.stats_project();
            self.session.stats = Some(StatsPanel::new(
                &project,
                &root,
                0,
                self.session.agenda_range,
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
                panel.stats = Some(stats);
                panel.run = None;
            }
            Some(Err(err)) => {
                panel.error = Some(err);
                panel.run = None;
            }
        }
    }

    /// The panel owns the keyboard while up; the Agenda takes its keys.
    pub(super) fn stats_keys(&mut self, ctx: &egui::Context) {
        if self.session.stats.as_ref().is_some_and(|p| !p.is_loading()) {
            self.agenda_keys(ctx);
        }
    }

    pub(super) fn stats_window(&mut self, ui: &egui::Ui) {
        let Some(panel) = self.session.stats.as_mut() else {
            return;
        };
        let p = theme::palette(ui);
        let screen = ui.ctx().content_rect();
        let width = (screen.width() - 80.0).clamp(480.0, 1280.0);
        let height = (screen.height() - 120.0).max(320.0);
        let mut ask = None;
        let modal = egui::Modal::new(egui::Id::new("stats"))
            .frame(widgets::popover_frame(ui))
            .backdrop_color(p.scrim)
            .show(ui.ctx(), |ui| {
                ui.set_width(width);
                ui.style_mut().interaction.selectable_labels = false;
                let mut page = None;
                if widgets::modal_header(ui, "Project stats", |ui| page = header(ui, panel)) {
                    ask = Some(StatsAsk::Close);
                }
                if let Some(page) = page {
                    ask = Some(StatsAsk::Page(page));
                }
                widgets::rule(ui);
                Frame::new().fill(p.bg).show(ui, |ui| {
                    theme::scroll_area(
                        ui,
                        egui::ScrollArea::vertical()
                            .content_margin(theme::scroll_margin(theme::SPACE_5, theme::SPACE_5))
                            .max_height(height + 2.0 * theme::SPACE_5)
                            .auto_shrink([false, true]),
                        true,
                        |ui| {
                            if let Some(draft) = stats_body(ui, panel) {
                                ask = Some(draft);
                            }
                        },
                    );
                });
                widgets::rule(ui);
                stats_footer(ui, panel.stats.as_ref());
            });
        self.session.agenda_range = panel.agenda.range;
        let range = panel.agenda.range;
        match ask {
            Some(StatsAsk::Close) => self.session.stats = None,
            Some(StatsAsk::Page(page)) => {
                let (project, root) = self.stats_project();
                self.session.stats = Some(StatsPanel::new(&project, &root, page, range));
            }
            Some(StatsAsk::DraftRefactor(title, description)) => {
                self.session.stats = None;
                self.draft_refactor(title, description);
            }
            None if modal.should_close() => self.session.stats = None,
            None => {}
        }
    }

    /// Opens the composer in Todo with the refactor issue drafted; nothing is
    /// written until it is confirmed.
    fn draft_refactor(&mut self, title: String, description: String) {
        let composer = self.session.composer.get_or_insert_with(Composer::default);
        if composer.is_empty() {
            composer.title = title;
            composer.description = description;
            composer.stage = TitleStage::Skipped;
        }
        self.open_composer();
    }
}

/// The page shown and its neighbours, or the reading under way; returns the
/// page asked for.
fn header(ui: &mut egui::Ui, panel: &StatsPanel) -> Option<usize> {
    let p = theme::palette(ui);
    if let Some((job, progress)) = &panel.run {
        let (done, total) = progress.get();
        let text = if total == 0 {
            "Listing commits…".to_string()
        } else {
            format!("Reading commit {}/{total}…", (done + 1).min(total))
        };
        ui.label(RichText::new(text).font(theme::meta()).color(p.ink_muted));
        if job.elapsed() >= CLOCK_AFTER {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(theme::ICON_SIZE), Sense::hover());
            theme::paint_icon(ui.painter(), Icon::Thinking, rect, p.ink);
            ui.label(
                RichText::new(stopwatch(job.elapsed()))
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        }
        return None;
    }
    let first = panel.page * stats::PAGE;
    let text = match panel.snapshots() {
        [] => format!("from commit {}", first + 1),
        shown => format!("commits {}–{} before HEAD", first + 1, first + shown.len()),
    };
    ui.label(RichText::new(text).font(theme::meta()).color(p.ink_muted));
    let mut page = None;
    ui.add_enabled_ui(panel.page > 0, |ui| {
        if widgets::button(ui, ButtonKind::Ghost, None, "Newer", None).clicked() {
            page = Some(panel.page - 1);
        }
    });
    let older = panel.stats.as_ref().is_some_and(|s| s.older);
    ui.add_enabled_ui(older, |ui| {
        let label = format!("Older {}", stats::PAGE);
        if widgets::button(ui, ButtonKind::Ghost, Some(Icon::Back), &label, None).clicked() {
            page = Some(panel.page + 1);
        }
    });
    page
}

/// The panel's body: tiles, trends and watch, files, Agenda.
fn stats_body(ui: &mut egui::Ui, panel: &mut StatsPanel) -> Option<StatsAsk> {
    let p = theme::palette(ui);
    ui.spacing_mut().item_spacing.y = theme::SPACE_5;
    if let Some(error) = &panel.error {
        ui.label(
            RichText::new(format!("Couldn't measure this project: {error}"))
                .font(theme::ui_body())
                .color(p.danger),
        );
        return None;
    }
    let pulse = panel
        .run
        .as_ref()
        .is_none_or(|(job, _)| job.elapsed() >= SKELETON_AFTER);
    let Some(stats) = panel.stats.as_ref() else {
        skeleton(ui, pulse);
        agenda::agenda_ui(ui, &mut panel.agenda, None);
        return None;
    };
    if stats.snapshots.is_empty() {
        ui.label(RichText::new("No commits on this page.").color(p.ink_muted));
        return None;
    }
    tiles(ui, stats);
    let mut draft = None;
    let mut hovered = None;
    split(
        ui,
        |ui| hovered = trends(ui, stats, panel.hovered),
        |ui| draft = watch(ui, stats),
    );
    if hovered != panel.hovered {
        panel.hovered = hovered;
        ui.ctx().request_repaint();
    }
    files_table(ui, &stats.files, &mut panel.sort, &mut panel.all_files);
    agenda::agenda_ui(ui, &mut panel.agenda, Some(&stats.agenda));
    draft
}

/// Three quarters and one quarter side by side, or one under the other on a
/// narrow window.
fn split(ui: &mut egui::Ui, wide: impl FnOnce(&mut egui::Ui), narrow: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    if width < SIDE_BY_SIDE {
        wide(ui);
        narrow(ui);
        return;
    }
    let gap = theme::SPACE_4;
    let three = (width - gap) * 0.75;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (w, draw) in [
            (three, Box::new(wide) as Box<dyn FnOnce(&mut egui::Ui)>),
            (width - gap - three, Box::new(narrow)),
        ] {
            ui.allocate_ui_with_layout(Vec2::new(w, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(w);
                draw(ui);
            });
        }
    });
}

/// A `surface` block with a `line` edge, as tiles and panels are.
fn block<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(Margin::symmetric(14, theme::SPACE_4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme::SPACE_2 + 2.0;
            add(ui)
        })
        .inner
}

/// An uppercase `ui-label`, as tiles and panels are headed.
fn block_label(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(
        RichText::new(text.to_uppercase())
            .font(theme::ui_label())
            .color(p.ink_muted),
    );
}

/// A pulsing placeholder bar, `share` of the width; blank before
/// [`SKELETON_AFTER`].
fn placeholder(ui: &mut egui::Ui, share: f32, height: f32, pulse: bool) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width() * share, height),
        Sense::hover(),
    );
    if pulse {
        let fill = theme::skeleton_fill(ui.ctx(), p);
        ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    }
}

/// The screen while the history is read: every block at its final size, with
/// its label, and its data pulsing.
fn skeleton(ui: &mut egui::Ui, pulse: bool) {
    ui.columns(Measure::ALL.len(), |columns| {
        for (ui, measure) in columns.iter_mut().zip(Measure::ALL) {
            block(ui, |ui| {
                block_label(ui, measure.name());
                placeholder(ui, 0.46, 22.0, pulse);
                placeholder(ui, 0.58, 8.0, pulse);
            });
        }
    });
    split(
        ui,
        |ui| {
            block(ui, |ui| {
                block_label(ui, "Trends");
                for measure in Measure::ALL {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(Vec2::new(TREND_LABEL_W, TREND_H), |ui| {
                            ui.set_width(TREND_LABEL_W);
                            ui.label(RichText::new(measure.name()).font(theme::ui_small()));
                        });
                        placeholder(ui, 1.0, TREND_H, pulse);
                    });
                }
            });
        },
        |ui| {
            block(ui, |ui| {
                block_label(ui, "Refactor watch");
                for share in [0.9, 0.7, 0.8, 0.6] {
                    placeholder(ui, share, 12.0, pulse);
                }
            });
        },
    );
    block(ui, |ui| {
        block_label(ui, "Biggest files");
        for share in [0.62, 0.44, 0.54, 0.36, 0.48] {
            placeholder(ui, share, 8.0, pulse);
        }
    });
}

/// Which way a measure going up goes.
#[derive(Clone, Copy)]
enum Up {
    Better,
    Worse,
    Neutral,
}

/// The four figures at the page's newest commit, each with its change since
/// the page's oldest.
fn tiles(ui: &mut egui::Ui, stats: &Stats) {
    let (Some(base), Some(now)) = (stats.snapshots.first(), stats.snapshots.last()) else {
        return;
    };
    let since = format!("since {}", base.short);
    let cycles = &stats.cycles;
    let cycle = match (cycles.percentile(0.5), cycles.percentile(0.85)) {
        (Some(median), Some(p85)) => (
            stats::span(median),
            format!("p85 {} · {} issues", stats::span(p85), cycles.cycles.len()),
            None,
        ),
        _ => (
            "–".to_string(),
            "no issue closed on this page".to_string(),
            None,
        ),
    };
    let complexity = (
        Measure::Complexity.show(now.complexity_per_kloc()),
        "per 1,000 code lines".to_string(),
        delta(
            now.complexity_per_kloc() - base.complexity_per_kloc(),
            0,
            Up::Worse,
        ),
    );
    let tests = (
        Measure::Tests.show(now.test_share() * 100.0),
        format!("{} tests · test lines, not coverage", thousands(now.tests)),
        delta(
            (now.test_share() - base.test_share()) * 100.0,
            1,
            Up::Better,
        ),
    );
    let code = (
        thousands(now.code),
        languages(now),
        delta(now.code as f32 - base.code as f32, 0, Up::Neutral),
    );
    ui.columns(Measure::ALL.len(), |columns| {
        for ((ui, measure), (value, detail, change)) in columns
            .iter_mut()
            .zip(Measure::ALL)
            .zip([cycle, complexity, tests, code])
        {
            block(ui, |ui| {
                let p = theme::palette(ui);
                block_label(ui, measure.name());
                ui.label(
                    RichText::new(value)
                        .font(egui::FontId::new(24.0, theme::ui_title().family))
                        .color(p.ink),
                );
                let (text, color) = change_line(ui, change, &since);
                ui.label(RichText::new(text).font(theme::meta()).color(color));
                ui.add(
                    egui::Label::new(RichText::new(detail).font(theme::meta()).color(p.ink_muted))
                        .truncate(),
                );
            });
        }
    });
}

/// The two main languages' shares of the code.
fn languages(s: &Snapshot) -> String {
    let share = |lines: usize| (lines as f32 / s.code.max(1) as f32 * 100.0).round();
    s.languages
        .iter()
        .take(2)
        .map(|(name, lines)| format!("{name} {:.0}%", share(*lines)))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A change and how it reads, or `None` when nothing changed.
fn delta(change: f32, decimals: usize, up: Up) -> Option<(String, Up, bool)> {
    let shown = format!("{change:+.decimals$}");
    let zero = format!("{:+.decimals$}", 0.0);
    (shown != zero && shown != format!("-{}", &zero[1..])).then_some((shown, up, change > 0.0))
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
fn trends(ui: &mut egui::Ui, stats: &Stats, hovered: Option<usize>) -> Option<usize> {
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
                let limit = (measure == Measure::Complexity).then_some(COMPLEXITY_LIMIT);
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

/// One reason to plan a refactor, on or off, with its numbers.
#[derive(Debug, Clone, PartialEq)]
struct Signal {
    name: &'static str,
    on: bool,
    detail: String,
}

/// The four signals of the Refactor watch, from the page's oldest commit to
/// its newest.
fn signals(stats: &Stats) -> Vec<Signal> {
    let (Some(base), Some(now)) = (stats.snapshots.first(), stats.snapshots.last()) else {
        return Vec::new();
    };
    let (was, is) = (base.complexity_per_kloc(), now.complexity_per_kloc());
    let growth = if was > 0.0 { is / was - 1.0 } else { 0.0 };
    let slide = (base.test_share() - now.test_share()) * 100.0;
    let hot = stats.files.iter().filter(|f| f.is_hotspot()).count();
    let (older, newer) = cycle_halves(stats);
    let drift = match (older.percentile(0.5), newer.percentile(0.5)) {
        (Some(before), Some(after)) => Some((before, after)),
        _ => None,
    };
    vec![
        Signal {
            name: "Complexity grows faster than code",
            on: growth > COMPLEXITY_GROWTH || is > COMPLEXITY_LIMIT,
            detail: format!(
                "{was:.0} → {is:.0} per kLOC ({:+.0}%), limit {COMPLEXITY_LIMIT:.0}",
                growth * 100.0
            ),
        },
        Signal {
            name: "Tests are sliding",
            on: slide >= TESTS_SLIDE,
            detail: format!(
                "{:.1}% → {:.1}% of the code is tests",
                base.test_share() * 100.0,
                now.test_share() * 100.0
            ),
        },
        Signal {
            name: "Hotspots",
            on: hot > 0,
            detail: format!(
                "{hot} files with complexity ≥ {} and ≥ {} changes",
                stats::HOTSPOT_COMPLEXITY,
                stats::HOTSPOT_CHANGES
            ),
        },
        Signal {
            name: "Cycle time drifts",
            on: drift.is_some_and(|(before, after)| after as f32 > before as f32 * CYCLE_DRIFT),
            detail: drift.map_or(
                "too few issues closed to tell".to_string(),
                |(before, after)| {
                    format!(
                        "median {} on the newer half, {} before",
                        stats::span(after),
                        stats::span(before)
                    )
                },
            ),
        },
    ]
}

/// The page's closed issues split at its middle commit: older, newer.
fn cycle_halves(stats: &Stats) -> (Cycles, Cycles) {
    let middle = stats.snapshots.len() / 2;
    let mut halves = (Cycles::default(), Cycles::default());
    for cycle in &stats.cycles.cycles {
        let at = stats.snapshots.iter().position(|s| s.short == cycle.short);
        match at {
            Some(at) if at < middle => halves.0.cycles.push(cycle.clone()),
            Some(_) => halves.1.cycles.push(cycle.clone()),
            None => {}
        }
    }
    halves
}

/// The files a refactor should start from: the hot spots, or else the most
/// complex and changed.
fn culprits(files: &[FileStat]) -> Vec<&FileStat> {
    let hot: Vec<&FileStat> = files.iter().filter(|f| f.is_hotspot()).collect();
    if !hot.is_empty() {
        return hot;
    }
    let mut ranked: Vec<&FileStat> = files.iter().filter(|f| f.changes > 0).collect();
    ranked.sort_by_key(|f| std::cmp::Reverse(f.complexity * f.changes));
    ranked.truncate(3);
    ranked
}

/// The refactor issue the watch drafts: its title and description.
fn refactor_draft(stats: &Stats, signals: &[Signal]) -> (String, String) {
    let on: Vec<&Signal> = signals.iter().filter(|s| s.on).collect();
    let mut body = format!(
        "Project stats turned on {} of {} refactor signals over the last {} commits:\n\n",
        on.len(),
        signals.len(),
        stats.snapshots.len()
    );
    for signal in &on {
        body.push_str(&format!("- {}: {}\n", signal.name, signal.detail));
    }
    let files = culprits(&stats.files);
    if !files.is_empty() {
        body.push_str("\n## Files\n\n");
        for f in files {
            body.push_str(&format!(
                "- `{}`: {} lines, complexity {}, {} changes\n",
                f.path,
                thousands(f.code),
                f.complexity,
                f.changes
            ));
        }
    }
    ("[refactor] Plan a refactor".to_string(), body)
}

/// The verdict, the four signals behind it, and the draft it offers.
fn watch(ui: &mut egui::Ui, stats: &Stats) -> Option<StatsAsk> {
    let signals = signals(stats);
    let on = signals.iter().filter(|s| s.on).count();
    block(ui, |ui| {
        let p = theme::palette(ui);
        block_label(ui, "Refactor watch");
        let mut ask = None;
        Frame::new()
            .fill(p.surface_sunken)
            .corner_radius(CornerRadius::same(theme::RADIUS_SM))
            .inner_margin(Margin::same(theme::SPACE_3 as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if on < 2 {
                    ui.label(
                        RichText::new("No refactor needed now")
                            .font(theme::ui_strong())
                            .color(p.ink),
                    );
                    return;
                }
                ui.label(
                    RichText::new("Time to plan a refactor.")
                        .font(theme::ui_strong())
                        .color(p.ink),
                );
                let files = culprits(&stats.files).len();
                ui.label(
                    RichText::new(format!(
                        "{on} of {} signals are on, mostly from {files} files.",
                        signals.len()
                    ))
                    .font(theme::ui_small())
                    .color(p.ink),
                );
                if widgets::button(
                    ui,
                    ButtonKind::Primary,
                    None,
                    "Draft a refactor issue",
                    None,
                )
                .clicked()
                {
                    let (title, body) = refactor_draft(stats, &signals);
                    ask = Some(StatsAsk::DraftRefactor(title, body));
                }
            });
        for signal in &signals {
            signal_row(ui, signal);
        }
        ask
    })
}

fn signal_row(ui: &mut egui::Ui, signal: &Signal) {
    let p = theme::palette(ui);
    let (icon, word, color) = if signal.on {
        (Icon::Warning, "On", p.status_doing)
    } else {
        (Icon::Check, "Off", p.status_done)
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(theme::ICON_SIZE), Sense::hover());
        theme::paint_icon(ui.painter(), icon, rect, color);
        ui.label(RichText::new(word).font(theme::meta()).color(color));
        ui.add(
            egui::Label::new(
                RichText::new(signal.name)
                    .font(theme::ui_small())
                    .color(p.ink),
            )
            .wrap(),
        );
    });
    ui.add(
        egui::Label::new(
            RichText::new(&signal.detail)
                .font(theme::meta())
                .color(p.ink_muted),
        )
        .wrap(),
    );
}

/// The files at the page's newest commit, biggest first unless sorted
/// otherwise.
fn files_table(ui: &mut egui::Ui, files: &[FileStat], sort: &mut FileSort, all: &mut bool) {
    block(ui, |ui| {
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            block_label(ui, "Biggest files");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut options = FileSort::ALL;
                options.reverse();
                if let Some(picked) = widgets::segmented(ui, *sort, &options) {
                    *sort = picked;
                }
            });
        });
        let mut rows: Vec<&FileStat> = files.iter().collect();
        match sort {
            FileSort::Lines => {}
            FileSort::Complexity => rows.sort_by_key(|f| std::cmp::Reverse(f.complexity)),
            FileSort::Changes => rows.sort_by_key(|f| std::cmp::Reverse(f.changes)),
        }
        let largest = files.first().map_or(1, |f| f.code.max(1));
        let cells = ["Lines", "Complexity", "Changes", "Hotspot"];
        file_row(ui, "File", &cells.map(str::to_string), None, p.ink_muted);
        widgets::rule(ui);
        let shown = if *all { rows.len() } else { FILES_SHOWN };
        for f in rows.iter().take(shown) {
            let hot = if f.is_hotspot() { "hotspot" } else { "–" };
            let cells = [
                thousands(f.code),
                f.complexity.to_string(),
                f.changes.to_string(),
                hot.to_string(),
            ];
            let bar = f.code as f32 / largest as f32;
            file_row(ui, &f.path, &cells, Some(bar), p.ink);
        }
        if rows.len() > FILES_SHOWN && !*all {
            let label = format!("Show all {}", rows.len());
            if widgets::button(ui, ButtonKind::Ghost, None, &label, None).clicked() {
                *all = true;
            }
        }
    });
}

/// One row of the files table, painted on fixed columns: the path, then its
/// four figures; `bar` scales the lines to the largest file.
fn file_row(ui: &mut egui::Ui, path: &str, cells: &[String; 4], bar: Option<f32>, ink: Color32) {
    const WIDTHS: [f32; 4] = [130.0, 80.0, 70.0, 90.0];
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, path));
    let painter = ui.painter_at(rect);
    let font = theme::meta();
    let mut x = rect.right() - WIDTHS.iter().sum::<f32>();
    let (dir, name) = path.rsplit_once('/').map_or(("", path), |(d, n)| (d, n));
    let mut text = egui::text::LayoutJob::default();
    if !dir.is_empty() {
        text.append(
            &format!("{dir}/"),
            0.0,
            egui::TextFormat::simple(font.clone(), p.ink_faint),
        );
    }
    text.append(name, 0.0, egui::TextFormat::simple(font.clone(), ink));
    text.wrap = egui::text::TextWrapping::truncate_at_width(x - rect.left() - theme::SPACE_3);
    let galley = ui.fonts_mut(|f| f.layout_job(text));
    painter.galley(
        Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    for (at, (cell, width)) in cells.iter().zip(WIDTHS).enumerate() {
        let hot = at == 3 && cell == "hotspot";
        let mut left = x;
        if hot {
            let icon =
                Rect::from_center_size(Pos2::new(x + 6.0, rect.center().y), Vec2::splat(12.0));
            theme::paint_icon(&painter, Icon::Flame, icon, p.status_doing);
            left += 16.0;
        }
        let color = if hot { p.status_doing } else { ink };
        let shown = painter.text(
            Pos2::new(left, rect.center().y),
            egui::Align2::LEFT_CENTER,
            cell,
            font.clone(),
            color,
        );
        if let (0, Some(share)) = (at, bar) {
            let track = Rect::from_min_size(
                Pos2::new(shown.right() + theme::SPACE_3, rect.center().y - 3.0),
                Vec2::new(64.0 * share, 6.0),
            );
            painter.rect_filled(track, CornerRadius::same(2), p.accent);
        }
        x += width;
    }
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
                         coverage; cycle time runs from an issue's first build run or \
                         commit to the commit that closes it.{kept}"
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

#[cfg(test)]
mod tests {
    use super::*;
    use iboard::stats::Cycle;

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

    fn file(path: &str, complexity: usize, changes: usize) -> FileStat {
        FileStat {
            path: path.into(),
            changes,
            code: 400,
            complexity,
        }
    }

    #[test]
    fn two_signals_draft_a_refactor_naming_the_hot_spots() {
        let stats = Stats {
            snapshots: vec![
                snapshot("a", 1000, 100, 300),
                snapshot("b", 1000, 110, 280),
                snapshot("c", 1000, 120, 260),
                snapshot("d", 1000, 140, 250),
            ],
            files: vec![file("src/app.rs", 90, 12), file("src/lib.rs", 10, 20)],
            cycles: Cycles {
                cycles: vec![cycle("a", 1), cycle("d", 4)],
                unknown: 0,
            },
            ..Stats::default()
        };
        let on: Vec<&str> = signals(&stats)
            .iter()
            .filter(|s| s.on)
            .map(|s| s.name)
            .collect();
        assert_eq!(
            on,
            [
                "Complexity grows faster than code",
                "Tests are sliding",
                "Hotspots",
                "Cycle time drifts"
            ]
        );
        let (title, body) = refactor_draft(&stats, &signals(&stats));
        assert_eq!(title, "[refactor] Plan a refactor");
        assert!(
            body.contains("- `src/app.rs`: 400 lines, complexity 90, 12 changes"),
            "{body}"
        );
        assert!(!body.contains("src/lib.rs"), "{body}");
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
