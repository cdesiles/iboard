//! The Cost block of Project stats: what the agents cost over the days the
//! trends cover, a day's tokens stacked by provider and a row per provider.

use crate::format::thousands;
use crate::stats::{Cost, CostDay, CostProvider};
use chrono::Datelike;

use super::blocks::{Blocks, Part, clock_note, framed, list_rows, panel_head};
use super::{CornerRadius, Duration, Pos2, Rect, RichText, Sense, Stroke, Vec2, theme};

/// The block's height, so a failure keeps its size while it fills.
const COST_H: f32 = 312.0;
/// The chart and the month row under it.
const CHART_H: f32 = 98.0;
const MONTHS_H: f32 = 14.0;
/// The strip at the chart's left that holds its token bounds.
const AXIS_W: f32 = 46.0;
/// How far the plot stays from its block's top, bottom and right.
const PLOT_INSET: f32 = 2.0;
const PLOT_RIGHT: f32 = 4.0;
/// The closest two month names sit.
const MONTH_GAP: f32 = 28.0;
/// A provider row, and its swatch.
const ROW_H: f32 = 27.0;
const SWATCH: f32 = 8.0;
/// The provider row's token and money columns.
const TOKENS_W: f32 = 118.0;

const NAME: &str = "Cost";

/// The range's cost on its own job: the total, a day per bar and a row per
/// provider.
pub(super) fn cost_block(ui: &mut egui::Ui, blocks: &mut Blocks) {
    framed(
        ui,
        blocks,
        (Part::Cost, NAME, COST_H),
        skeleton,
        |ui, stats, clock| body(ui, &stats.cost, clock),
    );
}

fn body(ui: &mut egui::Ui, cost: &Cost, clock: Option<Duration>) {
    let caption = caption(cost);
    panel_head(ui, NAME, &caption, |ui| clock_note(ui, clock));
    if cost.is_empty() {
        empty_state(ui);
        return;
    }
    total(ui, cost);
    let p = theme::palette(ui);
    ui.label(
        RichText::new("tokens per day, by provider")
            .font(theme::meta())
            .color(p.ink_faint),
    );
    chart(ui, cost);
    for (at, provider) in cost.providers.iter().enumerate() {
        provider_row(ui, provider, at);
    }
}

/// The days the trends cover and how many runs they hold.
fn caption(cost: &Cost) -> String {
    let Some(first) = cost.days.first() else {
        return String::new();
    };
    let last = cost.days.last().unwrap_or(first);
    let range = if first.day == last.day {
        first.day.format("%-d %b").to_string()
    } else if first.day.year() == last.day.year() {
        format!(
            "{} – {}",
            first.day.format("%-d %b"),
            last.day.format("%-d %b")
        )
    } else {
        format!(
            "{} – {}",
            first.day.format("%-d %b %Y"),
            last.day.format("%-d %b %Y")
        )
    };
    match cost.runs {
        0 => format!("{range} · no run"),
        1 => format!("{range} · 1 run"),
        runs => format!("{range} · {runs} runs"),
    }
}

/// The range's paid total, its tokens, and its API-equivalent when any run was
/// a subscription's.
fn total(ui: &mut egui::Ui, cost: &Cost) {
    let p = theme::palette(ui);
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2 + 2.0;
        let color = if cost.spent > 0.0 { p.ink } else { p.ink_muted };
        ui.label(
            RichText::new(format!("${:.2}", cost.spent))
                .font(egui::FontId::new(18.0, theme::ui_title().family))
                .color(color),
        );
        ui.label(
            RichText::new("spent")
                .font(theme::meta())
                .color(p.ink_muted),
        );
        ui.label(
            RichText::new(format!("· {} tokens", compact(cost.tokens)))
                .font(theme::meta())
                .color(p.ink_muted),
        );
        if cost.equivalent > 0.0 {
            ui.label(
                RichText::new(format!("· ${:.2} API-equivalent", cost.equivalent))
                    .font(theme::meta())
                    .color(p.ink_faint),
            );
        }
    });
    let tooltip = if cost.equivalent > 0.0 {
        format!(
            "Paid · ${:.2}\nAPI-equivalent · ${:.2}, never charged",
            cost.spent, cost.equivalent
        )
    } else {
        format!(
            "Paid · ${:.2}\nNo subscription run in this range",
            cost.spent
        )
    };
    row.response.on_hover_text(tooltip);
}

