//! Each block of Project stats on its own: a job started once the block is on
//! screen, its own timeout, its placeholder at the block's final size, and its
//! failure with Retry (see `design/components/Skeleton`).

use std::collections::HashMap;
use std::sync::Arc;

use crate::stats::{Need, Source, Stats};

use super::{
    Align, ButtonKind, CornerRadius, Duration, Icon, Job, Layout, Measure, Pos2, Rect, RichText,
    Sense, Stroke, Vec2, block, block_label, job, panel_title, placeholder, theme, widgets,
};

/// How long one block's read may take. A timed-out commit read stays cached,
/// so Retry resumes it.
const TIMEOUT: Duration = Duration::from_secs(60);
/// The opacity of old numbers while a refresh reads the new ones.
const STALE_OPACITY: f32 = 0.6;
/// Between a panel's title and its caption.
const PANEL_HEAD_GAP: f32 = 10.0;
/// A row of the list placeholder.
const LIST_ROW_H: f32 = 28.0;
/// The name bar's share of each row of the list placeholder.
const LIST_ROWS: [f32; 5] = [0.62, 0.44, 0.54, 0.36, 0.48];

/// A block with a job of its own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(in crate::shell::app) enum Part {
    /// The header's page of commits, read as soon as the screen opens.
    Head,
    Tile(Measure),
    Trend(Measure),
    Watch,
    Files,
    Agenda,
    Releases,
    /// What the agents cost over the trends' days, from the usage log.
    Cost,
}

impl Part {
    fn needs(self) -> &'static [Need] {
        match self {
            Part::Head => &[Need::Commits],
            Part::Tile(Measure::Cycle) | Part::Trend(Measure::Cycle) => {
                &[Need::Commits, Need::Cycles]
            }
            Part::Tile(_) | Part::Trend(_) => &[Need::Measures],
            Part::Watch => &[Need::Measures, Need::Cycles, Need::Files],
            Part::Files => &[Need::Files],
            Part::Agenda => &[Need::Agenda],
            Part::Releases => &[Need::Commits, Need::Releases],
            Part::Cost => &[Need::Cost],
        }
    }

    /// What a failure says it couldn't read.
    fn what(self) -> &'static str {
        match self {
            Part::Head | Part::Agenda => "git history",
            Part::Tile(Measure::Cycle) | Part::Trend(Measure::Cycle) => "cycle times",
            Part::Tile(_) | Part::Trend(_) => "commit measures",
            Part::Watch => "refactor signals",
            Part::Files => "biggest files",
            Part::Releases => "release tags",
            Part::Cost => "the usage log",
        }
    }
}

/// Where a block stands, as it is drawn.
pub(in crate::shell::app) enum Phase {
    /// No data yet; `pulse` once the placeholder may show.
    Waiting {
        pulse: bool,
    },
    Failed(String),
    /// `stale` while a refresh reads newer numbers.
    Loaded {
        stats: Arc<Stats>,
        stale: bool,
    },
}

#[derive(Default)]
struct Slot {
    run: Option<Job<Result<Stats, String>>>,
    stats: Option<Arc<Stats>>,
    error: Option<String>,
}

/// The screen's blocks and the page they read.
pub(in crate::shell::app) struct Blocks {
    source: Arc<Source>,
    slots: HashMap<Part, Slot>,
    pub(in crate::shell::app) timeout: Duration,
}

impl Blocks {
    pub(super) fn new(source: Source) -> Self {
        let mut blocks = Self {
            source: Arc::new(source),
            slots: HashMap::new(),
            timeout: TIMEOUT,
        };
        blocks.start(Part::Head);
        blocks
    }

    pub(super) fn source(&self) -> &Source {
        &self.source
    }

    /// Reads `part`, replacing any read under way; its numbers stay until the
    /// new ones land.
    pub(super) fn start(&mut self, part: Part) {
        let source = Arc::clone(&self.source);
        let slot = self.slots.entry(part).or_default();
        slot.error = None;
        slot.run = Some(Job::spawn(move |cancel| source.read(part.needs(), cancel)));
    }

    /// Reads every block started so far from `source`.
    pub(super) fn refresh(&mut self, source: Source) {
        self.source = Arc::new(source);
        let started: Vec<Part> = self.slots.keys().copied().collect();
        for part in started {
            self.start(part);
        }
    }

