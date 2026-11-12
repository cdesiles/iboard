//! The usage screen's read model: what each provider burnt, a day at a time,
//! and what that adds up to over the last week, month or quarter.
//!
//! The log is the project's sessions in the tool iboard drives, not the
//! provider's bill: a key shared with another machine stays invisible. A
//! metered run's dollars are money spent; a subscription's are API-equivalent
//! and never join the spend total.
//!
//! [`UsageView`] keeps the days a chart draws, the model and project totals of
//! each range and its previous one, and, when a subscription reports one, the
//! plan window it reports. The range, the selected provider and the measure
//! are the screen's state, so every range is computed here once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};

use crate::domains::agent::artifacts::usage_log::{Consumption, Entry, Meter};
use crate::domains::agent::runtime::{
    Billing, Plan, Provider, Quota, Window, provider_key, provider_name,
};
use crate::kernel::labels::Hue;
use crate::platform::files;

/// How many days the log is read into, up to today: twice the widest range, so
/// its previous period is whole.
pub const DAYS: i64 = 180;

/// The ranges the screen offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Range {
    D7,
    #[default]
    D30,
    D90,
}

impl Range {
    /// Every range, narrowest first.
    pub const ALL: [Range; 3] = [Range::D7, Range::D30, Range::D90];
    pub const WEEK: Range = Range::D7;
    pub const MONTH: Range = Range::D30;
    pub const QUARTER: Range = Range::D90;

    /// How many days the range holds, today included.
    pub const fn days(self) -> i64 {
        match self {
            Range::D7 => 7,
            Range::D30 => 30,
            Range::D90 => 90,
        }
    }

    /// The word the segmented control uses.
    pub const fn label(self) -> &'static str {
        match self {
            Range::D7 => "7d",
            Range::D30 => "30d",
            Range::D90 => "90d",
        }
    }

    /// [`Range::label`], as a range's configuration name.
    pub const fn name(self) -> &'static str {
        self.label()
    }

    /// The range ending on `today`: its first and last day, both included.
    pub fn span(self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        (today - Duration::days(self.days() - 1), today)
    }

    /// The range of equal length just before the one ending on `today`.
    pub fn previous_span(self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let days = self.days();
        (
            today - Duration::days(2 * days - 1),
            today - Duration::days(days),
        )
    }
}

/// What a provider used over a range.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Totals {
    pub tokens: u64,
    pub runs: usize,
    /// Money metered runs were charged.
    pub spent: f64,
    /// A subscription's API-equivalent dollars, never money taken.
    pub equivalent: f64,
}

impl Totals {
    /// Whether the range holds a run at all.
    pub fn is_empty(&self) -> bool {
        self.runs == 0
    }

    /// The dollars this provider's runs used: money spent, or a subscription's
    /// API-equivalent.
    pub fn dollars(&self, billing: Billing) -> f64 {
        match billing {
            Billing::Metered => self.spent,
            Billing::Subscription => self.equivalent,
        }
    }

    /// The tokens' change against `previous`, as a signed percentage. `None`
    /// with no run in the previous range: nothing to compare against.
    pub fn tokens_change(&self, previous: &Totals) -> Option<f32> {
        if previous.runs == 0 || previous.tokens == 0 {
            return None;
        }
        Some((self.tokens as f32 - previous.tokens as f32) / previous.tokens as f32 * 100.0)
    }

    /// [`Totals::tokens_change`], as a fraction rather than a percentage.
    pub fn tokens_ratio(&self, previous: &Totals) -> Option<f32> {
        self.tokens_change(previous).map(|change| change / 100.0)
    }
}

/// One day's runs, provider by provider. A day without a run is absent.
#[derive(Debug, Clone, PartialEq)]
pub struct DayUsage {
    pub day: NaiveDate,
    pub tokens: u64,
    pub runs: usize,
    /// Money metered runs were charged.
    pub spent: f64,
    /// A subscription's API-equivalent dollars.
    pub equivalent: f64,
}

impl DayUsage {
    /// The day's dollars for a provider's billing kind.
    pub fn dollars(&self, billing: Billing) -> f64 {
        match billing {
            Billing::Metered => self.spent,
            Billing::Subscription => self.equivalent,
        }
    }
}

