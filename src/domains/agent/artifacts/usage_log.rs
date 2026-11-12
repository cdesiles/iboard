//! The project's spend history: one JSON line per finished service session in
//! `$XDG_DATA_HOME/iboard/usage.jsonl`, `~/.local/share/iboard/usage.jsonl`.
//!
//! OpenCode prunes its sessions and a cache is made to be cleared, so this is
//! user data. A per-repo `.git/iboard/` file would go with a clone.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::domains::agent::runtime::service::Session;
use crate::domains::agent::runtime::{Billing, Cap, Plan, Quota, Tokens, Window, provider_key};
use crate::platform::files;

/// How long a session stays in the log.
const KEEP_DAYS: i64 = 400;

/// One finished session, as the log keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub session: String,
    /// The canonical path of the project the session ran in.
    pub project: PathBuf,
    pub engine: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub billing: Billing,
    pub cost: Option<f64>,
    pub tokens: Tokens,
    /// RFC 3339 in UTC, as the engine's own session times say.
    pub started: String,
    pub ended: String,
}

/// What a plan's window has used, read from the raw log.
///
/// The log keeps each run's own cost and token counts, so a plan is applied
/// when the history is read: editing it never rewrites a line.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Consumption {
    /// Money metered runs were charged.
    pub spent: f64,
    /// A subscription's API-equivalent dollars, never money taken.
    pub equivalent: f64,
    /// Every token the window's runs used.
    pub tokens: u64,
    /// How many runs the window holds.
    pub runs: usize,
    /// The plan's cap, when it declares one.
    pub cap: Option<Cap>,
}

impl Consumption {
    /// `entries`' runs of `provider` inside `plan`'s window, ending at or
    /// after `now`'s window start.
    pub fn of(entries: &[Entry], provider: &str, plan: &Plan, now: DateTime<Utc>) -> Self {
        Self::since(entries, provider, plan, plan.window.start(now))
    }

    /// [`Consumption::of`] with the window's own start, which a provider's
    /// reset may put off a calendar boundary.
    pub fn since(entries: &[Entry], provider: &str, plan: &Plan, start: DateTime<Utc>) -> Self {
        let key = provider_key(provider);
        let mut used = Consumption {
            cap: plan.cap,
            ..Consumption::default()
        };
        for entry in entries {
            if entry.provider.as_deref().map(provider_key).as_deref() != Some(key.as_str()) {
                continue;
            }
            if instant(&entry.ended).is_none_or(|ended| ended < start) {
                continue;
            }
            used.runs += 1;
            let cost = entry.cost.unwrap_or(0.0);
            match plan.billing {
                Billing::Metered => used.spent += cost,
                Billing::Subscription => used.equivalent += cost,
            }
            used.tokens += entry.tokens.total();
        }
        used
    }

    /// The dollars this plan's window has used: money spent, or a
    /// subscription's API-equivalent.
    pub fn dollars(&self, plan: &Plan) -> f64 {
        match plan.billing {
            Billing::Metered => self.spent,
            Billing::Subscription => self.equivalent,
        }
    }

    /// The dollars this plan's window has used, labelled by what they are.
    pub fn money_label(&self, plan: &Plan) -> String {
        match plan.billing {
            Billing::Metered => format!("${:.2}", self.spent),
            Billing::Subscription => format!("${:.2} API-equivalent", self.equivalent),
        }
    }
}

/// One day of a plan's window, as its daily chart draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayUse {
    pub day: NaiveDate,
    pub tokens: u64,
    pub today: bool,
}

/// One model's share of a plan's window.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelUse {
    pub model: String,
    pub tokens: u64,
    /// Its share of the window's tokens, from 0 to 1.
    pub share: f32,
}

