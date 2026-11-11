//! The project's spend history: one JSON line per finished service session in
//! `$XDG_DATA_HOME/iboard/usage.jsonl`, `~/.local/share/iboard/usage.jsonl`.
//!
//! OpenCode prunes its sessions and a cache is made to be cleared, so this is
//! user data. A per-repo `.git/iboard/` file would go with a clone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domains::agent::runtime::{
    Billing, Cap, Plan, Plans, Provider, Tokens, Usage, Window, provider_key, provider_name,
    status_of,
};
use crate::platform::files;

/// How long a session stays in the log.
const KEEP_DAYS: i64 = 400;

/// The engine the service sessions come from.
const ENGINE: &str = "opencode";

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
        let key = provider_key(provider);
        let start = plan.window.start(now);
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

/// One subscription plan's window: what it used, against the cap it knows.
#[derive(Debug, Clone, PartialEq)]
pub struct Meter {
    /// The normalized provider ID the log and `[usage]` share.
    pub provider: String,
    /// The provider's name, as the status bar shows it.
    pub name: String,
    pub plan: Plan,
    pub consumed: Consumption,
    /// When the plan's window ends, in UTC.
    pub reset: DateTime<Utc>,
}

impl Meter {
    /// The share of the cap the window used, or `None` with no known cap.
    pub fn share(&self) -> Option<f32> {
        match self.plan.cap? {
            Cap::Money(cap) if cap > 0.0 => Some((self.consumed.dollars(&self.plan) / cap) as f32),
            Cap::Tokens(cap) if cap > 0 => Some(self.consumed.tokens as f32 / cap as f32),
            _ => None,
        }
    }
}

/// Every subscription plan, known or declared, with its window's use.
pub fn subscription_meters(plans: &Plans, entries: &[Entry], now: DateTime<Utc>) -> Vec<Meter> {
    let mut keys: Vec<String> = Provider::ALL
        .iter()
        .map(|provider| provider.key().to_string())
        .collect();
    for key in plans.overrides().keys() {
        if !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    keys.into_iter()
        .filter_map(|key| {
            let plan = plans.plan(&key);
            (plan.billing == Billing::Subscription).then(|| Meter {
                name: provider_name(&key),
                consumed: Consumption::of(entries, &key, &plan, now),
                reset: plan.window.reset(now),
                provider: key,
                plan,
            })
        })
        .collect()
}

/// What a board's projects spent, as the log has it: today's and this week's
/// metered dollars, and the last finished session's.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spend {
    /// What today's finished runs were charged.
    pub today: f64,
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
pub(crate) fn record(sessions: &[Value], project: &Path) -> Result<(), String> {
    let entries = finished(sessions, project);
    if entries.is_empty() {
        return Ok(());
    }
    append(&entries)
}

/// The finished sessions of `project` in a service session list.
pub(crate) fn finished(sessions: &[Value], project: &Path) -> Vec<Entry> {
    let project = canonical(project);
    sessions
        .iter()
        .filter_map(|session| Entry::from_session(session, &project))
        .collect()
}

/// Adds `entries`, replacing an earlier line for the same session and dropping
/// what is older than [`KEEP_DAYS`].
pub(crate) fn append(entries: &[Entry]) -> Result<(), String> {
    let path = path().ok_or_else(|| "Couldn't find the data folder.".to_string())?;
    append_at(&path, entries)
}

impl Entry {
    fn from_session(session: &Value, project: &Path) -> Option<Self> {
        let status = status_of(session);
        if status.running {
            return None;
        }
        let id = session["id"].as_str().filter(|id| !id.is_empty())?;
        let directory = session["location"]["directory"].as_str()?;
        let directory = canonical(Path::new(directory));
        if directory != project {
            return None;
        }
        let usage = Usage::from_session(session).unwrap_or_default();
        let ended = ended_of(session, status.created);
        Some(Self {
            session: id.to_string(),
            project: directory,
            engine: ENGINE.to_string(),
            provider: usage.provider,
            model: usage.model,
            billing: usage.billing,
            cost: usage.cost,
            tokens: usage.tokens,
            started: stamp(status.created),
            ended: stamp(ended),
        })
    }
}

/// The last time a finished session wrote anything, in the engine's
/// milliseconds.
fn ended_of(session: &Value, created: u64) -> u64 {
    ["updated", "idle", "busy"]
        .iter()
        .filter_map(|key| session["time"][key].as_u64())
        .max()
        .unwrap_or(created)
        .max(created)
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
    let text = std::fs::read_to_string(path).unwrap_or_default();
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
    use crate::domains::agent::runtime::{Plans, Window};
    use serde_json::json;

    fn project(dir: &Path) -> PathBuf {
        std::fs::canonicalize(dir).unwrap()
    }

    fn base() -> u64 {
        Utc::now().timestamp_millis() as u64 - 3_600_000
    }

    fn session(id: &str, project: &Path, cost: f64, updated: u64) -> Value {
        let at = base();
        json!({
            "id": id,
            "location": {"directory": project},
            "model": {"id": "deepseek-flash", "providerID": "deepseek"},
            "cost": cost,
            "tokens": {"input": 1234, "output": 56, "reasoning": 7,
                       "cache": {"read": 890, "write": 12}},
            "time": {"created": at, "busy": at + 1_000, "idle": at + 2_000,
                     "updated": at + updated},
        })
    }

    fn entry(id: &str, project: &Path, ended: String) -> Entry {
        Entry {
            session: id.to_string(),
            project: project.to_path_buf(),
            engine: ENGINE.to_string(),
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
        let sessions = [session("ses_1", &project, 0.42, 4_000)];

        append_at(&log, &finished(&sessions, &project)).unwrap();

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
        let running = json!({
            "id": "ses_live",
            "location": {"directory": project},
            "time": {"created": 1_000, "busy": 2_000, "idle": 1_500},
        });
        assert!(finished(&[running], &project).is_empty());
    }

    #[test]
    fn a_session_is_deduped_by_its_id() {
        let scratch = tempfile::tempdir().unwrap();
        let project = project(scratch.path());
        let log = scratch.path().join("usage.jsonl");

        append_at(
            &log,
            &finished(&[session("ses_1", &project, 0.1, 4_000)], &project),
        )
        .unwrap();
        append_at(
            &log,
            &finished(&[session("ses_1", &project, 0.3, 5_000)], &project),
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

    fn used(id: &str, provider: &str, cost: f64, at: DateTime<Utc>) -> Entry {
        Entry {
            session: id.to_string(),
            project: PathBuf::from("/p"),
            engine: ENGINE.to_string(),
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
    fn a_subscription_adds_no_spend() {
        let now = noon();
        let mut sub = used("ses_sub", "github-copilot", 3.0, now);
        sub.billing = Billing::Subscription;
        let spend = Spend::of(&[sub], &[PathBuf::from("/p")], now);
        assert_eq!(spend.today, 0.0);
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
