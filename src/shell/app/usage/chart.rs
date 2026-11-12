//! The Usage screen's per-day chart: a bar per day, stacked by provider on All,
//! and the readout above it that hover and the arrow keys move.

use std::collections::BTreeMap;

use chrono::{Local, NaiveDate};

use super::blocks::{self, Read};
use super::{
    Align, CornerRadius, Key, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2, theme, widgets,
};
use crate::agent::Billing;
use crate::domains::insights::usage::{DayUsage, ProviderUsage, Range, UsageView};
use crate::kernel::labels::Hue;

/// The plot and its day labels, the readout's own height flowing with it.
const CHART_H: f32 = 152.0;
/// The strip at the chart's left for its token bounds, and under it for days.
const AXIS_W: f32 = 46.0;
const PLOT_TOP: f32 = 6.0;
const PLOT_RIGHT: f32 = 6.0;
const DAY_LABEL_H: f32 = 16.0;
/// The closest two day labels sit, so a quarter keeps its month names apart.
const DAY_GAP_MIN: f32 = 44.0;

/// What the chart counts: tokens, or a metered provider's real dollars.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Metric {
    #[default]
    Tokens,
    Spend,
}

impl Metric {
    pub(super) const ALL: [(Metric, &'static str); 2] =
        [(Metric::Tokens, "Tokens"), (Metric::Spend, "Spend")];

    pub(super) fn title(self) -> &'static str {
        match self {
            Metric::Tokens => "Tokens",
            Metric::Spend => "Spend",
        }
    }
}

/// One provider's share of one bar.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct BarPart {
    pub provider: String,
    pub name: String,
    pub hue: Hue,
    pub metered: bool,
    pub tokens: u64,
    pub runs: usize,
    pub equivalent: f64,
    pub value: f64,
}

/// One day of the range, with a part per provider that ran; no run leaves no
/// part, so the day is an empty slot rather than a zero bar.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DayBar {
    pub day: NaiveDate,
    pub parts: Vec<BarPart>,
    pub tokens: u64,
    pub runs: usize,
    pub equivalent: f64,
    pub total: f64,
}

impl DayBar {
    pub(super) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// The chart's series and its rounded y maximum.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ChartData {
    pub bars: Vec<DayBar>,
    pub max: f64,
    pub metric: Metric,
    pub stacked: bool,
}

impl ChartData {
    /// One bar per day of `range`, today last. `chosen` is the provider shown,
    /// or `None` for All, whose bars stack every provider in the tool's order.
    pub(super) fn of(
        view: &UsageView,
        chosen: Option<&ProviderUsage>,
        range: Range,
        metric: Metric,
    ) -> Self {
        let today = view.now.with_timezone(&Local).date_naive();
        let (first, last) = range.span(today);
        let providers: Vec<&ProviderUsage> = match chosen {
            Some(provider) => vec![provider],
            None => view.providers.iter().collect(),
        };
        let by_day: Vec<BTreeMap<NaiveDate, &DayUsage>> = providers
            .iter()
            .map(|provider| {
                provider
                    .days_in(range)
                    .iter()
                    .map(|day| (day.day, day))
                    .collect()
            })
            .collect();
        let mut bars = Vec::new();
        let mut day = first;
        while day <= last {
            let mut parts = Vec::new();
            let (mut tokens, mut runs, mut equivalent) = (0, 0, 0.0);
            for (at, provider) in providers.iter().enumerate() {
                let Some(usage) = by_day[at].get(&day).copied() else {
                    continue;
                };
                let value = match metric {
                    Metric::Tokens => usage.tokens as f64,
                    Metric::Spend => usage.spent,
                };
                tokens += usage.tokens;
                runs += usage.runs;
                equivalent += usage.equivalent;
                if value <= 0.0 {
                    continue;
                }
                parts.push(BarPart {
                    provider: provider.provider.clone(),
                    name: provider.name.clone(),
                    hue: provider.hue,
                    metered: provider.billing == Billing::Metered,
                    tokens: usage.tokens,
                    runs: usage.runs,
                    equivalent: usage.equivalent,
                    value,
                });
            }
            let total = parts.iter().map(|part| part.value).sum();
            bars.push(DayBar {
                day,
                parts,
                tokens,
                runs,
                equivalent,
                total,
            });
            day += chrono::Duration::days(1);
        }
        let peak = bars.iter().map(|bar| bar.total).fold(0.0, f64::max);
        Self {
            bars,
            max: nice(peak),
            metric,
            stacked: chosen.is_none(),
        }
    }