/// One model's share of a range.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelUsage {
    /// The model as the log names it; `Unknown` when it names none.
    pub model: String,
    /// [`ModelUsage::model`] under the list's common name column.
    pub name: String,
    pub tokens: u64,
    pub runs: usize,
    pub spent: f64,
    pub equivalent: f64,
    /// Its share of the range's tokens, from 0 to 1.
    pub share: f32,
}

/// One project's share of a range.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectUsage {
    /// The project folder's name, as the screen lists it.
    pub project: String,
    /// [`ProjectUsage::project`] under the list's common name column.
    pub name: String,
    pub path: PathBuf,
    pub tokens: u64,
    pub runs: usize,
    pub spent: f64,
    pub equivalent: f64,
    /// Its share of the range's tokens, from 0 to 1.
    pub share: f32,
}

/// What one provider used over one range, and the range before it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RangeUsage {
    pub range: Range,
    pub totals: Totals,
    pub previous: Totals,
    /// The range's days that hold a run, oldest first.
    pub days: Vec<DayUsage>,
    /// The largest token day of the range.
    pub busiest: Option<DayUsage>,
    /// The largest token day of the range before it.
    pub previous_busiest: Option<DayUsage>,
    /// Largest first, ties by name.
    pub models: Vec<ModelUsage>,
    pub previous_models: Vec<ModelUsage>,
    pub projects: Vec<ProjectUsage>,
    pub previous_projects: Vec<ProjectUsage>,
    /// The tokens' change against the previous range, in percent.
    pub change: Option<f32>,
}

impl RangeUsage {
    /// Whether the range holds a run.
    pub fn is_empty(&self) -> bool {
        self.totals.is_empty()
    }

    /// The tokens' change against the previous range, as a signed percentage.
    pub fn tokens_change(&self) -> Option<f32> {
        self.change
    }

    /// [`RangeUsage::tokens_change`] under the tile's shorter name.
    pub fn delta(&self) -> Option<f32> {
        self.change
    }

    /// Whether the previous range holds a run to compare against.
    pub fn has_previous(&self) -> bool {
        !self.previous.is_empty()
    }
}

/// A subscription's plan window, as its meter reports it. The share and the
/// pace are the meter's rules, unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanWindow {
    pub provider: String,
    pub name: String,
    pub window: Window,
    /// The share of the window used.
    pub share: Option<f32>,
    /// The share of the window already gone.
    pub pace: Option<f32>,
    /// When the window resets.
    pub reset: DateTime<Utc>,
    /// When the window began.
    pub start: DateTime<Utc>,
    pub over_pace: bool,
    /// The weekday or date the cap is reached at the current pace.
    pub cap_reached_on: Option<String>,
    /// The provider's own report, when it made one.
    pub reported: Option<Quota>,
    pub plan: Plan,
    pub consumed: Consumption,
}

impl PlanWindow {
    /// The window `meter` reports.
    pub fn of(meter: &Meter) -> Self {
        Self {
            provider: meter.provider.clone(),
            name: meter.name.clone(),
            window: meter.window,
            share: meter.share(),
            pace: meter.elapsed(),
            reset: meter.reset,
            start: meter.start,
            over_pace: meter.over_pace(),
            cap_reached_on: meter.cap_reached_on(),
            reported: meter.reported.clone(),
            plan: meter.plan,
            consumed: meter.consumed,
        }
    }

    /// [`PlanWindow::of`] under the constructor's common name.
    pub fn from_meter(meter: &Meter) -> Self {
        Self::of(meter)
    }
}

/// A provider as the tool lists it, before the log adds its runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedProvider {
    /// The normalized provider ID the log and `[usage]` share.
    pub provider: String,
    pub name: String,
    pub billing: Billing,
    pub hue: Hue,
}

impl ListedProvider {
    /// The provider `provider` as the tool lists it, with its default billing.
    pub fn new(provider: impl AsRef<str>, billing: Billing, hue: Hue) -> Self {
        let provider = provider_key(provider.as_ref());
        Self {
            name: provider_name(&provider),
            provider,
            billing,
            hue,
        }
    }

