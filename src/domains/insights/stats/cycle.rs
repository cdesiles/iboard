//! Cycle time: how long an issue took, from entering `doing/` to entering
//! `done/`, as the board's file moves in git tell it.
//!
//! A board that skips `doing/` starts at the issue's first commit, and one
//! kept out of git ends at the commit whose `closes <file>.md` line closed it.
//! All follow the issue's slug, so a renumbered or moved file stays the same
//! issue.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, FixedOffset};

use super::PAGE;
use crate::domains::board::{discovery::ColumnKind, priority};
use crate::git;

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
pub(super) fn collect(dir: &Path, root: &Path, newest: &str) -> Result<Cycles, String> {
    let log = git::output(
        dir,
        &[
            "log",
            "--first-parent",
            &format!("--max-count={PAGE}"),
            "--format=%x1e%h%x1f%cI%x1f%B",
            newest,
        ],
    )?;
    let moves = git::output(
        root,
        &[
            "log",
            "--no-renames",
            "--diff-filter=A",
            "--format=%x00%h%x1f%cI",
            "--name-only",
            "--relative",
            "--",
            ".",
        ],
    )
    .map(|log| board_moves(&log))
    .unwrap_or_default();
    let mut ends = closings(&log);
    let page = commits_of(&log);
    for moved in moves.values() {
        if let Some(done) = moved
            .done
            .as_ref()
            .filter(|at| page.contains(&at.short.as_str()))
        {
            ends.push((done.short.clone(), done.date, done.file.clone()));
        }
    }
    let mut seen = Vec::new();
    let mut cycles = Cycles::default();
    for (short, closed, issue) in ends {
        let slug = priority::slug(Path::new(&issue));
        if seen.contains(&slug) {
            continue;
        }
        seen.push(slug.clone());
        let moved = moves.get(&slug);
        let started = moved.and_then(|m| m.doing.as_ref().map(|at| at.date).or(m.added));
        let ended = moved
            .and_then(|m| m.done.as_ref().map(|at| at.date))
            .unwrap_or(closed);
        match started.map(|at| (ended - at).num_seconds()) {
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

/// The short hashes of the page's log.
fn commits_of(log: &str) -> Vec<&str> {
    log.split('\x1e')
        .filter_map(|record| record.split('\x1f').next())
        .map(str::trim)
        .collect()
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

/// A commit that put an issue's file in a column.
#[derive(Debug, Clone, PartialEq)]
struct Entered {
    short: String,
    date: DateTime<FixedOffset>,
    file: String,
}

/// How one issue moved across the board.
#[derive(Debug, Clone, Default, PartialEq)]
struct Moves {
    /// When its file first appeared, anywhere on the board.
    added: Option<DateTime<FixedOffset>>,
    /// When work first started, however often it came back.
    doing: Option<Entered>,
    /// When it last landed, a reopened issue counting its second landing.
    done: Option<Entered>,
}

/// Per slug, the issue's moves, from `git log --no-renames --diff-filter=A
/// --format=%x00%h%x1f%cI --name-only` newest first: each file appearing under
/// a folder is the commit that moved it there.
fn board_moves(log: &str) -> HashMap<String, Moves> {
    let mut moves: HashMap<String, Moves> = HashMap::new();
    let mut current = None;
    for line in log.lines() {
        if let Some(header) = line.strip_prefix('\0') {
            current = header.split_once('\x1f').and_then(|(short, date)| {
                let date = DateTime::parse_from_rfc3339(date.trim()).ok()?;
                Some((short.to_string(), date))
            });
        } else if let Some((short, date)) = &current
            && line.ends_with(".md")
            && !line.split('/').any(|part| part.starts_with('.'))
        {
            let file = line.rsplit('/').next().unwrap_or(line).to_string();
            let moved = moves.entry(priority::slug(Path::new(&file))).or_default();
            moved.added = Some(*date);
            let entered = Entered {
                short: short.clone(),
                date: *date,
                file,
            };
            let column = line.split('/').next().map(Path::new);
            match column.and_then(ColumnKind::of) {
                Some(ColumnKind::Doing) => moved.doing = Some(entered),
                Some(ColumnKind::Done) if moved.done.is_none() => moved.done = Some(entered),
                _ => {}
            }
        }
    }
    moves
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
    fn work_starts_at_the_first_doing_and_ends_at_the_last_done() {
        let log = "\u{0}f5\x1f2026-10-09T10:00:00+00:00\n\ndone/a.md\n\n\
                   \u{0}f4\x1f2026-10-07T10:00:00+00:00\n\ndoing/a.md\n\n\
                   \u{0}f3\x1f2026-10-05T10:00:00+00:00\n\ndone/a.md\n.reviews/b.md\n\n\
                   \u{0}f2\x1f2026-10-03T10:00:00+00:00\n\nwip/002-a.md\n\n\
                   \u{0}f1\x1f2026-09-01T10:00:00+00:00\n\ntodo/a.md\n";
        let moves = board_moves(log);
        let a = &moves["a"];
        let at = |date| DateTime::parse_from_rfc3339(date).unwrap();
        assert_eq!(a.added, Some(at("2026-09-01T10:00:00+00:00")));
        let doing = a.doing.as_ref().unwrap();
        assert_eq!(
            (doing.short.as_str(), doing.date),
            ("f2", at("2026-10-03T10:00:00+00:00"))
        );
        let done = a.done.as_ref().unwrap();
        assert_eq!(
            (done.short.as_str(), done.date),
            ("f5", at("2026-10-09T10:00:00+00:00"))
        );
        assert!(!moves.contains_key("b"));
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
