//! The four tiles atop Project stats: each figure at the page's newest
//! commit, with its change since the page's oldest, or for cycle time from the
//! page's older half to its newer one.

use iboard::stats::{self, Snapshot, Stats};

use super::blocks::{Blocks, Part, block_head, framed, tile_bars};
use super::{Color32, Measure, RichText, theme, thousands};

/// A tile's height inside its frame, so nothing moves as it fills.
const TILE_H: f32 = 104.0;

/// Which way a measure going up goes.
#[derive(Clone, Copy)]
enum Up {
    Better,
    Worse,
    Neutral,
}

/// One figure, with its change, what that change is against, and what the
/// tile says without one.
struct Figure {
    value: String,
    detail: String,
    change: Option<(String, Up, bool)>,
    since: String,
    quiet: &'static str,
}

fn figure(stats: &Stats, measure: Measure) -> Figure {
    if measure == Measure::Cycle {
        let cycles = &stats.cycles;
        let (value, detail) = match (cycles.percentile(0.5), cycles.percentile(0.85)) {
            (Some(median), Some(p85)) => (
                stats::span(median),
                format!("p85 {} · {} issues", stats::span(p85), cycles.cycles.len()),
            ),
            _ => ("–".to_string(), "no issue closed on this page".to_string()),
        };
        let drift = stats::cycle_drift(stats);
        return Figure {
            value,
            detail,
            change: drift.and_then(cycle_change),
            since: "on the newer half".to_string(),
            quiet: if drift.is_some() {
                "unchanged"
            } else {
                "too few issues to compare"
            },
        };
    }
    let since = stats
        .snapshots
        .first()
        .map_or(String::new(), |base| format!("since {}", base.short));
    let quiet = "unchanged";
    let (Some(base), Some(now)) = (stats.snapshots.first(), stats.snapshots.last()) else {
        return Figure {
            value: "–".to_string(),
            detail: String::new(),
            change: None,
            since,
            quiet,
        };
    };
    match measure {
        Measure::Complexity => Figure {
            value: measure.show(now.complexity_per_kloc()),
            detail: "per 1,000 code lines".to_string(),
            change: delta(
                now.complexity_per_kloc() - base.complexity_per_kloc(),
                0,
                Up::Worse,
            ),
            since,
            quiet,
        },
        Measure::Tests => Figure {
            value: measure.show(now.test_share() * 100.0),
            detail: format!("{} tests · test lines, not coverage", thousands(now.tests)),
            change: delta(
                (now.test_share() - base.test_share()) * 100.0,
                1,
                Up::Better,
            ),
            since,
            quiet,
        },
        Measure::Code | Measure::Cycle => Figure {
            value: thousands(now.code),
            detail: languages(now),
            change: delta(now.code as f32 - base.code as f32, 0, Up::Neutral),
            since,
            quiet,
        },
    }
}

/// One figure at the page's newest commit, on its own job.
pub(super) fn tile(ui: &mut egui::Ui, blocks: &mut Blocks, measure: Measure) {
    let part = Part::Tile(measure);
    framed(
        ui,
        blocks,
        (part, measure.name(), TILE_H),
        tile_bars,
        |ui, stats, clock| {
            let p = theme::palette(ui);
            let shown = figure(stats, measure);
            block_head(ui, measure.name(), clock);
            ui.label(
                RichText::new(shown.value)
                    .font(egui::FontId::new(24.0, theme::ui_title().family))
                    .color(p.ink),
            );
            let (text, color) = change_line(ui, shown.change, &shown.since, shown.quiet);
            ui.label(RichText::new(text).font(theme::meta()).color(color));
            ui.add(
                egui::Label::new(
                    RichText::new(shown.detail)
                        .font(theme::meta())
                        .color(p.ink_muted),
                )
                .truncate(),
            );
        },
    );
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

/// How the cycle time moved between the page's halves; slower reads worse.
fn cycle_change((before, after): (i64, i64)) -> Option<(String, Up, bool)> {
    let change = after - before;
    let sign = if change > 0 { '+' } else { '−' };
    (change != 0).then(|| {
        let shown = format!("{sign}{}", stats::span(change.abs()));
        (shown, Up::Worse, change > 0)
    })
}

/// A tile's change, coloured by how it reads.
fn change_line(
    ui: &egui::Ui,
    change: Option<(String, Up, bool)>,
    since: &str,
    quiet: &str,
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
        None => (quiet.to_string(), p.ink_faint),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iboard::stats::{Cycle, Cycles};

    fn cycle(short: &str, days: i64) -> Cycle {
        Cycle {
            issue: format!("{short}.md"),
            short: short.into(),
            seconds: days * 86_400,
        }
    }

    fn stats(cycles: Vec<Cycle>) -> Stats {
        Stats {
            snapshots: ["a", "b", "c", "d"]
                .map(|short| Snapshot {
                    short: short.into(),
                    ..Snapshot::default()
                })
                .to_vec(),
            cycles: Cycles { cycles, unknown: 0 },
            ..Stats::default()
        }
    }

    #[test]
    fn the_cycle_tile_compares_the_pages_halves() {
        let slower = figure(&stats(vec![cycle("a", 1), cycle("d", 4)]), Measure::Cycle);
        let (shown, _, rose) = slower.change.unwrap();
        assert_eq!((shown.as_str(), rose), ("+3d 0h", true));
        assert_eq!(slower.since, "on the newer half");

        let faster = figure(&stats(vec![cycle("a", 4), cycle("d", 1)]), Measure::Cycle);
        assert_eq!(faster.change.unwrap().0, "−3d 0h");

        let steady = figure(&stats(vec![cycle("a", 2), cycle("d", 2)]), Measure::Cycle);
        assert!(steady.change.is_none());
        assert_eq!(steady.quiet, "unchanged");

        let lone = figure(&stats(vec![cycle("d", 2)]), Measure::Cycle);
        assert_eq!(lone.quiet, "too few issues to compare");
    }
}
