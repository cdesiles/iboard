//! What the project's agents cost over the days the trends cover: the log's
//! finished runs, a day at a time and a provider at a time.
//!
//! A metered run's dollars are money spent; a subscription run's are
//! API-equivalent and never spend. Both have tokens, so the days stack those,
//! and a day without a run carries none.

use std::collections::BTreeMap;

use chrono::{DateTime, Local, NaiveDate};

use crate::domains::agent::artifacts::usage_log::Entry;
use crate::domains::agent::runtime::{Billing, provider_key, provider_name};

/// What one day's runs used, provider by provider.
#[derive(Debug, Clone, PartialEq)]
pub struct CostDay {
    pub day: NaiveDate,
    /// The providers that ran that day, in the providers' order. Empty on a
    /// day without a run.
    pub parts: Vec<CostPart>,
    pub spent: f64,
    pub equivalent: f64,
    pub tokens: u64,
}

/// One provider's share of a day.
#[derive(Debug, Clone, PartialEq)]
pub struct CostPart {
    /// The normalized provider ID the rows and the `[usage]` section share.
    pub provider: String,
    pub tokens: u64,
    pub spent: f64,
    pub equivalent: f64,
}

/// What one provider used over the range.
#[derive(Debug, Clone, PartialEq)]
pub struct CostProvider {
    pub provider: String,
    /// The provider's name, as the status bar shows it.
    pub name: String,
    pub billing: Billing,
    /// Money the provider's runs were charged, or 0 for a subscription.
    pub spent: f64,
    /// A subscription's API-equivalent dollars, never money taken.
    pub equivalent: f64,
    pub tokens: u64,
    pub runs: usize,
}

/// The range's runs: a day with its providers, and a row per provider.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cost {
    /// One day per day of the range, oldest first, empty days included so the
    /// chart can leave a gap for them.
    pub days: Vec<CostDay>,
    /// Metered providers first, then subscriptions, each by what it used.
    pub providers: Vec<CostProvider>,
    /// What the range's metered runs were charged.
    pub spent: f64,
    /// The range's API-equivalent dollars, never added to `spent`.
    pub equivalent: f64,
    /// Every token the range's runs used.
    pub tokens: u64,
    pub runs: usize,
}

impl Cost {
    /// Whether the range holds a run at all.
    pub fn is_empty(&self) -> bool {
        self.runs == 0
    }
}

/// `entries`' runs between `first` and `last`, both included, a day at a time.
pub fn collect(entries: &[Entry], first: NaiveDate, last: NaiveDate) -> Cost {
    if first > last {
        return Cost::default();
    }
    let mut cost = Cost::default();
    let mut totals: BTreeMap<(String, bool), (f64, f64, u64, usize)> = BTreeMap::new();
    for entry in entries {
        let Some(day) = ended_day(entry) else {
            continue;
        };
        if day < first || day > last {
            continue;
        }
        let provider = provider_key(entry.provider.as_deref().unwrap_or("unknown"));
        let tokens = entry.tokens.total();
        let dollars = entry.cost.unwrap_or(0.0);
        let subscription = entry.billing == Billing::Subscription;
        let (spent, equivalent) = match entry.billing {
            Billing::Metered => (dollars, 0.0),
            Billing::Subscription => (0.0, dollars),
        };
        cost.runs += 1;
        cost.tokens += tokens;
        cost.spent += spent;
        cost.equivalent += equivalent;
        let at = totals
            .entry((provider, subscription))
            .or_insert((0.0, 0.0, 0, 0));
        at.0 += spent;
        at.1 += equivalent;
        at.2 += tokens;
        at.3 += 1;
    }
    cost.days = days_of(entries, first, last);
    cost.providers = providers_of(totals);
    order_parts(&mut cost);
    cost
}

/// Puts each day's parts in the providers' order, so the chart stacks a
/// provider the same way every day.
fn order_parts(cost: &mut Cost) {
    let order: BTreeMap<&str, usize> = cost
        .providers
        .iter()
        .enumerate()
        .map(|(at, provider)| (provider.provider.as_str(), at))
        .collect();
    for day in &mut cost.days {
        day.parts.sort_by_key(|part| {
            order
                .get(part.provider.as_str())
                .copied()
                .unwrap_or(usize::MAX)
        });
    }
}