    /// Whether no provider ran in the range.
    pub(super) fn is_empty(&self) -> bool {
        self.bars.iter().all(|bar| bar.tokens == 0)
    }

    /// The day with the most tokens, for the tile and the chart's label.
    pub(super) fn busiest(&self) -> Option<&DayBar> {
        self.bars
            .iter()
            .filter(|bar| bar.tokens > 0)
            .max_by_key(|bar| bar.tokens)
    }
}

/// What the chart asked for: another measure, or another read.
pub(super) enum Asked {
    Measure(Metric),
    Retry,
}

/// The chart's per-frame state: the range, the measure and the day the
/// readout shows.
pub(super) struct State<'a> {
    pub range: Range,
    pub metric: &'a mut Metric,
    pub hover: &'a mut Option<usize>,
    pub selected: &'a mut Option<usize>,
}

/// The chart block: its head, the readout, then the bars.
pub(super) fn chart_ui(
    ui: &mut egui::Ui,
    view: &UsageView,
    chosen: Option<&ProviderUsage>,
    data: &ChartData,
    state: &mut State<'_>,
    read: &Read,
) -> Option<Asked> {
    let mut asked = None;
    if read.waiting().is_some() {
        blocks::announce_loading(ui);
    }
    blocks::card(ui, |ui| {
        let can_spend = chosen.is_some_and(|provider| provider.billing == Billing::Metered);
        let today = view.now.with_timezone(&Local).date_naive();
        let mut asked_in = None;
        head(ui, view, data, can_spend, state, &mut asked_in);
        if let Some(error) = read.failed() {
            if blocks::failure(ui, error) {
                asked_in = Some(Asked::Retry);
            }
        } else if let Some(pulse) = read.waiting() {
            skeleton(ui, pulse);
        } else if data.is_empty() {
            empty(ui);
        } else if data.max <= 0.0 {
            no_value(ui);
        } else {
            let index = shown_index(data, state);
            readout(ui, data, chosen, index, today);
            plot(ui, data, state, today);
        }
        asked = asked_in;
    });
    asked
}

/// `Tokens per day`, the range, then the legend and the measure control.
fn head(
    ui: &mut egui::Ui,
    view: &UsageView,
    data: &ChartData,
    can_spend: bool,
    state: &mut State<'_>,
    asked: &mut Option<Asked>,
) {
    let p = theme::palette(ui);
    let title = format!("{} per day", data.metric.title());
    let caption = range_caption(state.range, today_of(view));
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).font(theme::ui_title()).color(p.ink));
        ui.add_space(theme::SPACE_2);
        ui.label(
            RichText::new(caption)
                .font(theme::meta())
                .color(p.ink_muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if can_spend {
                let picked = widgets::segmented(ui, *state.metric, &Metric::ALL);
                if let Some(metric) = picked {
                    *state.metric = metric;
                    *asked = Some(Asked::Measure(metric));
                }
                ui.add_space(theme::SPACE_4);
            }
            if data.stacked {
                legend(ui, view);
            }
        });
    });
}

/// One swatch and name per provider, in the tool's order.
fn legend(ui: &mut egui::Ui, view: &UsageView) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        for provider in &view.providers {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(2), p.label(provider.hue));
            ui.label(
                RichText::new(&provider.name)
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
            ui.add_space(theme::SPACE_3);
        }
    });
}

/// The day the readout shows: the bar under the pointer, the arrow key's day,
/// or today.
fn shown_index(data: &ChartData, state: &State<'_>) -> usize {
    let last = data.bars.len().saturating_sub(1);
    state.hover.or(*state.selected).unwrap_or(last).min(last)
}