    /// A known provider with its default plan and fixed hue.
    pub fn from_provider(provider: Provider) -> Self {
        let key = provider.key();
        Self {
            provider: key.to_string(),
            name: provider.name().to_string(),
            billing: Plan::default_for(key).billing,
            hue: hue_for(key).unwrap_or(Hue::Grey),
        }
    }
}

/// A type a caller may hand over as one of the tool's providers.
pub trait Listed {
    fn listed(&self) -> ListedProvider;
}

impl Listed for ListedProvider {
    fn listed(&self) -> ListedProvider {
        self.clone()
    }
}

impl Listed for Provider {
    fn listed(&self) -> ListedProvider {
        ListedProvider::from_provider(*self)
    }
}

impl Listed for &str {
    fn listed(&self) -> ListedProvider {
        let key = provider_key(self);
        ListedProvider {
            name: provider_name(&key),
            billing: Plan::default_for(&key).billing,
            hue: hue_for(&key).unwrap_or(Hue::Grey),
            provider: key,
        }
    }
}

impl Listed for String {
    fn listed(&self) -> ListedProvider {
        self.as_str().listed()
    }
}

impl Listed for (String, Billing) {
    fn listed(&self) -> ListedProvider {
        let key = provider_key(&self.0);
        ListedProvider {
            name: provider_name(&key),
            billing: self.1,
            hue: hue_for(&key).unwrap_or(Hue::Grey),
            provider: key,
        }
    }
}

/// One provider's consumption, by day and by range.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderUsage {
    /// The normalized provider ID the log, `[usage]` and the rail share.
    pub provider: String,
    pub name: String,
    pub billing: Billing,
    /// Its fixed label hue, or the next free one for a new provider.
    pub hue: Hue,
    /// The days of the last [`DAYS`] that hold a run, oldest first.
    pub days: Vec<DayUsage>,
    /// Every range the screen offers.
    pub ranges: BTreeMap<Range, RangeUsage>,
    /// The plan windows the provider reported, the displayed one first.
    pub plans: Vec<PlanWindow>,
    /// The plan window the screen shows, when one was reported.
    pub plan: Option<PlanWindow>,
    /// The default [`Range::D30`] totals, for a caller that has no range yet.
    pub totals: Totals,
    pub previous: Totals,
    pub days_in_range: Vec<DayUsage>,
    pub busiest: Option<DayUsage>,
    pub models: Vec<ModelUsage>,
    pub projects: Vec<ProjectUsage>,
    pub previous_models: Vec<ModelUsage>,
    pub previous_projects: Vec<ProjectUsage>,
    pub change: Option<f32>,
}

impl ProviderUsage {
    fn empty(provider: String, name: String, billing: Billing, hue: Hue) -> Self {
        let mut ranges = BTreeMap::new();
        for range in Range::ALL {
            ranges.insert(
                range,
                RangeUsage {
                    range,
                    ..RangeUsage::default()
                },
            );
        }
        let d30 = ranges.get(&Range::D30).cloned().unwrap_or_default();
        Self {
            provider,
            name,
            billing,
            hue,
            days: Vec::new(),
            ranges,
            plans: Vec::new(),
            plan: None,
            totals: d30.totals,
            previous: d30.previous,
            days_in_range: d30.days.clone(),
            busiest: d30.busiest,
            models: d30.models,
            projects: d30.projects,
            previous_models: d30.previous_models,
            previous_projects: d30.previous_projects,
            change: d30.change,
        }
    }

    /// The range's totals, its previous one and its splits.
    pub fn range(&self, range: Range) -> &RangeUsage {
        self.ranges
            .get(&range)
            .unwrap_or_else(|| self.ranges.values().next().unwrap_or_else(|| empty_range()))
    }

    /// Whether the provider ran at all in the last [`DAYS`].
    pub fn has_run(&self) -> bool {
        !self.days.is_empty()
    }

    /// Whether the provider ran in `range`.
    pub fn has_run_in(&self, range: Range) -> bool {
        !self.range(range).is_empty()
    }