    /// Takes what landed and fails what took too long; true while any block
    /// still reads.
    pub(super) fn poll(&mut self) -> bool {
        let mut reading = false;
        for slot in self.slots.values_mut() {
            let Some(job) = &slot.run else {
                continue;
            };
            let landed = job.answer().or_else(|| {
                (job.elapsed() >= self.timeout)
                    .then(|| Err(format!("took longer than {}s", self.timeout.as_secs())))
            });
            match landed {
                None => reading = true,
                Some(Ok(stats)) => {
                    slot.stats = Some(Arc::new(stats));
                    slot.run = None;
                }
                Some(Err(err)) => {
                    slot.stats = None;
                    slot.error = Some(err);
                    slot.run = None;
                }
            }
        }
        reading
    }

    pub(in crate::shell::app) fn is_loading(&self) -> bool {
        self.slots.values().any(|slot| slot.run.is_some())
    }

    /// True once `part` has been asked to read.
    pub(in crate::shell::app) fn started(&self, part: Part) -> bool {
        self.slots.contains_key(&part)
    }

    pub(in crate::shell::app) fn phase(&self, part: Part) -> Phase {
        let Some(slot) = self.slots.get(&part) else {
            return Phase::Waiting { pulse: false };
        };
        match (&slot.stats, &slot.error, &slot.run) {
            (Some(stats), _, run) => Phase::Loaded {
                stats: Arc::clone(stats),
                stale: run.is_some(),
            },
            (None, Some(error), _) => Phase::Failed(error.clone()),
            (None, None, run) => Phase::Waiting {
                pulse: run
                    .as_ref()
                    .is_some_and(|job| job.elapsed() >= theme::SKELETON_AFTER),
            },
        }
    }

    /// `part`'s data, new or stale.
    pub(in crate::shell::app) fn loaded(&self, part: Part) -> Option<&Stats> {
        self.slots.get(&part)?.stats.as_deref()
    }

    /// How long `part` has been reading, once its head shows it.
    pub(super) fn clock(&self, part: Part) -> Option<Duration> {
        let elapsed = self.slots.get(&part)?.run.as_ref()?.elapsed();
        (elapsed >= theme::CLOCK_AFTER).then_some(elapsed)
    }

    /// After drawing `part`: starts it the first time it is on screen, and
    /// marks it busy for screen readers.
    pub(super) fn settle(&mut self, ui: &egui::Ui, part: Part, response: &egui::Response) {
        if !self.started(part) && ui.is_rect_visible(response.rect) {
            self.start(part);
            ui.ctx().request_repaint_after(job::POLL_EVERY);
        }
        if self.slots.get(&part).is_some_and(|slot| slot.run.is_some()) {
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_busy());
        }
    }

    /// A test's stand-in for `part`'s read.
    #[cfg(test)]
    pub(in crate::shell::app) fn reading(&mut self, part: Part, job: Job<Result<Stats, String>>) {
        let slot = self.slots.entry(part).or_default();
        slot.error = None;
        slot.run = Some(job);
    }
}

/// The block's head: its label, and from 2s the Thinking mark and a clock.
pub(super) fn block_head(ui: &mut egui::Ui, name: &str, clock: Option<Duration>) {
    ui.horizontal(|ui| {
        block_label(ui, name);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            clock_note(ui, clock);
        });
    });
}

/// A panel's head: its title and caption on the left, and `controls` from the
/// right edge, right to left.
pub(super) fn panel_head(
    ui: &mut egui::Ui,
    name: &str,
    caption: &str,
    controls: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = PANEL_HEAD_GAP;
        panel_title(ui, name);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            controls(ui);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(caption)
                            .font(theme::meta())
                            .color(p.ink_muted),
                    )
                    .truncate(),
                );
            });
        });
    })
    .response
}

/// The Thinking mark and how long it has been, in a right-to-left layout;
/// nothing before [`theme::CLOCK_AFTER`].
pub(in crate::shell::app) fn clock_note(ui: &mut egui::Ui, clock: Option<Duration>) {
    let Some(elapsed) = clock else {
        return;
    };
    let p = theme::palette(ui);
    ui.spacing_mut().item_spacing.x = theme::SPACE_1;
    ui.label(
        RichText::new(crate::format::clock(elapsed))
            .font(theme::meta())
            .color(p.ink_muted),
    );
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(theme::ICON_SIZE), Sense::hover());
    theme::paint_icon(ui.painter(), Icon::Thinking, rect, p.ink);
}

