//! The four figures atop a provider's pane: tokens, spend or API-equivalent,
//! runs and the busiest day.

use super::blocks::{self, Read, placeholder};
use super::chart::{ChartData, compact_tokens};
use super::{RichText, theme};
use crate::agent::Billing;
use crate::domains::insights::usage::{ProviderUsage, Range, Totals, UsageView};

/// A tile's height inside its card, so nothing moves as it fills.
const TILE_H: f32 = 96.0;

/// The tiles, or the read's skeleton or failure. True when Retry was pressed.
pub(super) fn tiles(
    ui: &mut egui::Ui,
    view: &UsageView,
    chosen: Option<&ProviderUsage>,
    range: Range,
    data: &ChartData,
    read: &Read,
) -> bool {
    let mut retry = false;
    if let Some(error) = read.failed() {
        blocks::card(ui, |ui| {
            ui.set_min_height(TILE_H);
            retry = blocks::failure(ui, error);
        });
        return retry;
    }
    let figures = figures(view, chosen, range, data);
    ui.columns(4, |columns| {
        for (at, ui) in columns.iter_mut().enumerate() {
            if let Some(pulse) = read.waiting() {
                skeleton(ui, at, pulse);
                continue;
            }
            tile_card(ui, &figures[at]);
        }
    });
    retry
}

/// One tile's text.
struct Figure {
    label: &'static str,
    value: String,
    meta: String,
    faint: bool,
}

fn figures(
    view: &UsageView,
    chosen: Option<&ProviderUsage>,
    range: Range,
    data: &ChartData,
) -> [Figure; 4] {
    let totals = match chosen {
        Some(provider) => *provider.totals(range),
        None => view.totals(range),
    };
    let previous = match chosen {
        Some(provider) => *provider.previous_totals(range),
        None => view.previous_totals(range),
    };
    let metered = match chosen {
        Some(provider) => provider.billing == Billing::Metered,
        None => view
            .providers
            .iter()
            .any(|provider| provider.billing == Billing::Metered),
    };
    [
        Figure {
            label: "Tokens",
            value: compact_tokens(totals.tokens),
            meta: change_text(totals.tokens_change(&previous), range),
            faint: totals.tokens_change(&previous).is_none(),
        },
        spend_figure(totals, metered),
        Figure {
            label: "Runs",
            value: totals.runs.to_string(),
            meta: format!(
                "{:.1} a day on average",
                totals.runs as f64 / range.days() as f64
            ),
            faint: false,
        },
        busiest_figure(data),
    ]
}

fn spend_figure(totals: Totals, metered: bool) -> Figure {
    if metered {
        return Figure {
            label: "Spent",
            value: format!("${:.2}", totals.spent),
            meta: if totals.equivalent > 0.0 {
                format!(
                    "plus ${:.2} API-equivalent, never charged",
                    totals.equivalent
                )
            } else {
                "billed by the provider".to_string()
            },
            faint: false,
        };
    }
    Figure {
        label: "API-equivalent",
        value: format!("${:.2}", totals.equivalent),
        meta: "never charged: it is a subscription".to_string(),
        faint: false,
    }
}

fn busiest_figure(data: &ChartData) -> Figure {
    match data.busiest() {
        Some(bar) => Figure {
            label: "Busiest day",
            value: compact_tokens(bar.tokens),
            meta: bar.day.format("%a %-d %b").to_string(),
            faint: false,
        },
        None => Figure {
            label: "Busiest day",
            value: "–".to_string(),
            meta: "No run in this range".to_string(),
            faint: true,
        },
    }
}

/// `+3% vs the previous 30 days`, `−3%`, or the word when there is no history.
fn change_text(change: Option<f32>, range: Range) -> String {
    let days = range.days();
    let Some(change) = change else {
        return format!("No run in the {days} days before");
    };
    let rounded = change.round() as i32;
    if rounded == 0 {
        return format!("No change vs the previous {days} days");
    }
    let sign = if rounded < 0 { '−' } else { '+' };
    format!("{sign}{}% vs the previous {days} days", rounded.abs())
}

fn tile_card(ui: &mut egui::Ui, figure: &Figure) {
    blocks::card(ui, |ui| {
        ui.set_min_height(TILE_H);
        let p = theme::palette(ui);
        ui.label(
            RichText::new(figure.label.to_uppercase())
                .font(theme::ui_label())
                .color(p.ink_muted),
        );
        ui.label(
            RichText::new(&figure.value)
                .font(egui::FontId::new(24.0, theme::ui_title().family))
                .color(p.ink),
        );
        ui.add(
            egui::Label::new(RichText::new(&figure.meta).font(theme::meta()).color(
                if figure.faint {
                    p.ink_faint
                } else {
                    p.ink_muted
                },
            ))
            .truncate(),
        );
    });
}

fn skeleton(ui: &mut egui::Ui, at: usize, pulse: bool) {
    const LABELS: [&str; 4] = ["Tokens", "Spent", "Runs", "Busiest day"];
    let p = theme::palette(ui);
    blocks::card(ui, |ui| {
        ui.set_min_height(TILE_H);
        ui.label(
            RichText::new(LABELS[at].to_uppercase())
                .font(theme::ui_label())
                .color(p.ink_muted),
        );
        placeholder(ui, 0.46, 22.0, pulse);
        placeholder(ui, 0.58, 8.0, pulse);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_spend_names_the_api_equivalent_it_never_charges() {
        let totals = Totals {
            tokens: 10,
            runs: 2,
            spent: 12.5,
            equivalent: 1.5,
        };
        let spent = spend_figure(totals, true);
        assert_eq!(spent.label, "Spent");
        assert_eq!(spent.value, "$12.50");
        assert_eq!(spent.meta, "plus $1.50 API-equivalent, never charged");
    }

    #[test]
    fn a_subscription_reads_api_equivalent_not_spend() {
        let totals = Totals {
            tokens: 10,
            runs: 2,
            spent: 0.0,
            equivalent: 1.5,
        };
        let shown = spend_figure(totals, false);
        assert_eq!(shown.label, "API-equivalent");
        assert_eq!(shown.value, "$1.50");
    }

    #[test]
    fn the_token_change_reads_against_the_previous_range() {
        assert_eq!(
            change_text(Some(-3.0), Range::D30),
            "−3% vs the previous 30 days"
        );
        assert_eq!(
            change_text(None, Range::D30),
            "No run in the 30 days before"
        );
    }
}
