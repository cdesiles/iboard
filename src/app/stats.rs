//! Project stats, from Tools › Project Stats…: what a page of commits says
//! about the cost of changing the project. Four figures, their trends on one
//! shared axis, a Refactor watch, the biggest files and the Agenda (see
//! `design/components/StatsScreen`).

use std::sync::Arc;

use iboard::agenda::{Agenda, Range};
use iboard::format::thousands;
use iboard::stats::{self, Limits, Progress, Snapshot, Stats};

use super::agenda;
use super::compose::TitleStage;
use super::*;

mod files;
mod tiles;
mod trends;
mod watch;

use files::files_table;
use tiles::tiles;
use trends::trends;
use watch::watch;

/// A trend's chart height, and the width of its name and value beside it.
const TREND_H: f32 = 64.0;
const TREND_LABEL_W: f32 = 150.0;
/// Narrower than this, the Refactor watch goes under the trends.
const SIDE_BY_SIDE: f32 = 760.0;
/// The Thinking mark and clock wait this long.
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
    fn new(dir: &Path, root: &Path, page: usize, range: Range) -> Self {
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
    /// Stats measure one history: a group's board has none to show.
    pub(super) fn open_stats(&mut self) {
        if self.showing_group() {
            let notice = "Stats measure one project: open it from the project menu";
            self.session.notice = Some((notice.to_string(), Instant::now()));
            return;
        }
        if self.session.stats.is_none() {
            self.session.stats = Some(StatsPanel::new(
                &self.located.project_dir,
                &self.located.root,
                0,
                self.session.agenda_range,
            ));
        }
    }

    /// Takes the measures once they are in.
    pub(super) fn poll_stats(&mut self) -> Option<Duration> {
        let panel = self.session.stats.as_mut()?;
        let (job, _) = panel.run.as_ref()?;
        match job.answer() {
            None => return Some(job::POLL_EVERY),
            Some(Ok(stats)) => {
                panel.stats = Some(stats);
                panel.run = None;
            }
            Some(Err(err)) => {
                panel.error = Some(err);
                panel.run = None;
            }
        }
        None
    }

    /// The panel owns the keyboard while up; the Agenda takes its keys.
    pub(super) fn stats_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.session.stats = None;
            return;
        }
        if self.session.stats.as_ref().is_some_and(|p| !p.is_loading()) {
            self.agenda_keys(ctx);
        }
    }

    /// Project stats in place of the board, under the header bar: its title
    /// row, the page of commits it reads, and where it is kept.
    pub(super) fn stats_screen(&mut self, ui: &mut egui::Ui) {
        let Some(panel) = self.session.stats.as_mut() else {
            return;
        };
        let p = theme::palette(ui);
        let mut ask = None;
        ui.style_mut().interaction.selectable_labels = false;
        let mut page = None;
        if widgets::modal_header(ui, "Project stats", |ui| page = header(ui, panel)) {
            ask = Some(StatsAsk::Close);
        }
        if let Some(page) = page {
            ask = Some(StatsAsk::Page(page));
        }
        widgets::rule(ui);
        egui::Panel::bottom("stats-footer")
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| {
                widgets::rule(ui);
                stats_footer(ui, panel.stats.as_ref());
            });
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| {
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .content_margin(theme::scroll_margin(theme::SPACE_5, theme::SPACE_5))
                        .auto_shrink([false, false]),
                    true,
                    |ui| {
                        if let Some(draft) = stats_body(ui, panel) {
                            ask = Some(draft);
                        }
                    },
                );
            });
        self.session.agenda_range = panel.agenda.range;
        let range = panel.agenda.range;
        match ask {
            Some(StatsAsk::Close) => self.session.stats = None,
            Some(StatsAsk::Page(page)) => {
                let (project, root) = (&self.located.project_dir, &self.located.root);
                self.session.stats = Some(StatsPanel::new(project, root, page, range));
            }
            Some(StatsAsk::DraftRefactor(title, description)) => {
                self.session.stats = None;
                self.draft_refactor(title, description);
            }
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
        .is_none_or(|(job, _)| job.elapsed() >= theme::SKELETON_AFTER);
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
    let limits = Limits::default();
    let mut draft = None;
    let mut hovered = None;
    split(
        ui,
        |ui| hovered = trends(ui, stats, &limits, panel.hovered),
        |ui| draft = watch(ui, stats, &limits),
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
/// [`theme::SKELETON_AFTER`].
fn placeholder(ui: &mut egui::Ui, share: f32, height: f32, pulse: bool) {
    if pulse {
        widgets::skeleton_bar(ui, share, height);
    } else {
        ui.allocate_exact_size(
            Vec2::new(ui.available_width() * share, height),
            Sense::hover(),
        );
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