    /// The range's totals.
    pub fn totals(&self, range: Range) -> &Totals {
        &self.range(range).totals
    }

    /// The range before it, of equal length.
    pub fn previous_totals(&self, range: Range) -> &Totals {
        &self.range(range).previous
    }

    /// The range's models, largest first.
    pub fn models(&self, range: Range) -> &[ModelUsage] {
        &self.range(range).models
    }

    /// The range's projects, largest first.
    pub fn projects(&self, range: Range) -> &[ProjectUsage] {
        &self.range(range).projects
    }

    /// The range's busiest token day.
    pub fn busiest(&self, range: Range) -> Option<&DayUsage> {
        self.range(range).busiest.as_ref()
    }

    /// The tokens' change against the previous range, in percent.
    pub fn tokens_change(&self, range: Range) -> Option<f32> {
        self.range(range).change
    }

    /// The range's days that hold a run, oldest first.
    pub fn days_in(&self, range: Range) -> &[DayUsage] {
        &self.range(range).days
    }
}

fn empty_range() -> &'static RangeUsage {
    static EMPTY: std::sync::OnceLock<RangeUsage> = std::sync::OnceLock::new();
    EMPTY.get_or_init(RangeUsage::default)
}

/// The usage screen's whole read model: its scope and every provider's days,
/// ranges and plan window.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageView {
    /// The canonical project paths the board covers: a group's projects, or
    /// the one open project.
    pub scope: Vec<PathBuf>,
    /// In the tool's order, then the providers only the log names.
    pub providers: Vec<ProviderUsage>,
    pub now: DateTime<Utc>,
}

impl UsageView {
    /// The log's runs in `scope`, over the last [`DAYS`], with every provider
    /// iboard knows when the tool's own list is not at hand.
    pub fn read(entries: &[Entry], scope: &[PathBuf], now: DateTime<Utc>) -> Self {
        let listed: Vec<ListedProvider> = Provider::ALL
            .iter()
            .copied()
            .map(ListedProvider::from_provider)
            .collect();
        Self::gather(entries, scope, now, &listed)
    }

    /// [`UsageView::read`] with the providers the tool lists, in its order.
    /// A listed provider that never ran keeps its row and reads as empty.
    pub fn read_with<P: Listed>(
        entries: &[Entry],
        scope: &[PathBuf],
        now: DateTime<Utc>,
        listed: &[P],
    ) -> Self {
        let listed: Vec<ListedProvider> = listed.iter().map(Listed::listed).collect();
        Self::gather(entries, scope, now, &listed)
    }

    /// [`UsageView::read_with`] under the list's other common name.
    pub fn read_listed<P: Listed>(
        entries: &[Entry],
        scope: &[PathBuf],
        now: DateTime<Utc>,
        listed: &[P],
    ) -> Self {
        Self::read_with(entries, scope, now, listed)
    }

    /// [`UsageView::read`] under the constructor's name.
    pub fn new(entries: &[Entry], scope: &[PathBuf], now: DateTime<Utc>) -> Self {
        Self::read(entries, scope, now)
    }

    /// [`UsageView::read_with`] under the constructor's name.
    pub fn new_with<P: Listed>(
        entries: &[Entry],
        scope: &[PathBuf],
        now: DateTime<Utc>,
        listed: &[P],
    ) -> Self {
        Self::read_with(entries, scope, now, listed)
    }

    /// [`UsageView::read`] under the older read-model verb.
    pub fn of(entries: &[Entry], scope: &[PathBuf], now: DateTime<Utc>) -> Self {
        Self::read(entries, scope, now)
    }

    /// [`UsageView::read_with`] under the older read-model verb.
    pub fn of_listed<P: Listed>(
        entries: &[Entry],
        scope: &[PathBuf],
        now: DateTime<Utc>,
        listed: &[P],
    ) -> Self {
        Self::read_with(entries, scope, now, listed)
    }

