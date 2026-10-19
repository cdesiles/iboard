//! Cycle time: how long an issue took, from the start of its work to the
//! commit whose `closes <file>.md` line closed it.
//!
//! Work starts at the issue's first build run in `.builds/`, or else at the
//! first commit that put its file on the board. Both follow the issue's slug,
//! so a renumbered or moved file stays the same issue.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone};

use super::{PAGE, git};
use crate::{build_log, discovery};

/// One issue closed on the page, and how long it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle {
    /// The file name the closing commit names.
    pub issue: String,
    /// The closing commit, abbreviated.
    pub short: String,
    /// From the start of the work to the closing commit, in seconds.
    pub seconds: i64,
}

/// The cycles of the issues closed on the page, slowest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cycles {
    pub cycles: Vec<Cycle>,
    /// Issues closed on the page whose start is unknown.
    pub unknown: usize,
}

impl Cycles {
    /// The cycle time that `share` of the issues stay within.
    pub fn percentile(&self, share: f32) -> Option<i64> {
        let mut seconds: Vec<i64> = self.cycles.iter().map(|c| c.seconds).collect();
        seconds.sort_unstable();
        let last = seconds.len().checked_sub(1)?;
        let at = (last as f32 * share).round() as usize;
        seconds.get(at.min(last)).copied()
    }
}

/// The cycle of each issue closed by the page's commits, up to `newest`,
/// with the issues kept under `root`.
pub(super) fn collect(
    dir: &Path,
    root: &Path,
    page: usize,
    newest: &str,
) -> Result<Cycles, String> {
    let log = git(
        dir,
        &[
            "log",
            "--first-parent",
            &format!("--skip={}", page * PAGE),
            &format!("--max-count={PAGE}"),
            "--format=%x1e%h%x1f%cI%x1f%B",
            newest,
        ],
    )?;
    let added = git(
        root,
        &[
            "-c",
            "core.quotePath=false",
            "log",
            "--diff-filter=A",
            "--format=%x00%cI",
            "--name-only",
            "--relative",
            "--",
            ".",
        ],
    )
    .map(|log| first_added(&log))
    .unwrap_or_default();
    let mut seen = Vec::new();
    let mut cycles = Cycles::default();
    for (short, closed, issue) in closings(&log) {
        let slug = slug(&issue);
        if seen.contains(&slug) {
            continue;
        }
        seen.push(slug.clone());
        let started = [first_run(root, &issue), added.get(&slug).copied()]
            .into_iter()
            .flatten()
            .min();
        match started.map(|at| (closed - at).num_seconds()) {
            Some(seconds) if seconds > 0 => cycles.cycles.push(Cycle {
                issue,
                short,
                seconds,
            }),
            _ => cycles.unknown += 1,
        }
    }
    cycles.cycles.sort_by_key(|c| std::cmp::Reverse(c.seconds));
    Ok(cycles)
}

fn slug(file: &str) -> String {
    let name = discovery::strip_priority(file);
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

/// `(short hash, date, issue file)` for each `closes <file>.md`, newest first.
fn closings(log: &str) -> Vec<(String, DateTime<FixedOffset>, String)> {
    let mut out = Vec::new();
    for record in log.split('\x1e') {
        let mut fields = record.splitn(3, '\x1f');
        let (Some(short), Some(date), Some(body)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(date) = DateTime::parse_from_rfc3339(date.trim()) else {
            continue;
        };
        for line in body.lines() {
            let Some(names) = line.trim().strip_prefix("closes ") else {
                continue;
            };
            for name in names.split([' ', ',']) {
                let name = name.trim_matches(['`', '"', '\'', '(', ')']);
                let name = name
                    .strip_suffix('.')
                    .filter(|name| name.ends_with(".md"))
                    .unwrap_or(name);
                let name = name.rsplit('/').next().unwrap_or(name);
                if name.ends_with(".md") && name.len() > 3 {
                    out.push((short.trim().to_string(), date, name.to_string()));
                }
            }
        }
    }
    out
}

/// Per slug, the first commit that added an issue file, from `git log
/// --diff-filter=A --format=%x00%cI --name-only` newest first.
fn first_added(log: &str) -> HashMap<String, DateTime<FixedOffset>> {
    let mut added = HashMap::new();
    let mut current = None;
    for line in log.lines() {
        if let Some(date) = line.strip_prefix('\0') {
            current = DateTime::parse_from_rfc3339(date.trim()).ok();
        } else if let Some(date) = current
            && line.ends_with(".md")
            && !line.split('/').any(|part| part.starts_with('.'))
        {
            let name = line.rsplit('/').next().unwrap_or(line);
            added.insert(slug(name), date);
        }
    }
    added
}

/// When the build agent first took the issue up, from its log's first heading.
fn first_run(root: &Path, issue: &str) -> Option<DateTime<FixedOffset>> {
    let text = build_log::load(&build_log::path(root, Path::new(issue)))?;
    let heading = text.lines().find_map(|line| line.strip_prefix("── "))?;
    let at = NaiveDateTime::parse_from_str(heading.get(..16)?, "%Y-%m-%d %H:%M").ok()?;
    Some(Local.from_local_datetime(&at).earliest()?.fixed_offset())
}

/// `3d 4h`, `5h 12m`, `12m`.
pub fn span(seconds: i64) -> String {
    let minutes = seconds / 60;
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{}m", minutes.max(1)),
        (0, _) => format!("{hours}h {minutes:02}m"),
        _ => format!("{days}d {hours}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_lines_name_the_issues() {
        let log = "\x1eabc\x1f2026-10-02T18:00:00+02:00\x1fFix it\n\ncloses 001-a.md\n\
                   \x1edef\x1f2026-10-01T10:00:00+00:00\x1fTwo\n\ncloses `b.md`, issues/done/c.md\nnot closes d.md\n";
        let names: Vec<(String, String)> = closings(log)
            .into_iter()
            .map(|(short, _, name)| (short, name))
            .collect();
        assert_eq!(
            names,
            [
                ("abc".into(), "001-a.md".into()),
                ("def".into(), "b.md".into()),
                ("def".into(), "c.md".into()),
            ]
        );
    }

    #[test]
    fn the_first_addition_wins_and_dot_folders_are_skipped() {
        let log = "\u{0}2026-10-02T18:00:00+02:00\n\ndone/002-a.md\n.reviews/b.md\n\n\
                   \u{0}2026-09-01T10:00:00+00:00\n\ntodo/a.md\n";
        let added = first_added(log);
        assert_eq!(
            added["a"],
            DateTime::parse_from_rfc3339("2026-09-01T10:00:00+00:00").unwrap()
        );
        assert!(!added.contains_key("b"));
    }

    #[test]
    fn spans_and_percentiles_read_plainly() {
        assert_eq!(span(30), "1m");
        assert_eq!(span(5 * 3600 + 12 * 60), "5h 12m");
        assert_eq!(span(3 * 86_400 + 4 * 3600), "3d 4h");
        let cycles = Cycles {
            cycles: [40, 10, 30, 20]
                .map(|seconds| Cycle {
                    issue: String::new(),
                    short: String::new(),
                    seconds,
                })
                .to_vec(),
            unknown: 0,
        };
        assert_eq!(cycles.percentile(0.5), Some(30));
        assert_eq!(cycles.percentile(0.85), Some(40));
        assert_eq!(Cycles::default().percentile(0.5), None);
    }
}