/// `Today · Thu 12 Nov`, its value, then runs and dollars, or All's split.
fn readout(
    ui: &mut egui::Ui,
    data: &ChartData,
    chosen: Option<&ProviderUsage>,
    index: usize,
    today: NaiveDate,
) {
    let p = theme::palette(ui);
    let Some(bar) = data.bars.get(index) else {
        return;
    };
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 0.0),
        Layout::top_down(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                ui.label(
                    RichText::new(day_head(bar.day, today))
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
                let no_run = bar.tokens == 0;
                let value = if no_run {
                    "No run".to_string()
                } else {
                    match data.metric {
                        Metric::Tokens => compact_tokens(bar.tokens),
                        Metric::Spend => format!("${:.2}", bar.total),
                    }
                };
                ui.label(RichText::new(value).font(theme::read_h2()).color(p.ink));
                if !no_run && data.metric == Metric::Tokens {
                    ui.label(
                        RichText::new("tokens")
                            .font(theme::meta())
                            .color(p.ink_muted),
                    );
                }
            });
            if bar.is_empty() {
                return;
            }
            match chosen {
                None => split(ui, bar),
                Some(provider) => one_provider(ui, bar, provider.billing, data.metric),
            }
        },
    );
}

fn one_provider(ui: &mut egui::Ui, bar: &DayBar, billing: Billing, metric: Metric) {
    let p = theme::palette(ui);
    let runs = format!(
        "{} {}",
        bar.runs,
        if bar.runs == 1 { "run" } else { "runs" }
    );
    let tokens = format!("{} tokens", compact_tokens(bar.tokens));
    let dollars = match (metric, billing) {
        (Metric::Spend, _) => tokens,
        (Metric::Tokens, Billing::Metered) if bar.equivalent > 0.0 => {
            format!("${:.2}", bar.equivalent)
        }
        (Metric::Tokens, Billing::Subscription) => {
            format!("${:.2} API-equivalent", bar.equivalent)
        }
        (Metric::Tokens, Billing::Metered) => String::new(),
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        ui.label(RichText::new(runs).font(theme::meta()).color(p.ink_muted));
        if !dollars.is_empty() {
            ui.label(
                RichText::new(dollars)
                    .font(theme::meta())
                    .color(p.ink_faint),
            );
        }
    });
}

/// All's readout: one swatch, name and token count per provider.
fn split(ui: &mut egui::Ui, bar: &DayBar) {
    let p = theme::palette(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        for part in bar.parts.iter().filter(|part| part.tokens > 0) {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(2), p.label(part.hue));
            ui.label(
                RichText::new(format!("{} {}", part.name, compact_tokens(part.tokens)))
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
            ui.add_space(theme::SPACE_3);
        }
    });
}

/// No run in the range: the chart's place says so.
fn empty(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    ui.label(
        RichText::new("No agent run in this range.")
            .font(theme::ui_small())
            .color(p.ink_muted),
    );
    ui.label(
        RichText::new("A finished run's tokens land here.")
            .font(theme::meta())
            .color(p.ink_faint),
    );
}

/// Spend was asked of a range whose runs carry no cost.
fn no_value(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    ui.label(
        RichText::new("No spend recorded in this range.")
            .font(theme::ui_small())
            .color(p.ink_muted),
    );
    ui.label(
        RichText::new("A finished run's cost lands here.")
            .font(theme::meta())
            .color(p.ink_faint),
    );
}

/// The bars, their grid and day labels, and the pointer and keyboard that
/// choose a day.
fn plot(ui: &mut egui::Ui, data: &ChartData, state: &mut State<'_>, today: NaiveDate) {
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), CHART_H), Sense::click());
    let label = chart_label(data, state.range, today);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &label));
    if response.clicked() {
        response.request_focus();
    }
    let plot = plot_rect(rect);
    let painter = ui.painter_at(rect);
    let (slot, gap) = columns(plot, data.bars.len());
    let shown = shown_index(data, state);
    wash(&painter, plot, slot, gap, shown, data.bars.len(), p);
    grid(&painter, plot, data, p);
    for (at, bar) in data.bars.iter().enumerate() {
        let left = plot.left() + at as f32 * (slot + gap);
        bars(&painter, plot, left, slot, bar, data.max, p);
    }
    day_labels(&painter, rect, plot, data, (slot, gap), today, p);
    hover(ui, &response, plot, data, state, slot, gap);
    keys(ui, &response, data, state);
    widgets::focus_ring(ui, &response, theme::RADIUS_SM);
}

