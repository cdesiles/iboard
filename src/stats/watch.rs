//! The Refactor watch: four signals read over a page of history against
//! [`Limits`], the rule that two of them make a refactor due, and the issue
//! they draft.

use super::{Cycles, FileStat, HOTSPOT_CHANGES, HOTSPOT_COMPLEXITY, Stats, span};
use crate::format::thousands;

/// How many signals on make a refactor due.
const DUE_AT: usize = 2;

/// Where the Refactor watch's signals turn on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Complexity per thousand code lines above which a refactor is due.
    pub complexity: f32,
    /// A rise in complexity per kLOC over the page that turns its signal on.
    pub complexity_growth: f32,
    /// A fall of the test share, in points, that turns its signal on.
    pub tests_slide: f32,
    /// How much slower the page's newer issues may close before cycle time
    /// drifts.
    pub cycle_drift: f32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            complexity: 130.0,
            complexity_growth: 0.05,
            tests_slide: 3.0,
            cycle_drift: 1.5,
        }
    }
}

/// One reason to plan a refactor, on or off, with its numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub name: &'static str,
    pub on: bool,
    pub detail: String,
}

/// The four signals of the Refactor watch, from the page's oldest commit to
/// its newest.
pub fn signals(stats: &Stats, limits: &Limits) -> Vec<Signal> {
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
            on: growth > limits.complexity_growth || is > limits.complexity,
            detail: format!(
                "{was:.0} → {is:.0} per kLOC ({:+.0}%), limit {:.0}",
                growth * 100.0,
                limits.complexity
            ),
        },
        Signal {
            name: "Tests are sliding",
            on: slide >= limits.tests_slide,
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
                "{hot} files with complexity ≥ {HOTSPOT_COMPLEXITY} and ≥ {HOTSPOT_CHANGES} changes"
            ),
        },
        Signal {
            name: "Cycle time drifts",
            on: drift
                .is_some_and(|(before, after)| after as f32 > before as f32 * limits.cycle_drift),
            detail: drift.map_or(
                "too few issues closed to tell".to_string(),
                |(before, after)| {
                    format!(
                        "median {} on the newer half, {} before",
                        span(after),
                        span(before)
                    )
                },
            ),
        },
    ]
}

/// Enough signals are on to plan a refactor.
pub fn refactor_due(signals: &[Signal]) -> bool {
    signals.iter().filter(|s| s.on).count() >= DUE_AT
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
pub fn culprits(files: &[FileStat]) -> Vec<&FileStat> {
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
pub fn refactor_draft(stats: &Stats, signals: &[Signal]) -> (String, String) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{Cycle, Snapshot};

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
        let signals = signals(&stats, &Limits::default());
        let on: Vec<&str> = signals.iter().filter(|s| s.on).map(|s| s.name).collect();
        assert_eq!(
            on,
            [
                "Complexity grows faster than code",
                "Tests are sliding",
                "Hotspots",
                "Cycle time drifts"
            ]
        );
        assert!(refactor_due(&signals[..2]));
        assert!(!refactor_due(&signals[..1]));
        let (title, body) = refactor_draft(&stats, &signals);
        assert_eq!(title, "[refactor] Plan a refactor");
        assert!(
            body.contains("- `src/app.rs`: 400 lines, complexity 90, 12 changes"),
            "{body}"
        );
        assert!(!body.contains("src/lib.rs"), "{body}");
    }
}