/// What failed, in `danger`, and Retry; true when pressed.
pub(in crate::shell::app) fn failure(ui: &mut egui::Ui, what: &str, error: &str) -> bool {
    let p = theme::palette(ui);
    ui.add(
        egui::Label::new(
            RichText::new(format!("Couldn't read {what}: {error}"))
                .font(theme::meta())
                .color(p.danger),
        )
        .truncate(),
    );
    widgets::button(ui, ButtonKind::Ghost, None, "Retry", None).clicked()
}

/// The head `part` has before its data: a tile's label or a panel's title.
fn part_head(ui: &mut egui::Ui, part: Part, name: &str, clock: Option<Duration>) {
    if matches!(part, Part::Tile(_)) {
        block_head(ui, name, clock);
    } else {
        panel_head(ui, name, "", |ui| clock_note(ui, clock));
    }
}

/// `part` in a [`block`] at least `height` tall whatever its phase. `loaded`
/// draws the head itself, as some heads hold controls, and is given the clock.
pub(super) fn framed<R>(
    ui: &mut egui::Ui,
    blocks: &mut Blocks,
    (part, name, height): (Part, &str, f32),
    waiting: impl FnOnce(&mut egui::Ui, bool),
    loaded: impl FnOnce(&mut egui::Ui, &Stats, Option<Duration>) -> R,
) -> Option<R> {
    let phase = blocks.phase(part);
    let clock = blocks.clock(part);
    let mut retry = false;
    let scope = ui.scope(|ui| {
        block(ui, |ui| {
            ui.set_min_height(height);
            match phase {
                Phase::Loaded { stats, stale } => {
                    if stale {
                        ui.multiply_opacity(STALE_OPACITY);
                    }
                    Some(loaded(ui, &stats, clock))
                }
                Phase::Waiting { pulse } => {
                    part_head(ui, part, name, clock);
                    waiting(ui, pulse);
                    None
                }
                Phase::Failed(error) => {
                    part_head(ui, part, name, None);
                    retry = failure(ui, part.what(), &error);
                    None
                }
            }
        })
    });
    if retry {
        blocks.start(part);
    }
    blocks.settle(ui, part, &scope.response);
    scope.inner
}

/// Dims `phase`'s data while it is stale, for the blocks not drawn by
/// [`framed`].
pub(super) fn dim_if_stale(ui: &mut egui::Ui, phase: &Phase) {
    ui.multiply_opacity(stale_opacity(phase));
}

/// The opacity of `phase`'s data.
pub(super) fn stale_opacity(phase: &Phase) -> f32 {
    if matches!(phase, Phase::Loaded { stale: true, .. }) {
        STALE_OPACITY
    } else {
        1.0
    }
}

/// A failure inside a row of a block, which says what failed and retries.
pub(super) fn failed_row(ui: &mut egui::Ui, blocks: &mut Blocks, part: Part, error: &str) {
    if failure(ui, part.what(), error) {
        blocks.start(part);
    }
}

/// The list placeholder: rows of a dot, a name bar, a count bar and a `line`
/// divider, blank but for the dividers before `pulse`.
pub(super) fn list_rows(ui: &mut egui::Ui, pulse: bool) {
    let p = theme::palette(ui);
    let saved = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    for share in LIST_ROWS {
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), LIST_ROW_H), Sense::hover());
        let painter = ui.painter_at(rect);
        if pulse {
            let fill = theme::skeleton_fill(ui.ctx(), p);
            let mid = rect.center().y;
            let bar = |left: f32, width: f32| {
                Rect::from_min_size(Pos2::new(left, mid - 4.0), Vec2::new(width, 8.0))
            };
            painter.circle_filled(Pos2::new(rect.left() + 4.0, mid), 4.0, fill);
            let name = bar(rect.left() + 16.0, (rect.width() - 64.0).max(0.0) * share);
            let count = bar(rect.right() - 32.0, 32.0);
            let radius = CornerRadius::same(theme::RADIUS_SM);
            painter.rect_filled(name, radius, fill);
            painter.rect_filled(count, radius, fill);
        }
        painter.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, p.line),
        );
    }
    ui.spacing_mut().item_spacing.y = saved;
}

/// A tile's placeholder: a value bar and a meta bar.
pub(super) fn tile_bars(ui: &mut egui::Ui, pulse: bool) {
    placeholder(ui, 0.46, 22.0, pulse);
    placeholder(ui, 0.58, 8.0, pulse);
}

/// Announces the screen's loading once, as one live region for all its
/// blocks; the placeholders themselves stay silent.
pub(super) fn announce_loading(ui: &mut egui::Ui) {
    widgets::announce(ui, "stats-loading", "Loading stats");
}