fn plot_rect(rect: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.left() + AXIS_W, rect.top() + PLOT_TOP),
        Pos2::new(
            rect.right() - PLOT_RIGHT,
            rect.bottom() - DAY_LABEL_H - PLOT_TOP,
        ),
    )
}

/// The bar's width and the space between two, as the preview's three steps.
fn columns(plot: Rect, count: usize) -> (f32, f32) {
    let gap = match count {
        0..=10 => 8.0,
        11..=40 => 4.0,
        _ => 2.0,
    };
    let count = count.max(1) as f32;
    ((plot.width() - gap * (count - 1.0)).max(1.0) / count, gap)
}

/// The selected day's `surface-hover` wash, behind its bars.
fn wash(
    painter: &egui::Painter,
    plot: Rect,
    slot: f32,
    gap: f32,
    shown: usize,
    count: usize,
    p: &theme::Palette,
) {
    if shown >= count {
        return;
    }
    let left = plot.left() + shown as f32 * (slot + gap);
    let rect = Rect::from_min_max(
        Pos2::new(left, plot.top()),
        Pos2::new(left + slot, plot.bottom()),
    );
    painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
}

fn grid(painter: &egui::Painter, plot: Rect, data: &ChartData, p: &theme::Palette) {
    for (value, y) in [
        (data.max, plot.top()),
        (data.max / 2.0, plot.center().y),
        (0.0, plot.bottom()),
    ] {
        let color = if value == 0.0 { p.line } else { p.line_soft };
        painter.hline(plot.x_range(), y, Stroke::new(1.0, color));
        painter.text(
            Pos2::new(plot.left() - 6.0, y),
            egui::Align2::RIGHT_CENTER,
            axis_text(data.metric, value),
            theme::meta(),
            p.ink_muted,
        );
    }
}

fn bars(
    painter: &egui::Painter,
    plot: Rect,
    left: f32,
    slot: f32,
    bar: &DayBar,
    max: f64,
    p: &theme::Palette,
) {
    if bar.is_empty() || max <= 0.0 {
        return;
    }
    let width = (slot * 0.62).clamp(1.0, 18.0);
    let left = left + (slot - width) / 2.0;
    let mut base = plot.bottom();
    let count = bar.parts.len();
    for (at, part) in bar.parts.iter().enumerate() {
        let height = ((part.value / max) as f32 * plot.height()).max(1.0);
        let top = (base - height).max(plot.top());
        let radius = if at + 1 == count {
            CornerRadius {
                nw: theme::RADIUS_SM,
                ne: theme::RADIUS_SM,
                ..CornerRadius::ZERO
            }
        } else {
            CornerRadius::ZERO
        };
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + width, base)),
            radius,
            p.label(part.hue),
        );
        if at + 1 < count {
            painter.hline(left..=(left + width), top, Stroke::new(1.0, p.surface));
        }
        base = top;
    }
}

/// Every day at 7d, every fifth at 30d and every fifteenth at 90d, today in
/// `ink`.
fn day_labels(
    painter: &egui::Painter,
    rect: Rect,
    plot: Rect,
    data: &ChartData,
    (slot, gap): (f32, f32),
    today: NaiveDate,
    p: &theme::Palette,
) {
    let count = data.bars.len();
    let step = match count {
        0..=7 => 1,
        8..=30 => 5,
        _ => 15,
    };
    let mut last = f32::NEG_INFINITY;
    for (at, bar) in data.bars.iter().enumerate() {
        if !(count - 1 - at).is_multiple_of(step) {
            continue;
        }
        let x = plot.left() + at as f32 * (slot + gap) + slot / 2.0;
        if x - last < DAY_GAP_MIN && at + 1 != count {
            continue;
        }
        last = x;
        let text = if count <= 7 {
            bar.day.format("%a %-d").to_string()
        } else {
            bar.day.format("%-d %b").to_string()
        };
        painter.text(
            Pos2::new(x, rect.bottom() - DAY_LABEL_H / 2.0),
            egui::Align2::CENTER_CENTER,
            text,
            theme::meta(),
            if bar.day == today { p.ink } else { p.ink_faint },
        );
    }
}