    fn gather(
        entries: &[Entry],
        scope: &[PathBuf],
        now: DateTime<Utc>,
        listed: &[ListedProvider],
    ) -> Self {
        let today = now.with_timezone(&Local).date_naive();
        let first = today - Duration::days(DAYS - 1);
        let projects: BTreeSet<PathBuf> = scope.iter().map(|path| canonical(path)).collect();
        let mut builders: BTreeMap<String, Builder> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        let mut known: BTreeMap<String, ListedProvider> = BTreeMap::new();
        for provider in listed {
            if known
                .insert(provider.provider.clone(), provider.clone())
                .is_none()
            {
                order.push(provider.provider.clone());
            }
        }
        for entry in entries {
            let project = canonical(&entry.project);
            if !projects.contains(&project) {
                continue;
            }
            let Some((day, at)) = ended_of(entry) else {
                continue;
            };
            if day < first || day > today {
                continue;
            }
            let key = provider_key(entry.provider.as_deref().unwrap_or("unknown"));
            if !order.contains(&key) {
                order.push(key.clone());
            }
            builders
                .entry(key)
                .or_default()
                .add(entry, day, at, project);
        }
        let mut used = Vec::new();
        let mut providers = Vec::new();
        for key in order {
            let info = known.remove(&key);
            let builder = builders.remove(&key).unwrap_or_default();
            let billing = builder
                .latest_billing()
                .or_else(|| info.as_ref().map(|provider| provider.billing))
                .unwrap_or_else(|| Plan::default_for(&key).billing);
            let hue = info
                .as_ref()
                .map(|provider| provider.hue)
                .or_else(|| hue_for(&key))
                .unwrap_or_else(|| next_hue(&used));
            if !used.contains(&hue) {
                used.push(hue);
            }
            let name = info
                .as_ref()
                .map(|provider| provider.name.clone())
                .unwrap_or_else(|| provider_name(&key));
            providers.push(builder.build(key, name, billing, hue, today));
        }
        Self {
            scope: projects.into_iter().collect(),
            providers,
            now,
        }
    }

    /// The provider with `provider`'s normalized ID.
    pub fn provider(&self, provider: &str) -> Option<&ProviderUsage> {
        let key = provider_key(provider);
        self.providers.iter().find(|usage| usage.provider == key)
    }

    /// The provider with `provider`'s normalized ID, to edit.
    pub fn provider_mut(&mut self, provider: &str) -> Option<&mut ProviderUsage> {
        let key = provider_key(provider);
        self.providers
            .iter_mut()
            .find(|usage| usage.provider == key)
    }

    /// Adds the listed providers the view does not have yet, and updates the
    /// billing, name and hue of those it does.
    pub fn with_listed<P: Listed>(mut self, listed: &[P]) -> Self {
        let mut ordered = Vec::new();
        for listed in listed.iter().map(Listed::listed) {
            if let Some(usage) = self.provider_mut(&listed.provider) {
                usage.name = listed.name;
                usage.billing = listed.billing;
                usage.hue = listed.hue;
            } else {
                ordered.push(listed);
            }
        }
        for listed in ordered {
            if self.provider(&listed.provider).is_none() {
                self.providers.push(ProviderUsage::empty(
                    listed.provider,
                    listed.name,
                    listed.billing,
                    listed.hue,
                ));
            }
        }
        self
    }

    /// Attaches each subscription's reported plan window to its provider. A
    /// provider that reports several keeps the first, with its report first.
    pub fn with_meters(mut self, meters: &[Meter]) -> Self {
        for meter in meters {
            let window = PlanWindow::of(meter);
            if let Some(usage) = self.provider_mut(&meter.provider) {
                if usage.plan.is_none() || meter.reported.is_some() {
                    usage.plan = Some(window.clone());
                }
                usage.plans.push(window);
            }
        }
        self
    }

    /// Every provider's total over `range`.
    pub fn totals(&self, range: Range) -> Totals {
        let mut totals = Totals::default();
        for provider in &self.providers {
            add_totals(&mut totals, provider.range(range).totals);
        }
        totals
    }

    /// Every provider's total over the range before `range`.
    pub fn previous_totals(&self, range: Range) -> Totals {
        let mut totals = Totals::default();
        for provider in &self.providers {
            add_totals(&mut totals, provider.range(range).previous);
        }
        totals
    }

