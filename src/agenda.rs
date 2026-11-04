//! The Agenda's calendar: the commits of each local day as git reads them,
//! the range and the day shown, the grid's weeks with quiet runs folded, and
//! the streak.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{Datelike, Duration as Days, Local, NaiveDate};

use crate::git;

/// How many days the agenda reads, up to today: as many as its widest range
/// shows.
pub const AGENDA_DAYS: i64 = Range::Y1.weeks() * 7;

/// The shortest run of empty weeks folded.
const FOLD_WEEKS: usize = 4;

/// One commit of a day, in the agenda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayCommit {
    /// The author's local `HH:MM`.
    pub time: String,
    pub short: String,
    pub subject: String,
}

/// The commits of each day over the [`AGENDA_DAYS`] up to today, by the
/// author's local date.
pub fn collect(dir: &Path) -> Result<BTreeMap<NaiveDate, Vec<DayCommit>>, String> {
    let start = Local::now() - Days::days(AGENDA_DAYS);
    let out = git::output(
        dir,
        &[
            "log",
            &format!("--since={}", start.to_rfc3339()),
            "--date=format-local:%Y-%m-%d %H:%M",
            "--format=%ad%x1f%h%x1f%s",
            "HEAD",
        ],
    )?;
    let mut days: BTreeMap<NaiveDate, Vec<DayCommit>> = BTreeMap::new();
    for line in out.lines() {
        let mut fields = line.splitn(3, '\x1f');
        let (Some(when), Some(short)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Some((day, time)) = when.split_once(' ') else {
            continue;
        };
        let Ok(day) = NaiveDate::parse_from_str(day, "%Y-%m-%d") else {
            continue;
        };
        days.entry(day).or_default().push(DayCommit {
            time: time.to_string(),
            short: short.to_string(),
            subject: fields.next().unwrap_or_default().to_string(),
        });
    }
    Ok(days)
}

/// How far back the grid goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Range {
    W4,
    #[default]
    W12,
    W26,
    Y1,
}

impl Range {
    pub const ALL: [(Range, &'static str); 4] = [
        (Range::W4, "4w"),
        (Range::W12, "12w"),
        (Range::W26, "26w"),
        (Range::Y1, "1y"),
    ];

    pub const fn weeks(self) -> i64 {
        match self {
            Range::W4 => 4,
            Range::W12 => 12,
            Range::W26 => 26,
            Range::Y1 => 52,
        }
    }

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(range, _)| *range == self)
            .map_or("", |(_, name)| name)
    }

    pub fn wider(self) -> Option<Range> {
        match self {
            Range::W4 => Some(Range::W12),
            Range::W12 => Some(Range::W26),
            Range::W26 => Some(Range::Y1),
            Range::Y1 => None,
        }
    }
}

/// The Agenda's range and the day it shows.
pub struct Agenda {
    pub range: Range,
    pub selected: NaiveDate,
    today: NaiveDate,
}

/// One column of the grid: a week from its Monday, or a run of empty weeks
/// folded into one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Week {
    Days(NaiveDate),
    Folded(NaiveDate, NaiveDate),
}

impl Agenda {
    pub fn new(range: Range) -> Self {
        let today = Local::now().date_naive();
        Self {
            range,
            selected: today,
            today,
        }
    }

    pub fn today(&self) -> NaiveDate {
        self.today
    }

    pub fn first_day(&self) -> NaiveDate {
        monday(self.today) - Days::weeks(self.range.weeks() - 1)
    }

    pub fn set_range(&mut self, range: Range) {
        self.range = range;
        if self.selected < self.first_day() {
            self.selected = self.today;
        }
    }

    pub fn step(&mut self, days: i64) {
        self.selected = (self.selected + Days::days(days)).clamp(self.first_day(), self.today);
    }