/// The tokens of each day, stacked by provider. A day without a run leaves a
/// gap, never a zero bar.
fn chart(ui: &mut egui::Ui, cost: &Cost) {
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), CHART_H), Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Unknown, true, "Tokens per day, by provider")
    });
    let painter = ui.painter_at(rect);
    let plot = plot_of(rect);
    let months = Rect::from_min_max(
        Pos2::new(plot.left(), plot.bottom()),
        Pos2::new(plot.right(), rect.bottom()),
    );
    grid(&painter, plot, p);
    let max = cost.days.iter().map(|day| day.tokens).max().unwrap_or(0);
    for (y, text) in [(plot.top(), compact(max)), (plot.bottom(), "0".to_string())] {
        painter.text(
            Pos2::new(plot.left() - 6.0, y),
            egui::Align2::RIGHT_CENTER,
            text,
            theme::meta(),
            p.ink_muted,
        );
    }
    if max == 0 {
        return;
    }
    let slot = plot.width() / cost.days.len().max(1) as f32;
    let width = (slot * 0.62).clamp(1.0, 18.0);
    for (at, day) in cost.days.iter().enumerate() {
        if day.parts.is_empty() {
            continue;
        }
        let left = plot.left() + slot * at as f32 + (slot - width) / 2.0;
        let mut base = plot.bottom();
        let mut top = base;
        let newest = cost.providers.iter().rposition(|provider| {
            day.parts
                .iter()
                .any(|part| part.provider == provider.provider && part.tokens > 0)
        });
        for (index, provider) in cost.providers.iter().enumerate() {
            let Some(part) = day
                .parts
                .iter()
                .find(|part| part.provider == provider.provider)
            else {
                continue;
            };
            let height = part.tokens as f32 / max as f32 * plot.height();
            top = (base - height).min(base);
            let color = p.labels[index % p.labels.len()];
            let segment = Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + width, base));
            let radius = if Some(index) == newest {
                CornerRadius {
                    nw: 2,
                    ne: 2,
                    ..CornerRadius::ZERO
                }
            } else {
                CornerRadius::ZERO
            };
            painter.rect_filled(segment, radius, color);
            base = top;
        }
        let bar = ui.interact(
            Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + width, plot.bottom())),
            ui.scope_id().with(("cost-bar", at)),
            Sense::hover(),
        );
        bar.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Unknown,
                true,
                format!("{} · {} tokens", day_label(day), compact(day.tokens)),
            )
        });
        bar.on_hover_text(day_tooltip(day));
    }
    month_axis(&painter, months, cost, slot, p);
}

fn plot_of(rect: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.left() + AXIS_W, rect.top() + PLOT_INSET),
        Pos2::new(
            rect.right() - PLOT_RIGHT,
            rect.bottom() - MONTHS_H - PLOT_INSET,
        ),
    )
}

fn grid(painter: &egui::Painter, plot: Rect, p: &theme::Palette) {
    for y in [plot.top(), plot.bottom()] {
        painter.hline(plot.x_range(), y, Stroke::new(1.0, p.line));
    }
}

/// Each month's name at its first day under the plot.
fn month_axis(painter: &egui::Painter, months: Rect, cost: &Cost, slot: f32, p: &theme::Palette) {
    let mut month = String::new();
    let mut last = f32::MIN;
    for (at, day) in cost.days.iter().enumerate() {
        let this = day.day.format("%Y-%m").to_string();
        if this == month {
            continue;
        }
        month = this;
        let x = months.left() + slot * at as f32 + slot / 2.0;
        if x - last < MONTH_GAP {
            continue;
        }
        last = x;
        painter.text(
            Pos2::new(x, months.center().y),
            egui::Align2::CENTER_CENTER,
            day.day.format("%b"),
            theme::meta(),
            p.ink_muted,
        );
    }
}

fn day_label(day: &CostDay) -> String {
    day.day.format("%a %-d %b").to_string()
}

fn day_tooltip(day: &CostDay) -> String {
    let mut text = format!(
        "{} · {} tokens",
        day_label(day),
        thousands(day.tokens as usize)
    );
    for part in &day.parts {
        text.push_str(&format!(
            "\n{} · {} tokens",
            provider_name(&part.provider),
            thousands(part.tokens as usize)
        ));
    }
    if day.spent > 0.0 {
        text.push_str(&format!("\n${:.2} paid", day.spent));
    }
    if day.equivalent > 0.0 {
        text.push_str(&format!("\n${:.2} API-equivalent", day.equivalent));
    }
    text
}