/// One subscription plan's window: what it used, against the cap it knows.
#[derive(Debug, Clone, PartialEq)]
pub struct Meter {
    /// The normalized provider ID the log and `[usage]` share.
    pub provider: String,
    /// The provider's name, as the status bar shows it.
    pub name: String,
    pub plan: Plan,
    pub consumed: Consumption,
    /// The share or count the provider reports; it wins over a cap.
    pub reported: Option<Quota>,
    /// Which window this meter reads: the plan's, or the provider's.
    pub window: Window,
    /// When that window began, in UTC.
    pub start: DateTime<Utc>,
    /// When that window ends, in UTC.
    pub reset: DateTime<Utc>,
    /// The window's runs, for its daily and model split.
    runs: Vec<Entry>,
    /// The moment the meter was read, for its pace and today.
    now: DateTime<Utc>,
}

impl Meter {
    /// The share of the cap the window used, or `None` with no known cap: the
    /// provider's own count first, then a cap the user declared.
    pub fn share(&self) -> Option<f32> {
        if let Some(quota) = &self.reported {
            return quota.share();
        }
        match self.plan.cap? {
            Cap::Money(cap) if cap > 0.0 => Some((self.consumed.dollars(&self.plan) / cap) as f32),
            Cap::Tokens(cap) if cap > 0 => Some(self.consumed.tokens as f32 / cap as f32),
            _ => None,
        }
    }

    /// The share of the window already gone, when it has a share.
    pub fn elapsed(&self) -> Option<f32> {
        self.share()?;
        let length = (self.reset - self.start).num_seconds();
        if length <= 0 {
            return None;
        }
        let gone = (self.now - self.start).num_seconds().clamp(0, length);
        Some(gone as f32 / length as f32)
    }

    /// Whether the window used a larger share than it has lived through.
    pub fn over_pace(&self) -> bool {
        match (self.share(), self.elapsed()) {
            (Some(share), Some(elapsed)) => share > elapsed,
            _ => false,
        }
    }

    /// The weekday (a week's window) or date (a month's) the cap is reached
    /// at the window's average rate so far, when it is over pace.
    pub fn cap_reached_on(&self) -> Option<String> {
        if !self.over_pace() {
            return None;
        }
        let share = self.share()? as f64;
        let at = if share >= 1.0 {
            self.now
        } else {
            let gone = (self.now - self.start).num_seconds() as f64;
            self.now + Duration::seconds((gone * (1.0 - share) / share).round() as i64)
        };
        Some(match self.window {
            Window::Session => at.format("%H:%M").to_string(),
            Window::Week => at.format("%a").to_string(),
            _ => at.format("%b %-d").to_string(),
        })
    }

    /// One entry per day of the window, today flagged and later days empty.
    pub fn per_day(&self) -> Vec<DayUse> {
        let today = self.now.date_naive();
        let mut by_day: HashMap<NaiveDate, u64> = HashMap::new();
        for entry in &self.runs {
            if let Some(ended) = instant(&entry.ended) {
                *by_day.entry(ended.date_naive()).or_default() += entry.tokens.total();
            }
        }
        let last = if self.window == Window::Session {
            self.reset.date_naive() + Duration::days(1)
        } else {
            self.reset.date_naive()
        };
        let mut days = Vec::new();
        let mut day = self.start.date_naive();
        while day < last {
            days.push(DayUse {
                day,
                tokens: by_day.remove(&day).unwrap_or(0),
                today: day == today,
            });
            day += Duration::days(1);
        }
        days
    }

    /// The window's models, the largest first, each with its share of it.
    pub fn by_model(&self) -> Vec<ModelUse> {
        let mut by_model: HashMap<&str, u64> = HashMap::new();
        for entry in &self.runs {
            let model = entry
                .model
                .as_deref()
                .filter(|model| !model.is_empty())
                .unwrap_or("Unknown");
            *by_model.entry(model).or_default() += entry.tokens.total();
        }
        let total = self.consumed.tokens;
        let mut models: Vec<ModelUse> = by_model
            .into_iter()
            .map(|(model, tokens)| ModelUse {
                tokens,
                share: if total == 0 {
                    0.0
                } else {
                    tokens as f32 / total as f32
                },
                model: model.to_string(),
            })
            .collect();
        models.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.model.cmp(&b.model)));
        models
    }
}