/// One entry per day of `first..=last`, oldest first, with the providers that
/// ran each.
fn days_of(entries: &[Entry], first: NaiveDate, last: NaiveDate) -> Vec<CostDay> {
    let mut by_day: BTreeMap<NaiveDate, BTreeMap<String, (f64, f64, u64)>> = BTreeMap::new();
    for entry in entries {
        let Some(day) = ended_day(entry) else {
            continue;
        };
        if day < first || day > last {
            continue;
        }
        let provider = provider_key(entry.provider.as_deref().unwrap_or("unknown"));
        let at = by_day.entry(day).or_default().entry(provider).or_default();
        let dollars = entry.cost.unwrap_or(0.0);
        match entry.billing {
            Billing::Metered => at.0 += dollars,
            Billing::Subscription => at.1 += dollars,
        }
        at.2 += entry.tokens.total();
    }
    let mut days = Vec::new();
    let mut at = first;
    while at <= last {
        let mut day = CostDay {
            day: at,
            parts: Vec::new(),
            spent: 0.0,
            equivalent: 0.0,
            tokens: 0,
        };
        for (provider, (spent, equivalent, tokens)) in by_day.remove(&at).unwrap_or_default() {
            day.spent += spent;
            day.equivalent += equivalent;
            day.tokens += tokens;
            day.parts.push(CostPart {
                provider,
                tokens,
                spent,
                equivalent,
            });
        }
        days.push(day);
        at += chrono::Duration::days(1);
    }
    days
}

fn providers_of(totals: BTreeMap<(String, bool), (f64, f64, u64, usize)>) -> Vec<CostProvider> {
    let mut providers: Vec<CostProvider> = totals
        .into_iter()
        .map(
            |((provider, subscription), (spent, equivalent, tokens, runs))| CostProvider {
                name: if provider == "unknown" {
                    "Unknown".to_string()
                } else {
                    provider_name(&provider)
                },
                provider,
                billing: if subscription {
                    Billing::Subscription
                } else {
                    Billing::Metered
                },
                spent,
                equivalent,
                tokens,
                runs,
            },
        )
        .collect();
    providers.sort_by(|a, b| {
        (a.billing == Billing::Subscription)
            .cmp(&(b.billing == Billing::Subscription))
            .then_with(|| match a.billing {
                Billing::Metered => b.spent.total_cmp(&a.spent),
                Billing::Subscription => b.tokens.cmp(&a.tokens),
            })
            .then_with(|| a.name.cmp(&b.name))
    });
    providers
}

pub(super) fn day_of(stamp: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(stamp.get(..10)?, "%Y-%m-%d").ok()
}