    /// The grid's columns, oldest first, with long runs of empty weeks
    /// folded in the wider ranges.
    pub fn weeks(&self, days: &BTreeMap<NaiveDate, Vec<DayCommit>>) -> Vec<Week> {
        let fold = matches!(self.range, Range::W26 | Range::Y1);
        let mondays: Vec<NaiveDate> = (0..self.range.weeks())
            .map(|week| self.first_day() + Days::weeks(week))
            .collect();
        let mut weeks = Vec::new();
        let mut at = 0;
        while at < mondays.len() {
            let quiet = mondays[at..]
                .iter()
                .take_while(|&&monday| week_commits(days, monday) == 0)
                .count();
            if fold && quiet >= FOLD_WEEKS && at + quiet < mondays.len() {
                weeks.push(Week::Folded(
                    mondays[at],
                    mondays[at + quiet - 1] + Days::days(6),
                ));
                at += quiet;
            } else {
                weeks.push(Week::Days(mondays[at]));
                at += 1;
            }
        }
        weeks
    }
}

fn monday(day: NaiveDate) -> NaiveDate {
    day - Days::days(i64::from(day.weekday().num_days_from_monday()))
}

pub fn commits_on(days: &BTreeMap<NaiveDate, Vec<DayCommit>>, day: NaiveDate) -> &[DayCommit] {
    days.get(&day).map_or(&[], Vec::as_slice)
}

fn week_commits(days: &BTreeMap<NaiveDate, Vec<DayCommit>>, monday: NaiveDate) -> usize {
    (0..7)
        .map(|day| commits_on(days, monday + Days::days(day)).len())
        .sum()
}

/// Days in a row with commits, up to today (or yesterday, before the day's
/// first commit).
pub fn streak(days: &BTreeMap<NaiveDate, Vec<DayCommit>>, today: NaiveDate) -> usize {
    let mut day = today;
    if commits_on(days, day).is_empty() {
        day -= Days::days(1);
    }
    let mut count = 0;
    while !commits_on(days, day).is_empty() {
        count += 1;
        day -= Days::days(1);
    }
    count
}

/// How a day reads to a screen reader and in its tooltip.
pub fn day_label(day: NaiveDate, commits: usize) -> String {
    let unit = if commits == 1 { "commit" } else { "commits" };
    format!("{}, {commits} {unit}", day.format("%a %-d %b"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn days(counts: &[(&str, usize)]) -> BTreeMap<NaiveDate, Vec<DayCommit>> {
        counts
            .iter()
            .map(|(text, n)| {
                let commit = DayCommit {
                    time: "10:00".into(),
                    short: "abc1234".into(),
                    subject: "work".into(),
                };
                (day(text), vec![commit; *n])
            })
            .collect()
    }

    fn agenda(range: Range, today: &str) -> Agenda {
        Agenda {
            range,
            selected: day(today),
            today: day(today),
        }
    }

    #[test]
    fn a_streak_runs_back_from_today_or_yesterday() {
        let log = days(&[("2026-10-17", 1), ("2026-10-18", 2), ("2026-10-19", 1)]);
        assert_eq!(streak(&log, day("2026-10-19")), 3);
        assert_eq!(streak(&log, day("2026-10-20")), 3, "today has none yet");
        assert_eq!(streak(&log, day("2026-10-22")), 0);
    }

    #[test]
    fn four_empty_weeks_fold_in_the_wide_ranges_only() {
        let log = days(&[("2026-05-04", 1), ("2026-10-19", 2)]);
        let wide = agenda(Range::W26, "2026-10-20").weeks(&log);
        let folded: Vec<&Week> = wide
            .iter()
            .filter(|w| matches!(w, Week::Folded(..)))
            .collect();
        assert_eq!(
            folded,
            [&Week::Folded(day("2026-05-11"), day("2026-10-18"))],
            "a single empty week stays: {wide:?}"
        );
        let short = agenda(Range::W12, "2026-10-20").weeks(&log);
        assert_eq!(short.len(), 12);
        assert!(short.iter().all(|w| matches!(w, Week::Days(_))));
    }

    #[test]
    fn the_selection_stays_between_the_first_day_and_today() {
        let mut a = agenda(Range::W4, "2026-10-21");
        a.step(1);
        assert_eq!(a.selected, day("2026-10-21"));
        a.step(-100);
        assert_eq!(a.selected, day("2026-09-28"), "the Monday 4 weeks back");
        a.set_range(Range::W12);
        assert_eq!(a.selected, day("2026-09-28"));
    }
}