    /// What the range's metered runs were charged, every provider together.
    pub fn spent(&self, range: Range) -> f64 {
        self.totals(range).spent
    }

    /// The range's API-equivalent dollars, never added to [`UsageView::spent`].
    pub fn equivalent(&self, range: Range) -> f64 {
        self.totals(range).equivalent
    }

    /// Whether no provider ran in `range`.
    pub fn is_empty(&self, range: Range) -> bool {
        self.totals(range).is_empty()
    }
}

fn add_totals(total: &mut Totals, part: Totals) {
    total.tokens += part.tokens;
    total.runs += part.runs;
    total.spent += part.spent;
    total.equivalent += part.equivalent;
}

/// One run as the ranges split it.
struct Fact {
    day: NaiveDate,
    model: String,
    project: PathBuf,
    project_name: String,
    tokens: u64,
    spent: f64,
    equivalent: f64,
}

/// The runs of one provider in the window, and its latest billing.
#[derive(Default)]
struct Builder {
    facts: Vec<Fact>,
    latest: Option<(DateTime<Utc>, Billing)>,
}

impl Builder {
    fn add(&mut self, entry: &Entry, day: NaiveDate, at: DateTime<Utc>, project: PathBuf) {
        let dollars = entry.cost.unwrap_or(0.0);
        let (spent, equivalent) = match entry.billing {
            Billing::Metered => (dollars, 0.0),
            Billing::Subscription => (0.0, dollars),
        };
        self.facts.push(Fact {
            day,
            model: entry
                .model
                .as_deref()
                .filter(|model| !model.is_empty())
                .unwrap_or("Unknown")
                .to_string(),
            project_name: files::file_name(&project),
            project,
            tokens: entry.tokens.total(),
            spent,
            equivalent,
        });
        if self.latest.is_none_or(|(seen, _)| at >= seen) {
            self.latest = Some((at, entry.billing));
        }
    }

    fn latest_billing(&self) -> Option<Billing> {
        self.latest.map(|(_, billing)| billing)
    }

    fn build(
        self,
        provider: String,
        name: String,
        billing: Billing,
        hue: Hue,
        today: NaiveDate,
    ) -> ProviderUsage {
        let days = days_of(self.facts.iter());
        let mut ranges = BTreeMap::new();
        for range in Range::ALL {
            ranges.insert(range, self.range_usage(range, today));
        }
        let d30 = ranges.get(&Range::D30).cloned().unwrap_or_default();
        ProviderUsage {
            provider,
            name,
            billing,
            hue,
            days,
            ranges,
            plans: Vec::new(),
            plan: None,
            totals: d30.totals,
            previous: d30.previous,
            days_in_range: d30.days,
            busiest: d30.busiest,
            models: d30.models,
            projects: d30.projects,
            previous_models: d30.previous_models,
            previous_projects: d30.previous_projects,
            change: d30.change,
        }
    }

    fn range_usage(&self, range: Range, today: NaiveDate) -> RangeUsage {
        let (first, last) = range.span(today);
        let (previous_first, previous_last) = range.previous_span(today);
        let current: Vec<&Fact> = self.facts_in(first, last).collect();
        let previous: Vec<&Fact> = self.facts_in(previous_first, previous_last).collect();
        let totals = totals_of(current.iter().copied());
        let previous_totals = totals_of(previous.iter().copied());
        RangeUsage {
            range,
            totals,
            previous: previous_totals,
            days: days_of(current.iter().copied()),
            busiest: busiest_of(current.iter().copied()),
            previous_busiest: busiest_of(previous.iter().copied()),
            models: models_of(&current, totals.tokens),
            previous_models: models_of(&previous, previous_totals.tokens),
            projects: projects_of(&current, totals.tokens),
            previous_projects: projects_of(&previous, previous_totals.tokens),
            change: totals.tokens_change(&previous_totals),
        }
    }

    fn facts_in(&self, first: NaiveDate, last: NaiveDate) -> impl Iterator<Item = &Fact> {
        self.facts
            .iter()
            .filter(move |fact| fact.day >= first && fact.day <= last)
    }
}

