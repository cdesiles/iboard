//! Project stats, from Tools › Project Stats…: what a page of commits says
//! about the cost of changing the project. Four figures, their trends on one
//! shared axis, a Refactor watch, the biggest files and the Agenda (see
//! `design/components/StatsScreen`).

use std::sync::Arc;

use iboard::agenda::{Agenda, Range};
use iboard::format::{display_path, thousands};
use iboard::stats::{self, Limits, Measures, Snapshot, Source, Stats};

use super::agenda;
use super::compose::Composer;
use super::{
    Align, ButtonKind, Color32, CornerRadius, Duration, Frame, IboardApp, Icon, Instant, Job, Key,
    Layout, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2, job, settings, theme, widgets,
};

pub(super) mod blocks;
mod files;
mod releases;
mod tiles;
mod trends;
mod watch;

use blocks::{Blocks, Part, Phase};
use files::{FileHistory, files_table, history_window};
use tiles::tile;
use trends::trends;
use watch::watch;

/// A trend's chart height, and the width of its name and value beside it.
const TREND_H: f32 = 68.0;
const TREND_LABEL_W: f32 = 150.0;
/// Narrower than this, the Refactor watch goes under the trends.
const SIDE_BY_SIDE: f32 = 760.0;
/// The Trends and Refactor watch blocks' height, so neither moves as it fills.
const TRENDS_H: f32 = 400.0;

/// One of the four measures, as a tile and as a trend.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Measure {
    Cycle,
    Complexity,
    Tests,
    Code,
}