mod meters;

pub use meters::{subscription_meters, subscription_meters_with};

/// What a board's projects spent, as the log has it: today's and this week's
/// metered dollars, and the last finished session's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spend {
    /// What today's finished runs were charged.
    pub today: f64,
    /// Today's charge by normalized provider ID, none of them zero.
    pub providers: BTreeMap<String, f64>,
    /// What this week's finished runs were charged.
    pub week: f64,
    /// The last finished session's metered dollars, when there is one.
    pub last: Option<f64>,
}

impl Spend {
    /// `projects`' finished sessions in `entries`, as `now`'s day and week
    /// see them. A subscription's notional dollars are never money spent.
    pub fn of(entries: &[Entry], projects: &[PathBuf], now: DateTime<Utc>) -> Self {
        let projects: Vec<PathBuf> = projects.iter().map(|project| canonical(project)).collect();
        let (day, week) = (Window::Day.start(now), Window::Week.start(now));
        let mut spend = Spend::default();
        let mut last: Option<DateTime<Utc>> = None;
        for entry in entries {
            if !projects.contains(&entry.project) {
                continue;
            }
            let Some(ended) = instant(&entry.ended) else {
                continue;
            };
            let cost = match entry.billing {
                Billing::Metered => entry.cost.unwrap_or(0.0),
                Billing::Subscription => 0.0,
            };
            if ended >= day {
                spend.today += cost;
                if cost > 0.0 {
                    let provider = provider_key(entry.provider.as_deref().unwrap_or("unknown"));
                    *spend.providers.entry(provider).or_default() += cost;
                }
            }
            if ended >= week {
                spend.week += cost;
            }
            if last.is_none_or(|seen| ended >= seen) {
                last = Some(ended);
                spend.last = Some(cost);
            }
        }
        spend
    }
}

/// The log file, when the user has a data folder.
pub fn path() -> Option<PathBuf> {
    files::data_dir().map(|dir| dir.join("usage.jsonl"))
}

/// Every entry, oldest first, with a truncated last line left out.
pub fn read() -> Vec<Entry> {
    path().map_or_else(Vec::new, |path| read_at(&path))
}

/// Every entry in `path`, oldest first. A missing log is empty; one that
/// cannot be read is an error the caller can report and retry.
pub fn read_result(path: &Path) -> Result<Vec<Entry>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(parse(&text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(format!("Couldn't read {}: {err}", path.display())),
    }
}

/// Every entry in `path` of `project`, oldest first.
pub fn read_for_at(path: &Path, project: &Path) -> Result<Vec<Entry>, String> {
    let project = canonical(project);
    Ok(read_result(path)?
        .into_iter()
        .filter(|entry| entry.project == project)
        .collect())
}

/// Every entry in `path`, oldest first: the log of a real run, or a test's.
pub fn read_from(path: &Path) -> Vec<Entry> {
    read_at(path)
}

/// The entries of `project`, oldest first.
pub fn read_for(project: &Path) -> Vec<Entry> {
    let project = canonical(project);
    read()
        .into_iter()
        .filter(|entry| entry.project == project)
        .collect()
}

/// Adds every finished session in `sessions` to the log of `project`.
pub(crate) fn record(sessions: &[Session], project: &Path, engine: &str) -> Result<(), String> {
    let entries = finished(sessions, project, engine);
    if entries.is_empty() {
        return Ok(());
    }
    append(&entries)
}

/// The finished sessions of `project` in a service session list.
pub(crate) fn finished(sessions: &[Session], project: &Path, engine: &str) -> Vec<Entry> {
    let project = canonical(project);
    sessions
        .iter()
        .filter_map(|session| Entry::from(session, &project, engine))
        .collect()
}

