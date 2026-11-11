//! The Agenda's calendar: the commits of each local day as git reads them
//! across the supervised projects, the range and the day shown, the grid's
//! weeks with quiet runs folded, the streak and the per-project weekly rows.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::{Datelike, Duration as Days, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::platform::files;
use crate::platform::git;
use crate::platform::job::Cancel;

/// How many days the agenda reads, up to today: as many as its widest range
/// shows.
pub const AGENDA_DAYS: i64 = Range::Y1.weeks() * 7;

/// The shortest run of empty weeks folded.
const FOLD_WEEKS: usize = 4;

/// The weeks the "By project" rows show, whatever the grid's range.
pub const PROJECT_WEEKS: i64 = 12;

/// One commit of a day, in the agenda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayCommit {
    /// The project it belongs to, as its folder names it.
    pub project: String,
    /// The author's local `HH:MM`.
    pub time: String,
    pub short: String,
    pub subject: String,
}

/// One supervised repository: the name its commits show under, and its root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub name: String,
    pub dir: PathBuf,
}

impl Repo {
    pub fn new(dir: &Path) -> Self {
        Self {
            name: files::file_name(dir),
            dir: dir.to_path_buf(),
        }
    }
}

/// The projects the Agenda counts: the open one first, then the recent ones
/// that are still folders, each once.
pub fn supervised(open: &Path, recents: &[PathBuf]) -> Vec<Repo> {
    let mut seen = Vec::new();
    let mut repos = Vec::new();
    for dir in std::iter::once(open).chain(recents.iter().map(PathBuf::as_path)) {
        if !dir.is_dir() {
            continue;
        }
        let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if seen.contains(&canonical) {
            continue;
        }
        seen.push(canonical);
        repos.push(Repo::new(dir));
    }
    repos
}

/// What the Agenda read from every supervised project.
#[derive(Debug, Clone, Default)]
pub struct AgendaDays {
    /// The commits of each local day, newest first, over the [`AGENDA_DAYS`]
    /// up to today.
    pub days: BTreeMap<NaiveDate, Vec<DayCommit>>,
    /// The projects git could read, in the order they were scanned.
    pub projects: Vec<String>,
    /// The projects it could not read, and why.
    pub failed: Vec<(String, String)>,
}

impl AgendaDays {
    /// The commits of `day`, newest first, from `project` alone when it is
    /// set.
    pub fn commits_on<'a>(
        &'a self,
        day: NaiveDate,
        project: Option<&'a str>,
    ) -> impl Iterator<Item = &'a DayCommit> + 'a {
        let commits = self.days.get(&day).map_or(&[][..], Vec::as_slice);
        commits
            .iter()
            .filter(move |commit| project.is_none_or(|name| commit.project == name))
    }

    pub fn count_on(&self, day: NaiveDate, project: Option<&str>) -> usize {
        self.commits_on(day, project).count()
    }

    /// The newest commit it read, and its project. `None` when every project
    /// is empty.
    pub fn last(&self, project: Option<&str>) -> Option<(NaiveDate, &DayCommit)> {
        self.days.iter().rev().find_map(|(day, commits)| {
            let newest = commits
                .iter()
                .filter(|commit| project.is_none_or(|name| commit.project == name))
                .max_by(|a, b| a.time.cmp(&b.time))?;
            Some((*day, newest))
        })
    }

    /// How many projects committed between `first` and `today`, both
    /// included, from `project` alone when it is set.
    pub fn projects_in(&self, first: NaiveDate, today: NaiveDate, project: Option<&str>) -> usize {
        let mut names = BTreeSet::new();
        for commits in self.days.range(first..=today).map(|(_, commits)| commits) {
            for commit in commits {
                if project.is_none_or(|name| commit.project == name) {
                    names.insert(commit.project.as_str());
                }
            }
        }
        names.len()
    }
}

/// The commits of each day over the [`AGENDA_DAYS`] up to today, by the
/// author's local date, under `project`.
pub fn collect(dir: &Path, project: &str) -> Result<BTreeMap<NaiveDate, Vec<DayCommit>>, String> {
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
            project: project.to_string(),
            time: time.to_string(),
            short: short.to_string(),
            subject: fields.next().unwrap_or_default().to_string(),
        });
    }
    Ok(days)
}

/// Reads every supervised project. A project git cannot read is kept with
/// its error and the others still count; `cancel` stops between projects.
pub fn collect_all(repos: &[Repo], cancel: &Cancel) -> Result<AgendaDays, String> {
    let mut out = AgendaDays::default();
    for repo in repos {
        if cancel.is_cancelled() {
            return Err("stopped".to_string());
        }
        match collect(&repo.dir, &repo.name) {
            Ok(days) => {
                out.projects.push(repo.name.clone());
                for (day, commits) in days {
                    out.days.entry(day).or_default().extend(commits);
                }
            }
            Err(error) => out.failed.push((repo.name.clone(), error)),
        }
    }
    for commits in out.days.values_mut() {
        commits.sort_by(|a, b| b.time.cmp(&a.time));
    }
    Ok(out)
}

