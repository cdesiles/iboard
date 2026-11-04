//! The four tiles atop Project stats: each figure at the page's newest
//! commit, with its change since the page's oldest.

use super::*;

/// Which way a measure going up goes.
#[derive(Clone, Copy)]
enum Up {
    Better,
    Worse,
    Neutral,
}

/// The four figures at the page's newest commit, each with its change since
/// the page's oldest.
pub(super) fn tiles(ui: &mut egui::Ui, stats: &Stats) {
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