impl Measure {
    pub(super) const ALL: [Measure; 4] = [
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

/// The stats panel: a page of history, each block reading it on its own.
pub(super) struct StatsPanel {
    page: usize,
    pub(super) blocks: Blocks,
    /// The commit under the trends' shared crosshair, an index into the
    /// snapshots.
    pub(super) hovered: Option<usize>,
    sort: FileSort,
    all_files: bool,
    /// The history popup of a row clicked in the files table.
    pub(super) history: Option<FileHistory>,
    pub(super) agenda: Agenda,
    /// The project's name, as the Agenda's day panel says it.
    project: String,
    /// The Releases checkbox of the Trends.
    pub(super) show_releases: bool,
}

impl StatsPanel {
    fn new(
        source: Source,
        project: String,
        page: usize,
        range: Range,
        show_releases: bool,
    ) -> Self {
        Self {
            page,
            blocks: Blocks::new(source),
            hovered: None,
            sort: FileSort::Lines,
            all_files: false,
            history: None,
            agenda: Agenda::new(range),
            project,
            show_releases,
        }
    }

    /// Reads every block started so far again; old numbers stay, dimmed.
    pub(super) fn refresh(&mut self) {
        let source = self.blocks.source().at_page(self.page);
        self.blocks.refresh(source);
    }

    pub(super) fn is_loading(&self) -> bool {
        self.blocks.is_loading()
    }

    /// The commits as the Lines of code trend measured them.
    #[cfg(test)]
    pub(super) fn snapshots(&self) -> &[Snapshot] {
        self.blocks
            .loaded(Part::Trend(Measure::Code))
            .map_or(&[], |s| &s.snapshots)
    }
}

/// What the panel asked for this frame.
enum StatsAsk {
    Page(usize),
    Refresh,
    DraftRefactor(String, String),
}

impl IboardApp {
    /// Opens the stats panel on the project, measuring its newest commits.
    /// Stats measure one history: a group's board has none to show.
    pub(super) fn open_stats(&mut self) {
        if self.welcoming() {
            return;
        }
        self.project_menu = None;
        if self.showing_group() {
            let notice = "Stats measure one project: open it from the project menu";
            self.session.notice = Some((notice.to_string(), Instant::now()));
            return;
        }
        if self.session.stats.is_none() {
            let dir = &self.located.project_dir;
            let measures = Arc::new(Measures::new(dir));
            let releases = &self.prefs.releases;
            let source =
                Source::new(dir, &self.located.root, 0, measures).with_release_tags(&releases.tags);
            self.session.stats = Some(StatsPanel::new(
                source,
                self.board.name(),
                0,
                self.prefs.agenda_range,
                releases.show,
            ));
        }
    }

    /// Takes each block's data once it is in.
    pub(super) fn poll_stats(&mut self) -> Option<Duration> {
        let panel = self.session.stats.as_mut()?;
        let reading = panel.blocks.poll();
        let history = panel.history.as_mut().is_some_and(FileHistory::poll);
        (reading || history).then_some(job::POLL_EVERY)
    }

    /// The panel owns the keyboard while up: `⌘R` refreshes it and the Agenda
    /// takes its keys once it has days. A file's history over it takes them all.
    pub(super) fn stats_keys(&mut self, ctx: &egui::Context) {
        if self
            .session
            .stats
            .as_ref()
            .is_some_and(|panel| panel.history.is_some())
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.session.stats = None;
            return;
        }
        let Some(panel) = self.session.stats.as_mut() else {
            return;
        };
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::R)) {
            panel.refresh();
        }
        if panel.blocks.loaded(Part::Agenda).is_some() {
            self.agenda_keys(ctx);
        }
    }

    /// The header bar's trail while the stats are up: `project / Stats`, the
    /// project leading back to the board.
    pub(super) fn stats_crumbs(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        let project = self.board.name();
        let back = widgets::crumb(ui, &project, theme::ui_body())
            .on_hover_text("Back to the board (esc)")
            .clicked();
        ui.label(RichText::new("/").color(p.ink_faint));
        ui.label(RichText::new("Stats").font(theme::ui_strong()).color(p.ink));
        if back {
            self.session.stats = None;
        }
    }

    /// The header bar's controls while the stats are up, right to left.
    pub(super) fn stats_controls(&mut self, ui: &mut egui::Ui) {
        let Some(panel) = self.session.stats.as_ref() else {
            return;
        };
        let ask = header(ui, panel);
        if let Some(ask) = ask {
            self.stats_ask(ask);
        }
    }

    /// Project stats in place of the board, under the header bar: its blocks,
    /// and where it is kept.
    pub(super) fn stats_screen(&mut self, ui: &mut egui::Ui) {
        let Some(panel) = self.session.stats.as_mut() else {
            return;
        };
        let p = theme::palette(ui);
        let mut ask = None;
        ui.style_mut().interaction.selectable_labels = false;
        widgets::rule(ui);
        egui::Panel::bottom("stats-footer")
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| {
                widgets::rule(ui);
                stats_footer(ui, panel.blocks.loaded(Part::Head));
            });
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| {
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .content_margin(theme::scroll_margin(theme::SPACE_5, theme::SPACE_5))
                        .auto_shrink([false, false]),
                    |ui| {
                        if let Some(draft) = stats_body(ui, panel) {
                            ask = Some(draft);
                        }
                    },
                );
            });
        if panel
            .history
            .as_ref()
            .is_some_and(|history| history_window(ui, history))
        {
            panel.history = None;
        }
        let show_releases = panel.show_releases;
        let range = panel.agenda.range;
        if range != self.prefs.agenda_range {
            self.prefs.agenda_range = range;
            self.remember_section(settings::Section::new(
                "agenda",
                &iboard::agenda::range_section(range),
            ));
        }
        if show_releases != self.prefs.releases.show {
            self.prefs.releases.show = show_releases;
            self.remember_section(settings::Section::new(
                "stats",
                &self.prefs.releases.section(),
            ));
        }
        if let Some(ask) = ask {
            self.stats_ask(ask);
        }
    }

    fn stats_ask(&mut self, ask: StatsAsk) {
        let Some(panel) = self.session.stats.as_mut() else {
            return;
        };
        match ask {
            StatsAsk::Page(page) => {
                let range = panel.agenda.range;
                let source = panel.blocks.source().at_page(page);
                let project = std::mem::take(&mut panel.project);
                *panel = StatsPanel::new(source, project, page, range, panel.show_releases);
            }
            StatsAsk::Refresh => panel.refresh(),
            StatsAsk::DraftRefactor(title, description) => {
                self.session.stats = None;
                self.draft_refactor(title, description);
            }
        }
    }

    /// Opens the composer in Todo with the refactor issue drafted; nothing is
    /// written until it is confirmed.
    fn draft_refactor(&mut self, title: String, description: String) {
        let composer = self.session.composer.get_or_insert_with(Composer::default);
        if composer.is_empty() {
            composer.title = title;
            composer.description = description;
        }
        self.open_composer();
    }
}

/// Refresh rightmost, the page's neighbours, then the page shown or the
/// reading under way, in the header bar's right-to-left layout; returns what
/// was asked for.
fn header(ui: &mut egui::Ui, panel: &StatsPanel) -> Option<StatsAsk> {
    let p = theme::palette(ui);
    let head = panel.blocks.loaded(Part::Head);
    let (done, total) = panel.blocks.source().progress.get();
    let first = panel.page * stats::PAGE;
    let text = match head {
        _ if panel.is_loading() && done < total => {
            format!("Reading commit {}/{total}…", done + 1)
        }
        None if panel.is_loading() => "Listing commits…".to_string(),
        None => format!("from commit {}", first + 1),
        Some(head) => format!(
            "commits {}–{} before HEAD",
            first + 1,
            first + head.snapshots.len()
        ),
    };
    let mut ask = None;
    let refresh = Some(Icon::Refresh);
    if widgets::button(ui, ButtonKind::Ghost, refresh, "Refresh", Some("⌘R")).clicked() {
        ask = Some(StatsAsk::Refresh);
    }
    ui.add_enabled_ui(head.is_some_and(|s| s.older), |ui| {
        let label = format!("Older {}", stats::PAGE);
        if widgets::button(ui, ButtonKind::Ghost, Some(Icon::Back), &label, None).clicked() {
            ask = Some(StatsAsk::Page(panel.page + 1));
        }
    });
    ui.add_enabled_ui(panel.page > 0, |ui| {
        if widgets::button(ui, ButtonKind::Ghost, None, "Newer", None).clicked() {
            ask = Some(StatsAsk::Page(panel.page - 1));
        }
    });
    ui.label(RichText::new(text).font(theme::meta()).color(p.ink_muted));
    ask
}