/// One row of "By project": the weeks' counts, oldest first, the total and
/// the last commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRow {
    pub name: String,
    pub weeks: Vec<usize>,
    pub total: usize,
    pub last: Option<NaiveDate>,
}

/// The supervised projects' [`PROJECT_WEEKS`] of counts, busiest first.
pub fn project_rows(days: &AgendaDays, today: NaiveDate) -> Vec<ProjectRow> {
    let first = monday(today) - Days::weeks(PROJECT_WEEKS - 1);
    let mut rows: Vec<ProjectRow> = days
        .projects
        .iter()
        .map(|name| ProjectRow {
            name: name.clone(),
            weeks: vec![0; PROJECT_WEEKS as usize],
            total: 0,
            last: None,
        })
        .collect();
    let index: BTreeMap<&str, usize> = days
        .projects
        .iter()
        .enumerate()
        .map(|(at, name)| (name.as_str(), at))
        .collect();
    for (day, commits) in &days.days {
        for commit in commits {
            let Some(&at) = index.get(commit.project.as_str()) else {
                continue;
            };
            let row = &mut rows[at];
            row.last = Some(row.last.map_or(*day, |last| last.max(*day)));
            if (first..=today).contains(day) {
                let week = ((*day - first).num_days() / 7) as usize;
                row.weeks[week] += 1;
                row.total += 1;
            }
        }
    }
    rows.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));
    rows
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

/// The `[agenda]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
pub struct AgendaSection {
    range: Option<String>,
}

impl AgendaSection {
    /// The range the Agenda opens on; 12w unless the section says otherwise.
    /// An unknown one is reported.
    pub fn range(self) -> (Range, Vec<String>) {
        let Some(name) = self.range else {
            return (Range::default(), Vec::new());
        };
        match Range::ALL.iter().find(|(_, known)| *known == name.trim()) {
            Some((range, _)) => (*range, Vec::new()),
            None => (
                Range::default(),
                vec![format!(
                    "[agenda]: range = \"{name}\" is not 4w, 12w, 26w or 1y"
                )],
            ),
        }
    }
}

/// The `[agenda]` section that stores `range`.
pub fn range_section(range: Range) -> AgendaSection {
    AgendaSection {
        range: Some(range.name().to_string()),
    }
}