/// The pointer chooses the day under it and leaving puts the readout back on
/// today.
fn hover(
    ui: &egui::Ui,
    response: &egui::Response,
    plot: Rect,
    data: &ChartData,
    state: &mut State<'_>,
    slot: f32,
    gap: f32,
) {
    let count = data.bars.len();
    let moving = ui.input(|i| i.pointer.delta() != Vec2::ZERO);
    let next = if !moving && state.selected.is_some() {
        *state.hover
    } else {
        response.hover_pos().and_then(|pos| {
            if pos.x < plot.left() || pos.x > plot.right() {
                return None;
            }
            let at = ((pos.x - plot.left()) / (slot + gap)).floor() as usize;
            Some(at.min(count.saturating_sub(1)))
        })
    };
    if *state.hover != next {
        *state.hover = next;
        ui.ctx().request_repaint();
    }
}

/// `←` `→` `Home` `End` on the focused chart move the readout by a day.
fn keys(ui: &egui::Ui, response: &egui::Response, data: &ChartData, state: &mut State<'_>) {
    if !response.has_focus() {
        return;
    }
    let last = data.bars.len().saturating_sub(1);
    let shown = shown_index(data, state);
    let mut next = None;
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft)) {
        next = Some(shown.saturating_sub(1));
    } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowRight)) {
        next = Some((shown + 1).min(last));
    } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Home)) {
        next = Some(0);
    } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::End)) {
        next = Some(last);
    }
    if let Some(at) = next
        && *state.selected != Some(at)
    {
        *state.selected = Some(at);
        *state.hover = None;
        ui.ctx().request_repaint();
    }
}

/// What VoiceOver reads: the measure, the range and the busiest day.
fn chart_label(data: &ChartData, range: Range, today: NaiveDate) -> String {
    let range = range_caption(range, today);
    match data.busiest() {
        Some(bar) => format!(
            "{} per day, {range}. Busiest day {}. Left and right arrows read a day.",
            data.metric.title(),
            bar.day.format("%a %-d %b")
        ),
        None => format!(
            "{} per day, {range}. No agent run in this range.",
            data.metric.title()
        ),
    }
}

fn range_caption(range: Range, today: NaiveDate) -> String {
    let (first, last) = range.span(today);
    format!("{} – {}", first.format("%-d %b"), last.format("%-d %b"))
}

fn today_of(view: &UsageView) -> NaiveDate {
    view.now.with_timezone(&Local).date_naive()
}

/// A day's name, `Today` first when it is.
fn day_head(day: NaiveDate, today: NaiveDate) -> String {
    if day == today {
        format!("Today · {}", day.format("%a %-d %b"))
    } else {
        day.format("%a %-d %b").to_string()
    }
}

/// Tokens in the chart's short form: `999`, `750k`, `1.5M`.
pub(super) fn compact_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        tokens.to_string()
    } else if tokens < 1_000_000 {
        format!("{:.0}k", tokens as f64 / 1_000.0)
    } else {
        let millions = tokens as f64 / 1_000_000.0;
        if (millions - millions.round()).abs() < 0.05 {
            format!("{millions:.0}M")
        } else {
            format!("{millions:.1}M")
        }
    }
}

fn axis_text(metric: Metric, value: f64) -> String {
    match metric {
        Metric::Tokens => compact_tokens(value.round() as u64),
        Metric::Spend if value >= 10.0 => format!("${value:.0}"),
        Metric::Spend => format!("${value:.1}"),
    }
}

/// A rounded maximum over the bars: 1, 1.5, 2, 2.5, 3, 4, 5, 6, 8 or 10
/// times a power of ten.
pub(super) fn nice(value: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    let magnitude = 10f64.powf(value.log10().floor());
    let fraction = value / magnitude;
    const STEPS: [f64; 10] = [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0];
    STEPS
        .into_iter()
        .find(|step| fraction <= step + 1e-9)
        .unwrap_or(10.0)
        * magnitude
}