/// The panel's body: tiles, trends and watch, files, Agenda, each on its own.
fn stats_body(ui: &mut egui::Ui, panel: &mut StatsPanel) -> Option<StatsAsk> {
    ui.spacing_mut().item_spacing.y = theme::SPACE_5;
    if panel.is_loading() {
        blocks::announce_loading(ui);
    }
    ui.columns(Measure::ALL.len(), |columns| {
        for (ui, measure) in columns.iter_mut().zip(Measure::ALL) {
            tile(ui, &mut panel.blocks, measure);
        }
    });
    let limits = Limits::default();
    let mut draft = None;
    let mut hovered = None;
    let shown = panel.hovered;
    let show_releases = &mut panel.show_releases;
    split(
        ui,
        &mut panel.blocks,
        |ui, blocks| hovered = trends(ui, blocks, &limits, shown, show_releases),
        |ui, blocks| draft = watch(ui, blocks, &limits),
    );
    if hovered != panel.hovered {
        panel.hovered = hovered;
        ui.ctx().request_repaint();
    }
    if let Some(path) = files_table(ui, &mut panel.blocks, &mut panel.sort, &mut panel.all_files) {
        panel.history = Some(FileHistory::open(panel.blocks.source().dir(), path));
    }
    agenda_block(ui, panel);
    draft
}

/// The Agenda, under its own frame and on its own job.
fn agenda_block(ui: &mut egui::Ui, panel: &mut StatsPanel) {
    let phase = panel.blocks.phase(Part::Agenda);
    let clock = panel.blocks.clock(Part::Agenda);
    let show = match &phase {
        Phase::Waiting { pulse } => agenda::Show::Waiting { pulse: *pulse },
        Phase::Failed(error) => agenda::Show::Failed(error),
        Phase::Loaded { stats, .. } => agenda::Show::Days(&stats.agenda),
    };
    let scope = ui.scope(|ui| {
        blocks::dim_if_stale(ui, &phase);
        agenda::agenda_ui(ui, &mut panel.agenda, &panel.project, show, clock)
    });
    if scope.inner {
        panel.blocks.start(Part::Agenda);
    }
    panel.blocks.settle(ui, Part::Agenda, &scope.response);
}

/// Three quarters and one quarter side by side, or one under the other on a
/// narrow window, both drawing with `state`.
fn split<S>(
    ui: &mut egui::Ui,
    state: &mut S,
    wide: impl FnOnce(&mut egui::Ui, &mut S),
    narrow: impl FnOnce(&mut egui::Ui, &mut S),
) {
    let width = ui.available_width();
    if width < SIDE_BY_SIDE {
        wide(ui, state);
        narrow(ui, state);
        return;
    }
    let gap = theme::SPACE_4;
    let three = (width - gap) * 0.75;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        column(ui, three, |ui| wide(ui, state));
        column(ui, width - gap - three, |ui| narrow(ui, state));
    });
}

fn column(ui: &mut egui::Ui, width: f32, draw: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        draw(ui);
    });
}

/// A `surface` block with a `line` edge, as tiles and panels are.
fn block<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_soft))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(Vec2::new(14.0, theme::SPACE_4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme::SPACE_2 + 2.0;
            add(ui)
        })
        .inner
}

/// An uppercase `ui-label`, as tiles are headed.
fn block_label(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(
        RichText::new(text.to_uppercase())
            .font(theme::ui_label())
            .color(p.ink_muted),
    );
}

/// A panel's title in `ui-title`, as the Agenda has.
fn panel_title(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(RichText::new(text).font(theme::ui_title()).color(p.ink));
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

fn stats_footer(ui: &mut egui::Ui, stats: Option<&Stats>) {
    let p = theme::palette(ui);
    let kept = stats
        .and_then(|s| s.cache.as_ref())
        .map(|file| format!(" Kept in {}.", display_path(file)))
        .unwrap_or_default();
    Frame::new().inner_margin(theme::BAND_PAD).show(ui, |ui| {
        ui.add(
            egui::Label::new(
                RichText::new(format!(
                    "Read from the source at each commit: complexity is McCabe's, \
                     estimated; test share is test lines over code lines, not run \
                     coverage; cycle time runs from an issue entering doing to its \
                     entering done, or from its first build run or commit to the \
                     commit that closes it; a release line sits on the commit its tag \
                     falls on.{kept}"
                ))
                .font(theme::meta())
                .color(p.ink_muted),
            )
            .wrap(),
        );
    });
}