/// The Agenda's range, the day it shows and the project it is filtered to.
pub struct Agenda {
    pub range: Range,
    pub selected: NaiveDate,
    today: NaiveDate,
    filter: Option<String>,
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
            filter: None,
        }
    }

    pub fn today(&self) -> NaiveDate {
        self.today
    }

    /// The project the grid, the summary and the day panel are filtered to.
    pub fn filter(&self) -> Option<&str> {
        self.filter.as_deref()
    }

    /// Filters on `project`, or shows all of them again when it is already
    /// the one filtered.
    pub fn toggle_filter(&mut self, project: &str) {
        if self.filter.as_deref() == Some(project) {
            self.filter = None;
        } else {
            self.filter = Some(project.to_string());
        }
    }

    pub fn clear_filter(&mut self) {
        self.filter = None;
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
    /// folded in the wider ranges. The current week stays: it holds today.
    pub fn weeks(&self, days: &AgendaDays) -> Vec<Week> {
        let fold = matches!(self.range, Range::W26 | Range::Y1);
        let mondays: Vec<NaiveDate> = (0..self.range.weeks())
            .map(|week| self.first_day() + Days::weeks(week))
            .collect();
        let mut weeks = Vec::new();
        let mut at = 0;
        while at < mondays.len() {
            let quiet = mondays[at..mondays.len() - 1]
                .iter()
                .take_while(|&&monday| week_commits(days, monday, self.filter()) == 0)
                .count();
            if fold && quiet >= FOLD_WEEKS {
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

fn week_commits(days: &AgendaDays, monday: NaiveDate, project: Option<&str>) -> usize {
    (0..7)
        .map(|day| days.count_on(monday + Days::days(day), project))
        .sum()
}

/// Days in a row with commits, up to today (or yesterday, before the day's
/// first commit).
pub fn streak(days: &AgendaDays, today: NaiveDate, project: Option<&str>) -> usize {
    let mut day = today;
    if days.count_on(day, project) == 0 {
        day -= Days::days(1);
    }
    let mut count = 0;
    while days.count_on(day, project) > 0 {
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

    fn days(counts: &[(&str, usize)]) -> AgendaDays {
        let mut log = AgendaDays {
            projects: vec!["iboard".to_string()],
            ..AgendaDays::default()
        };
        for (text, n) in counts {
            let commit = DayCommit {
                project: "iboard".into(),
                time: "10:00".into(),
                short: "abc1234".into(),
                subject: "work".into(),
            };
            log.days.insert(day(text), vec![commit; *n]);
        }
        log
    }

    fn agenda(range: Range, today: &str) -> Agenda {
        Agenda {
            range,
            selected: day(today),
            today: day(today),
            filter: None,
        }
    }

    #[test]
    fn a_streak_runs_back_from_today_or_yesterday() {
        let log = days(&[("2026-10-17", 1), ("2026-10-18", 2), ("2026-10-19", 1)]);
        assert_eq!(streak(&log, day("2026-10-19"), None), 3);
        assert_eq!(
            streak(&log, day("2026-10-20"), None),
            3,
            "today has none yet"
        );
        assert_eq!(streak(&log, day("2026-10-22"), None), 0);
    }

    #[test]
    fn a_project_filter_counts_only_that_project() {
        let mut log = days(&[("2026-10-18", 2), ("2026-10-19", 1)]);
        let other = DayCommit {
            project: "site".into(),
            time: "11:00".into(),
            short: "def5678".into(),
            subject: "other".into(),
        };
        log.days.get_mut(&day("2026-10-19")).unwrap().push(other);
        log.projects.push("site".into());
        assert_eq!(log.count_on(day("2026-10-19"), None), 2);
        assert_eq!(log.count_on(day("2026-10-19"), Some("site")), 1);
        assert_eq!(streak(&log, day("2026-10-20"), Some("site")), 1);
        assert_eq!(
            log.projects_in(day("2026-10-18"), day("2026-10-20"), None),
            2
        );
    }

    #[test]
    fn the_last_commit_of_a_filter_looks_past_quiet_days() {
        let mut log = days(&[("2026-10-19", 1)]);
        log.projects.push("site".into());
        log.days.insert(
            day("2026-10-18"),
            vec![DayCommit {
                project: "site".into(),
                time: "11:00".into(),
                short: "def5678".into(),
                subject: "other".into(),
            }],
        );
        assert_eq!(log.last(None).map(|(day, _)| day), Some(day("2026-10-19")));
        assert_eq!(
            log.last(Some("site")).map(|(day, _)| day),
            Some(day("2026-10-18"))
        );
    }

    #[test]
    fn the_project_rows_hold_twelve_weeks_busiest_first() {
        let mut log = days(&[("2026-10-14", 3), ("2026-10-19", 1)]);
        log.projects.push("site".into());
        log.days
            .get_mut(&day("2026-10-19"))
            .unwrap()
            .push(DayCommit {
                project: "site".into(),
                time: "11:00".into(),
                short: "def5678".into(),
                subject: "other".into(),
            });
        let rows = project_rows(&log, day("2026-10-20"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "iboard");
        assert_eq!(rows[0].total, 4);
        assert_eq!(rows[0].weeks.iter().sum::<usize>(), 4);
        assert_eq!(rows[0].last, Some(day("2026-10-19")));
        assert_eq!(rows[1].name, "site");
        assert_eq!(rows[1].total, 1);
    }

    #[test]
    fn supervised_starts_with_the_open_project_and_skips_gone_folders() {
        let root = tempfile::tempdir().unwrap();
        let (open, recent) = (root.path().join("open"), root.path().join("recent"));
        std::fs::create_dir_all(&open).unwrap();
        std::fs::create_dir_all(&recent).unwrap();
        let gone = root.path().join("gone");
        let repos = supervised(&open, &[gone, recent.clone(), open.clone()]);
        assert_eq!(
            repos
                .iter()
                .map(|repo| repo.name.as_str())
                .collect::<Vec<_>>(),
            ["open", "recent"]
        );
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
    fn trailing_empty_weeks_fold_up_to_the_current_week() {
        let log = days(&[("2026-05-04", 1)]);
        let weeks = agenda(Range::W26, "2026-10-20").weeks(&log);
        assert_eq!(
            weeks[weeks.len() - 2..],
            [
                Week::Folded(day("2026-05-11"), day("2026-10-18")),
                Week::Days(day("2026-10-19")),
            ],
        );
        let short_run = days(&[("2026-09-28", 1)]);
        let weeks = agenda(Range::W26, "2026-10-20").weeks(&short_run);
        assert!(
            weeks[weeks.len() - 3..]
                .iter()
                .all(|week| matches!(week, Week::Days(_))),
            "three empty weeks stay: {weeks:?}"
        );
    }

    #[test]
    fn the_range_is_read_from_its_section_and_written_back() {
        let parse = |text: &str| -> AgendaSection {
            let document: toml::Table = toml::from_str(text).unwrap();
            document
                .get("agenda")
                .and_then(|value| value.clone().try_into().ok())
                .unwrap_or_default()
        };
        assert_eq!(parse("").range().0, Range::W12);
        assert_eq!(parse("[agenda]\nrange = \"1y\"\n").range().0, Range::Y1);
        let (range, problems) = parse("[agenda]\nrange = \"2w\"\n").range();
        assert_eq!(range, Range::W12);
        assert_eq!(
            problems,
            ["[agenda]: range = \"2w\" is not 4w, 12w, 26w or 1y"]
        );
        assert_eq!(range_section(Range::W26).range().0, Range::W26);
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