fn provider_name(provider: &str) -> String {
    crate::agent::provider_name(provider)
}

/// The provider's swatch, name, and its tokens; a metered provider's money is
/// the rightmost figure, a subscription's dollars stay in the tooltip.
fn provider_row(ui: &mut egui::Ui, provider: &CostProvider, at: usize) {
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.hline(
        rect.x_range(),
        rect.top() + 0.5,
        Stroke::new(1.0, p.line_soft),
    );
    let mid = rect.center().y;
    let swatch = Rect::from_center_size(
        Pos2::new(rect.left() + SWATCH / 2.0, mid),
        Vec2::splat(SWATCH),
    );
    painter.rect_filled(swatch, CornerRadius::same(2), p.labels[at % p.labels.len()]);
    painter.text(
        Pos2::new(rect.left() + SWATCH + theme::SPACE_3, mid),
        egui::Align2::LEFT_CENTER,
        &provider.name,
        theme::ui_small(),
        p.ink,
    );
    let tokens_x = rect.right() - theme::SPACE_2;
    painter.text(
        Pos2::new(tokens_x, mid),
        egui::Align2::RIGHT_CENTER,
        format!("{} tokens", compact(provider.tokens)),
        theme::meta(),
        p.ink_muted,
    );
    if provider.billing == crate::agent::Billing::Metered {
        painter.text(
            Pos2::new(tokens_x - TOKENS_W, mid),
            egui::Align2::RIGHT_CENTER,
            format!("${:.2}", provider.spent),
            theme::meta(),
            p.ink,
        );
    }
    let detail = row_detail(provider);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Unknown, true, &detail));
    response.on_hover_text(detail);
}

fn row_detail(provider: &CostProvider) -> String {
    let tokens = thousands(provider.tokens as usize);
    let runs = match provider.runs {
        1 => "1 run".to_string(),
        runs => format!("{runs} runs"),
    };
    match provider.billing {
        crate::agent::Billing::Metered => format!(
            "{} · ${:.2} paid · {tokens} tokens · {runs}",
            provider.name, provider.spent
        ),
        crate::agent::Billing::Subscription if provider.equivalent > 0.0 => format!(
            "{} · {tokens} tokens · {runs} · ${:.2} API-equivalent, never spend",
            provider.name, provider.equivalent
        ),
        crate::agent::Billing::Subscription => {
            format!("{} · {tokens} tokens · {runs}", provider.name)
        }
    }
}

fn empty_state(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    ui.label(
        RichText::new("No agent run in this range.")
            .font(theme::ui_small())
            .color(p.ink_muted),
    );
    ui.label(
        RichText::new("A finished run's cost lands here.")
            .font(theme::meta())
            .color(p.ink_faint),
    );
}

/// The chart's placeholder: its grid at once, and from `pulse` on a handful of
/// soft bars and the provider rows they add up to.
fn skeleton(ui: &mut egui::Ui, pulse: bool) {
    let p = theme::palette(ui);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), CHART_H), Sense::hover());
    let plot = plot_of(rect);
    grid(&ui.painter_at(rect), plot, p);
    if pulse {
        const SHARES: [f32; 14] = [
            0.20, 0.42, 0.12, 0.68, 0.34, 0.52, 0.08, 0.76, 0.46, 0.22, 0.58, 0.30, 0.64, 0.18,
        ];
        let slot = plot.width() / SHARES.len() as f32;
        let width = (slot * 0.62).clamp(1.0, 18.0);
        let fill = theme::skeleton_fill(ui.ctx(), p);
        let painter = ui.painter_at(rect);
        for (at, share) in SHARES.into_iter().enumerate() {
            let left = plot.left() + slot * at as f32 + (slot - width) / 2.0;
            let height = plot.height() * share;
            let bar = Rect::from_min_max(
                Pos2::new(left, plot.bottom() - height),
                Pos2::new(left + width, plot.bottom()),
            );
            painter.rect_filled(bar, CornerRadius::ZERO, fill);
        }
    }
    list_rows(ui, pulse);
}

fn compact(tokens: u64) -> String {
    if tokens < 1_000 {
        tokens.to_string()
    } else if tokens < 1_000_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_count_reads_short_above_a_thousand() {
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1_500), "1.5k");
        assert_eq!(compact(2_345_678), "2.3M");
    }
}