/// The local day `entry`'s run ended on: its stamp is UTC, while the page's
/// days are the local ones its commits fall on.
fn ended_day(entry: &Entry) -> Option<NaiveDate> {
    let ended = DateTime::parse_from_rfc3339(&entry.ended).ok()?;
    Some(ended.with_timezone(&Local).date_naive())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::runtime::Tokens;
    use chrono::{SecondsFormat, TimeZone, Utc};

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn run(id: &str, provider: &str, billing: Billing, cost: f64, day: &str, tokens: u64) -> Entry {
        Entry {
            session: id.to_string(),
            project: "/p".into(),
            engine: "opencode".to_string(),
            provider: Some(provider.to_string()),
            model: None,
            billing,
            cost: Some(cost),
            tokens: Tokens {
                input: tokens,
                ..Tokens::default()
            },
            started: format!("{day}T10:00:00.000Z"),
            ended: format!("{day}T10:30:00.000Z"),
        }
    }

    #[test]
    fn a_run_after_local_midnight_is_counted_on_that_local_day() {
        let today = Local::now().date_naive();
        let ended = Local
            .from_local_datetime(&today.and_hms_opt(0, 30, 0).unwrap())
            .earliest()
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut entry = run("a", "deepseek", Billing::Metered, 0.25, "2026-11-05", 10);
        entry.started = ended.clone();
        entry.ended = ended;

        let cost = collect(&[entry], today, today);

        assert_eq!(cost.runs, 1, "a run at 00:30 local is today's");
        assert_eq!(cost.days[0].day, today);
    }

    #[test]
    fn a_metered_run_is_spend_and_a_subscription_is_only_tokens() {
        let entries = [
            run("a", "deepseek", Billing::Metered, 0.25, "2026-11-05", 10),
            run(
                "b",
                "github-copilot",
                Billing::Subscription,
                3.0,
                "2026-11-05",
                100,
            ),
        ];
        let cost = collect(&entries, day("2026-11-05"), day("2026-11-05"));
        assert_eq!(cost.spent, 0.25);
        assert_eq!(cost.equivalent, 3.0, "never spend");
        assert_eq!(cost.tokens, 110);
        assert_eq!(cost.runs, 2);
        assert_eq!(cost.days[0].tokens, 110);
        assert_eq!(cost.days[0].parts.len(), 2);
        assert_eq!(
            cost.providers
                .iter()
                .map(|p| (p.provider.as_str(), p.billing))
                .collect::<Vec<_>>(),
            [
                ("deepseek", Billing::Metered),
                ("copilot", Billing::Subscription)
            ],
            "a metered provider comes first"
        );
        assert_eq!(cost.providers[0].spent, 0.25);
        assert_eq!(cost.providers[1].spent, 0.0);
        assert_eq!(cost.providers[1].equivalent, 3.0);
    }

    #[test]
    fn a_day_without_a_run_has_no_parts_between_two_days() {
        let entries = [run("a", "deepseek", Billing::Metered, 0.5, "2026-11-06", 7)];
        let cost = collect(&entries, day("2026-11-04"), day("2026-11-06"));
        let days: Vec<NaiveDate> = cost.days.iter().map(|d| d.day).collect();
        assert_eq!(
            days,
            [day("2026-11-04"), day("2026-11-05"), day("2026-11-06")]
        );
        assert!(cost.days[..2].iter().all(|d| d.parts.is_empty()));
        assert_eq!(cost.days[2].parts.len(), 1);
    }

    #[test]
    fn a_run_outside_the_range_is_left_out() {
        let entries = [
            run("old", "deepseek", Billing::Metered, 1.0, "2026-10-01", 5),
            run("new", "deepseek", Billing::Metered, 0.5, "2026-11-06", 7),
        ];
        let cost = collect(&entries, day("2026-11-04"), day("2026-11-06"));
        assert_eq!(cost.runs, 1);
        assert_eq!(cost.spent, 0.5);
        assert_eq!(cost.tokens, 7);
    }

    #[test]
    fn the_providers_are_named() {
        let entries = [
            run("a", "deepseek", Billing::Metered, 0.25, "2026-11-05", 1000),
            run(
                "b",
                "github-copilot",
                Billing::Subscription,
                0.0,
                "2026-11-05",
                10,
            ),
        ];
        let cost = collect(&entries, day("2026-11-05"), day("2026-11-05"));
        assert_eq!(
            cost.providers
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["DeepSeek", "Copilot"]
        );
    }

    #[test]
    fn a_plan_that_changed_billing_keeps_both_readings_apart() {
        let entries = [
            run("a", "claude", Billing::Metered, 0.25, "2026-11-05", 10),
            run("b", "claude", Billing::Subscription, 1.0, "2026-11-05", 20),
        ];
        let cost = collect(&entries, day("2026-11-05"), day("2026-11-05"));
        assert_eq!(cost.providers.len(), 2);
        assert_eq!(cost.spent, 0.25);
        assert_eq!(cost.equivalent, 1.0);
        assert_eq!(cost.tokens, 30);
    }

    #[test]
    fn a_range_with_no_run_is_empty() {
        let cost = collect(&[], day("2026-11-04"), day("2026-11-06"));
        assert!(cost.is_empty());
        assert_eq!(cost.days.len(), 3);
        assert!(cost.days.iter().all(|d| d.parts.is_empty()));
    }
}