fn days_of<'a>(facts: impl IntoIterator<Item = &'a Fact>) -> Vec<DayUsage> {
    let mut by_day: BTreeMap<NaiveDate, DayUsage> = BTreeMap::new();
    for fact in facts {
        let day = by_day.entry(fact.day).or_insert(DayUsage {
            day: fact.day,
            tokens: 0,
            runs: 0,
            spent: 0.0,
            equivalent: 0.0,
        });
        day.tokens += fact.tokens;
        day.runs += 1;
        day.spent += fact.spent;
        day.equivalent += fact.equivalent;
    }
    by_day.into_values().collect()
}

fn totals_of<'a>(facts: impl IntoIterator<Item = &'a Fact>) -> Totals {
    let mut totals = Totals::default();
    for fact in facts {
        totals.tokens += fact.tokens;
        totals.runs += 1;
        totals.spent += fact.spent;
        totals.equivalent += fact.equivalent;
    }
    totals
}

fn busiest_of<'a>(facts: impl IntoIterator<Item = &'a Fact>) -> Option<DayUsage> {
    let mut busiest: Option<DayUsage> = None;
    for day in days_of(facts) {
        if busiest.as_ref().is_none_or(|seen| day.tokens > seen.tokens) {
            busiest = Some(day);
        }
    }
    busiest
}

fn models_of(facts: &[&Fact], total: u64) -> Vec<ModelUsage> {
    let mut by_model: BTreeMap<&str, Totals> = BTreeMap::new();
    for fact in facts {
        let totals = by_model.entry(fact.model.as_str()).or_default();
        totals.tokens += fact.tokens;
        totals.runs += 1;
        totals.spent += fact.spent;
        totals.equivalent += fact.equivalent;
    }
    let mut models: Vec<ModelUsage> = by_model
        .into_iter()
        .map(|(model, totals)| ModelUsage {
            model: model.to_string(),
            name: model.to_string(),
            tokens: totals.tokens,
            runs: totals.runs,
            spent: totals.spent,
            equivalent: totals.equivalent,
            share: share(totals.tokens, total),
        })
        .collect();
    models.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.model.cmp(&b.model)));
    models
}

fn projects_of(facts: &[&Fact], total: u64) -> Vec<ProjectUsage> {
    let mut by_project: BTreeMap<&Path, (String, Totals)> = BTreeMap::new();
    for fact in facts {
        let (name, totals) = by_project
            .entry(fact.project.as_path())
            .or_insert_with(|| (fact.project_name.clone(), Totals::default()));
        totals.tokens += fact.tokens;
        totals.runs += 1;
        totals.spent += fact.spent;
        totals.equivalent += fact.equivalent;
        let _ = name;
    }
    let mut projects: Vec<ProjectUsage> = by_project
        .into_iter()
        .map(|(path, (name, totals))| ProjectUsage {
            project: name.clone(),
            name,
            path: path.to_path_buf(),
            tokens: totals.tokens,
            runs: totals.runs,
            spent: totals.spent,
            equivalent: totals.equivalent,
            share: share(totals.tokens, total),
        })
        .collect();
    projects.sort_by(|a, b| {
        b.tokens
            .cmp(&a.tokens)
            .then_with(|| a.project.cmp(&b.project))
    });
    projects
}

fn share(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The local day and instant a run ended.
fn ended_of(entry: &Entry) -> Option<(NaiveDate, DateTime<Utc>)> {
    let ended = DateTime::parse_from_rfc3339(&entry.ended)
        .ok()?
        .with_timezone(&Utc);
    Some((ended.with_timezone(&Local).date_naive(), ended))
}

fn hue_for(provider: &str) -> Option<Hue> {
    match provider_key(provider).as_str() {
        "anthropic" | "claude" => Some(Hue::Orange),
        "deepseek" => Some(Hue::Teal),
        "copilot" => Some(Hue::Pink),
        _ => None,
    }
}

fn next_hue(used: &[Hue]) -> Hue {
    Hue::ALL
        .into_iter()
        .find(|hue| !used.contains(hue))
        .unwrap_or(Hue::Grey)
}

#[cfg(test)]
mod tests;