/// Adds `entries`, replacing an earlier line for the same session and dropping
/// what is older than [`KEEP_DAYS`].
pub(crate) fn append(entries: &[Entry]) -> Result<(), String> {
    let path = path().ok_or_else(|| "Couldn't find the data folder.".to_string())?;
    append_at(&path, entries)
}

impl Entry {
    fn from(session: &Session, project: &Path, engine: &str) -> Option<Self> {
        if session.status.running || session.id.is_empty() {
            return None;
        }
        let usage = session.usage.clone().unwrap_or_default();
        Some(Self {
            session: session.id.clone(),
            project: project.to_path_buf(),
            engine: engine.to_string(),
            provider: usage.provider,
            model: usage.model,
            billing: usage.billing,
            cost: usage.cost,
            tokens: usage.tokens,
            started: stamp(session.status.created),
            ended: stamp(session.ended),
        })
    }
}

/// The engine's milliseconds as an RFC 3339 timestamp.
fn stamp(millis: u64) -> String {
    let at = DateTime::<Utc>::from_timestamp_millis(millis as i64).unwrap_or_default();
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn instant(stamp: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn retained(entry: &Entry, cutoff: DateTime<Utc>) -> bool {
    instant(&entry.ended).is_none_or(|ended| ended >= cutoff)
}

fn read_at(path: &Path) -> Vec<Entry> {
    read_result(path).unwrap_or_default()
}

fn parse(text: &str) -> Vec<Entry> {
    let mut entries: HashMap<String, Entry> = HashMap::new();
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Entry>(line) else {
            continue;
        };
        entries.insert(entry.session.clone(), entry);
    }
    sorted(entries)
}

fn append_at(path: &Path, entries: &[Entry]) -> Result<(), String> {
    let now = Utc::now();
    let cutoff = now - Duration::days(KEEP_DAYS);
    let mut merged: HashMap<String, Entry> = read_at(path)
        .into_iter()
        .map(|entry| (entry.session.clone(), entry))
        .collect();
    merged.retain(|_, entry| retained(entry, cutoff));
    for entry in entries.iter().filter(|entry| retained(entry, cutoff)) {
        merged.insert(entry.session.clone(), entry.clone());
    }
    let mut text = String::new();
    for entry in sorted(merged) {
        text.push_str(
            &serde_json::to_string(&entry)
                .map_err(|err| format!("Couldn't encode a usage entry: {err}"))?,
        );
        text.push('\n');
    }
    files::write_creating(path, text).map_err(|err| {
        let err = std::io::Error::from(err);
        format!("Couldn't write {}: {err}", path.display())
    })
}

fn sorted(entries: HashMap<String, Entry>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = entries.into_values().collect();
    entries.sort_by(|a, b| {
        (&a.started, &a.ended, &a.session).cmp(&(&b.started, &b.ended, &b.session))
    });
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::runtime::service::Status;
    use crate::domains::agent::runtime::{Plans, Usage, Window};

    fn project(dir: &Path) -> PathBuf {
        std::fs::canonicalize(dir).unwrap()
    }

    fn base() -> u64 {
        Utc::now().timestamp_millis() as u64 - 3_600_000
    }

    fn session(id: &str, cost: f64, updated: u64) -> Session {
        let at = base();
        Session {
            id: id.to_string(),
            slug: None,
            status: Status {
                running: false,
                created: at,
                updated: at + updated,
            },
            ended: at + updated,
            usage: Some(Usage {
                cost: Some(cost),
                tokens: Tokens {
                    input: 1234,
                    output: 56,
                    reasoning: 7,
                    cache_read: 890,
                    cache_write: 12,
                },
                provider: Some("deepseek".to_string()),
                model: Some("deepseek-flash".to_string()),
                billing: Billing::Metered,
            }),
        }
    }

    fn entry(id: &str, project: &Path, ended: String) -> Entry {
        Entry {
            session: id.to_string(),
            project: project.to_path_buf(),
            engine: "opencode".to_string(),
            provider: Some("deepseek".to_string()),
            model: Some("deepseek-flash".to_string()),
            billing: Billing::Metered,
            cost: Some(0.42),
            tokens: Tokens::default(),
            started: ended.clone(),
            ended,
        }
    }

    #[test]
    fn a_finished_session_is_written_with_its_numbers() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");
        let sessions = [session("ses_1", 0.42, 4_000)];

        append_at(&log, &finished(&sessions, &project, "opencode")).unwrap();

        let entries = read_at(&log);
        assert_eq!(entries.len(), 1);
        let seen = &entries[0];
        assert_eq!(seen.session, "ses_1");
        assert_eq!(seen.project, project);
        assert_eq!(seen.engine, "opencode");
        assert_eq!(seen.provider.as_deref(), Some("deepseek"));
        assert_eq!(seen.model.as_deref(), Some("deepseek-flash"));
        assert_eq!(seen.billing, Billing::Metered);
        assert_eq!(seen.cost, Some(0.42));
        assert_eq!(seen.tokens.input, 1234);
        assert_eq!(seen.tokens.cache_read, 890);
        assert!(seen.started < seen.ended);
    }

    #[test]
    fn a_running_session_is_not_written_yet() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let mut running = session("ses_live", 0.0, 2_000);
        running.status.running = true;
        assert!(finished(&[running], &project, "opencode").is_empty());
    }

    #[test]
    fn a_session_is_deduped_by_its_id() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");

        append_at(
            &log,
            &finished(&[session("ses_1", 0.1, 4_000)], &project, "opencode"),
        )
        .unwrap();
        append_at(
            &log,
            &finished(&[session("ses_1", 0.3, 5_000)], &project, "opencode"),
        )
        .unwrap();

        let entries = read_at(&log);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].cost, Some(0.3), "the newer line wins");
    }

    #[test]
    fn a_truncated_last_line_is_ignored() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");
        let whole = entry("ses_1", &project, stamp(4_000));
        std::fs::write(
            &log,
            format!(
                "{}\n{{\"session\":\"ses_2\",",
                serde_json::to_string(&whole).unwrap()
            ),
        )
        .unwrap();

        let entries = read_at(&log);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session, "ses_1");
    }

    #[test]
    fn entries_read_back_oldest_first() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");
        let old = entry("ses_old", &project, stamp(2_000));
        let new = entry("ses_new", &project, stamp(4_000));
        std::fs::write(
            &log,
            format!(
                "{}\n{}\n",
                serde_json::to_string(&new).unwrap(),
                serde_json::to_string(&old).unwrap()
            ),
        )
        .unwrap();

        let entries = read_at(&log);
        let ids: Vec<&str> = entries.iter().map(|entry| entry.session.as_str()).collect();
        assert_eq!(ids, ["ses_old", "ses_new"]);
    }

    #[test]
    fn a_missing_log_reads_as_empty_and_an_unreadable_one_fails() {
        let scratch = tempfile::tempdir().unwrap();
        let missing = scratch.path().join("usage.jsonl");
        assert!(read_result(&missing).unwrap().is_empty());
        let err = read_result(scratch.path()).unwrap_err();
        assert!(err.contains("Couldn't read"), "{err}");
    }

    #[test]
    fn a_projects_entries_are_read_from_a_log_at_a_known_path() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let other = PathBuf::from("/other");
        let log = scratch.path().join("usage.jsonl");
        let mut here = entry("ses_here", &project, stamp(2_000));
        let mut there = entry("ses_there", &other, stamp(3_000));
        here.project = project.clone();
        there.project = other;
        std::fs::write(
            &log,
            format!(
                "{}\n{}\n",
                serde_json::to_string(&there).unwrap(),
                serde_json::to_string(&here).unwrap()
            ),
        )
        .unwrap();
        let entries = read_for_at(&log, scratch.path()).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.session.as_str())
                .collect::<Vec<_>>(),
            ["ses_here"]
        );
    }

    fn used(id: &str, provider: &str, cost: f64, at: DateTime<Utc>) -> Entry {
        Entry {
            session: id.to_string(),
            project: PathBuf::from("/p"),
            engine: "opencode".to_string(),
            provider: Some(provider.to_string()),
            model: None,
            billing: Billing::Metered,
            cost: Some(cost),
            tokens: Tokens {
                input: 10,
                output: 5,
                reasoning: 1,
                cache_read: 2,
                cache_write: 3,
            },
            started: stamp(at.timestamp_millis() as u64),
            ended: stamp(at.timestamp_millis() as u64),
        }
    }

    #[test]
    fn a_metered_plan_reads_the_logs_dollars_as_spend() {
        let now = Utc::now();
        let entries = [
            used("ses_1", "deepseek", 0.25, now),
            used("ses_2", "deepseek", 0.5, now - Duration::hours(1)),
            used("ses_3", "github-copilot", 1.0, now),
        ];
        let plan = Plan::default();
        let consumed = Consumption::of(&entries, "deepseek", &plan, now);
        assert_eq!(consumed.spent, 0.75);
        assert_eq!(consumed.equivalent, 0.0);
        assert_eq!(consumed.tokens, 2 * 21);
        assert_eq!(
            consumed.runs, 2,
            "another provider's run is not this plan's"
        );
        assert_eq!(consumed.money_label(&plan), "$0.75");
    }

    #[test]
    fn a_subscriptions_dollars_are_api_equivalent_never_spend() {
        let now = Utc::now();
        let entries = [used("ses_1", "github-copilot", 1.0, now)];
        let plan = Plans::default().plan("copilot");
        let consumed = Consumption::of(&entries, "github-copilot", &plan, now);
        assert_eq!(consumed.spent, 0.0);
        assert_eq!(consumed.equivalent, 1.0);
        assert_eq!(consumed.money_label(&plan), "$1.00 API-equivalent");
        assert_eq!(consumed.cap, None, "a cap iboard cannot know stays unknown");
    }

    #[test]
    fn editing_a_plan_rereads_the_same_history() {
        let now = Utc::now();
        let entries = [used("ses_1", "deepseek", 0.25, now)];
        let before = entries.clone();
        let metered = Plan::default();
        let subscription = Plan {
            billing: Billing::Subscription,
            ..Plan::default()
        };
        let spent = Consumption::of(&entries, "deepseek", &metered, now);
        let equivalent = Consumption::of(&entries, "deepseek", &subscription, now);
        assert_eq!(spent.spent, 0.25);
        assert_eq!(spent.equivalent, 0.0);
        assert_eq!(equivalent.spent, 0.0);
        assert_eq!(equivalent.equivalent, 0.25);
        assert_eq!(entries, before, "reading a plan never rewrites the log");
    }

    #[test]
    fn a_window_counts_only_the_runs_since_it_began() {
        let now = Utc::now();
        let entries = [
            used("ses_old", "deepseek", 1.0, now - Duration::days(2)),
            used("ses_new", "deepseek", 0.25, now),
        ];
        let plan = Plan {
            window: Window::Day,
            ..Plan::default()
        };
        let consumed = Consumption::of(&entries, "deepseek", &plan, now);
        assert_eq!(consumed.runs, 1);
        assert_eq!(consumed.spent, 0.25);
    }

    #[test]
    fn a_plans_cap_travels_with_its_consumption() {
        let now = Utc::now();
        let entries = [used("ses_1", "deepseek", 1.0, now)];
        let plan = Plan {
            cap: Some(Cap::Money(5.0)),
            ..Plan::default()
        };
        let consumed = Consumption::of(&entries, "deepseek", &plan, now);
        assert_eq!(consumed.cap, Some(Cap::Money(5.0)));
    }

    fn with_tokens(entry: &mut Entry, total: u64) {
        entry.tokens = Tokens {
            input: total,
            ..Tokens::default()
        };
    }

    fn meter_of<'a>(meters: &'a [Meter], provider: &str) -> &'a Meter {
        meters
            .iter()
            .find(|meter| meter.provider == provider)
            .unwrap_or_else(|| panic!("no meter for {provider}"))
    }

    #[test]
    fn a_subscriptions_money_cap_fills_its_ring_with_api_equivalent_dollars() {
        let now = DateTime::parse_from_rfc3339("2026-11-11T15:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut plans = Plans::default();
        plans.set(
            "claude",
            Plan {
                billing: Billing::Subscription,
                window: Window::Week,
                price: None,
                cap: Some(Cap::Money(10.0)),
            },
        );
        let entries = [used("ses_1", "claude", 4.2, now)];
        let meters = subscription_meters(&plans, &entries, now);
        let claude = meter_of(&meters, "claude");
        assert_eq!(claude.name, "Claude.ai");
        assert_eq!(claude.share(), Some(0.42));
        assert_eq!(claude.reset, Window::Week.reset(now));
        assert_eq!(
            claude.consumed.money_label(&claude.plan),
            "$4.20 API-equivalent"
        );
        assert_eq!(
            meters
                .iter()
                .map(|meter| meter.provider.as_str())
                .collect::<Vec<_>>(),
            ["claude", "copilot"],
            "only subscription plans have a meter"
        );
    }

    #[test]
    fn a_subscriptions_token_cap_reads_its_tokens() {
        let now = Utc::now();
        let mut plans = Plans::default();
        plans.set(
            "claude",
            Plan {
                billing: Billing::Subscription,
                window: Window::Day,
                price: None,
                cap: Some(Cap::Tokens(50)),
            },
        );
        let mut entry = used("ses_1", "claude", 1.0, now);
        with_tokens(&mut entry, 21);
        let meters = subscription_meters(&plans, &[entry], now);
        assert_eq!(meter_of(&meters, "claude").share(), Some(0.42));
    }

    #[test]
    fn a_subscription_with_no_cap_has_no_share_but_keeps_its_tokens() {
        let now = Utc::now();
        let mut entry = used("ses_1", "github-copilot", 3.0, now);
        with_tokens(&mut entry, 1_234);
        let meters = subscription_meters(&Plans::default(), &[entry], now);
        let copilot = meter_of(&meters, "copilot");
        assert_eq!(copilot.name, "Copilot");
        assert_eq!(copilot.share(), None, "no cap, no made-up percentage");
        assert_eq!(copilot.consumed.tokens, 1_234);
        assert_eq!(copilot.consumed.dollars(&copilot.plan), 3.0);
    }

    #[test]
    fn a_metered_plan_keeps_its_dollars_and_gets_no_ring() {
        let now = Utc::now();
        let mut plans = Plans::default();
        plans.set(
            "deepseek",
            Plan {
                billing: Billing::Metered,
                window: Window::Day,
                price: None,
                cap: Some(Cap::Money(5.0)),
            },
        );
        let meters = subscription_meters(&plans, &[used("ses_1", "deepseek", 1.0, now)], now);
        assert_eq!(
            meters
                .iter()
                .map(|meter| meter.provider.as_str())
                .collect::<Vec<_>>(),
            ["claude", "copilot"]
        );
    }

    #[test]
    fn a_provider_declared_a_subscription_gets_a_meter_too() {
        let now = Utc::now();
        let mut plans = Plans::default();
        plans.set(
            "acme",
            Plan {
                billing: Billing::Subscription,
                window: Window::Month,
                price: Some(5.0),
                cap: Some(Cap::Tokens(100)),
            },
        );
        let mut entry = used("ses_1", "acme", 0.0, now);
        with_tokens(&mut entry, 25);
        let meters = subscription_meters(&plans, &[entry], now);
        let acme = meter_of(&meters, "acme");
        assert_eq!(acme.name, "acme");
        assert_eq!(acme.share(), Some(0.25));
    }

    #[test]
    fn a_line_older_than_400_days_is_dropped() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");
        let now = Utc::now();
        let stamp = |at: DateTime<Utc>| at.to_rfc3339_opts(SecondsFormat::Millis, true);
        let old = entry(
            "ses_old",
            &project,
            stamp(now - Duration::days(KEEP_DAYS + 1)),
        );
        let recent = entry("ses_recent", &project, stamp(now - Duration::days(1)));
        std::fs::write(
            &log,
            format!(
                "{}\n{}\n",
                serde_json::to_string(&old).unwrap(),
                serde_json::to_string(&recent).unwrap()
            ),
        )
        .unwrap();

        let fresh = entry("ses_fresh", &project, stamp(now));
        append_at(&log, &[fresh]).unwrap();

        let entries = read_at(&log);
        let ids: Vec<&str> = entries.iter().map(|entry| entry.session.as_str()).collect();
        assert_eq!(ids, ["ses_recent", "ses_fresh"]);
    }

    fn noon() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-11T15:30:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn spend_sums_this_day_and_weeks_metered_runs() {
        let now = noon();
        let entries = [
            used("ses_today", "deepseek", 0.25, now),
            used("ses_week", "deepseek", 0.5, now - Duration::days(1)),
            used("ses_old", "deepseek", 1.0, now - Duration::days(30)),
        ];
        let spend = Spend::of(&entries, &[PathBuf::from("/p")], now);
        assert_eq!(spend.today, 0.25);
        assert_eq!(spend.week, 0.75);
        assert_eq!(spend.last, Some(0.25), "the newest run is the last one");
    }

    #[test]
    fn spend_today_is_split_by_provider() {
        let now = noon();
        let entries = [
            used("ses_a", "deepseek", 0.25, now),
            used("ses_b", "deepseek", 0.5, now),
            used("ses_c", "anthropic", 1.0, now),
            used("ses_yesterday", "deepseek", 2.0, now - Duration::days(1)),
        ];
        let spend = Spend::of(&entries, &[PathBuf::from("/p")], now);
        assert_eq!(spend.providers.len(), 2);
        assert_eq!(spend.providers["deepseek"], 0.75);
        assert_eq!(spend.providers["anthropic"], 1.0);
    }

    #[test]
    fn a_subscription_adds_no_spend() {
        let now = noon();
        let mut sub = used("ses_sub", "github-copilot", 3.0, now);
        sub.billing = Billing::Subscription;
        let spend = Spend::of(&[sub], &[PathBuf::from("/p")], now);
        assert_eq!(spend.today, 0.0);
        assert!(spend.providers.is_empty());
        assert_eq!(spend.week, 0.0);
        assert_eq!(spend.last, Some(0.0), "notional dollars are not spend");
    }

    #[test]
    fn spend_of_another_project_is_left_out() {
        let now = noon();
        let mut elsewhere = used("ses_elsewhere", "deepseek", 2.0, now);
        elsewhere.project = PathBuf::from("/other");
        let entries = [used("ses_here", "deepseek", 0.25, now), elsewhere];
        let spend = Spend::of(&entries, &[PathBuf::from("/p")], now);
        assert_eq!(spend.today, 0.25);
        assert_eq!(spend.last, Some(0.25));
    }

    #[test]
    fn a_group_sums_its_projects() {
        let now = noon();
        let mut site = used("ses_site", "deepseek", 0.4, now);
        site.project = PathBuf::from("/site");
        let entries = [used("ses_app", "deepseek", 0.25, now), site];
        let spend = Spend::of(
            &entries,
            &[PathBuf::from("/p"), PathBuf::from("/site")],
            now,
        );
        assert_eq!(spend.today, 0.65);
        assert_eq!(spend.week, 0.65);
    }
}

#[cfg(test)]
mod pace_tests;
#[cfg(test)]
mod quota_tests;