/// The chart's placeholder: its grid at once, then a handful of soft bars and
/// day labels.
fn skeleton(ui: &mut egui::Ui, pulse: bool) {
    let p = theme::palette(ui);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), CHART_H), Sense::hover());
    let plot = plot_rect(rect);
    let painter = ui.painter_at(rect);
    for y in [plot.top(), plot.center().y, plot.bottom()] {
        painter.hline(
            plot.x_range(),
            y,
            Stroke::new(
                1.0,
                if y == plot.bottom() {
                    p.line
                } else {
                    p.line_soft
                },
            ),
        );
    }
    if !pulse {
        return;
    }
    const SHARES: [f32; 14] = [
        0.20, 0.42, 0.12, 0.68, 0.34, 0.52, 0.08, 0.76, 0.46, 0.22, 0.58, 0.30, 0.64, 0.18,
    ];
    let (slot, gap) = columns(plot, SHARES.len());
    let width = (slot * 0.62).clamp(1.0, 18.0);
    let fill = theme::skeleton_fill(ui.ctx(), p);
    for (at, share) in SHARES.into_iter().enumerate() {
        let left = plot.left() + at as f32 * (slot + gap) + (slot - width) / 2.0;
        let height = plot.height() * share;
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(left, plot.bottom() - height),
                Pos2::new(left + width, plot.bottom()),
            ),
            CornerRadius::ZERO,
            fill,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Billing, Provider, Tokens};
    use crate::domains::agent::artifacts::usage_log::Entry;
    use crate::domains::insights::usage::ListedProvider;
    use chrono::{DateTime, Duration, Utc};
    use std::path::PathBuf;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-12T15:30:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn run(session: &str, provider: &str, tokens: u64, now: DateTime<Utc>, ago: i64) -> Entry {
        let ended = (now - Duration::days(ago)).to_rfc3339();
        Entry {
            session: session.to_string(),
            project: PathBuf::from("/app"),
            engine: "opencode".to_string(),
            provider: Some(provider.to_string()),
            model: Some("model".to_string()),
            billing: if provider == "copilot" {
                Billing::Subscription
            } else {
                Billing::Metered
            },
            cost: Some(0.5),
            tokens: Tokens {
                input: tokens,
                ..Tokens::default()
            },
            started: ended.clone(),
            ended,
        }
    }

    fn listed() -> [ListedProvider; 2] {
        [
            ListedProvider::from_provider(Provider::DeepSeek),
            ListedProvider::from_provider(Provider::Copilot),
        ]
    }

    #[test]
    fn the_stack_follows_the_tools_order_and_the_max_rounds_up() {
        let now = now();
        let entries = [
            run("a", "deepseek", 100, now, 0),
            run("b", "copilot", 310, now, 0),
        ];
        let view = UsageView::read_with(&entries, &["/app".into()], now, &listed());
        let data = ChartData::of(&view, None, Range::D7, Metric::Tokens);
        let last = data.bars.last().unwrap();
        let names: Vec<&str> = last.parts.iter().map(|part| part.name.as_str()).collect();
        assert_eq!(names, ["DeepSeek", "Copilot"]);
        assert_eq!(data.max, 500.0);
    }

    #[test]
    fn a_day_without_a_run_is_an_empty_slot() {
        let now = now();
        let entries = [
            run("old", "deepseek", 100, now, 2),
            run("new", "deepseek", 50, now, 0),
        ];
        let view = UsageView::read_with(&entries, &["/app".into()], now, &listed());
        let data = ChartData::of(&view, None, Range::D7, Metric::Tokens);
        assert_eq!(data.bars.len(), 7);
        assert!(!data.bars[6].is_empty(), "today has a bar");
        assert!(data.bars[5].is_empty(), "yesterday is a gap");
        assert!(!data.bars[4].is_empty());
    }

    #[test]
    fn a_maximum_is_rounded_to_a_readable_step() {
        assert_eq!(nice(0.0), 0.0);
        assert_eq!(nice(410.0), 500.0);
        assert_eq!(nice(1_234.0), 1_500.0);
        assert_eq!(nice(2_400_000.0), 2_500_000.0);
    }
}
