//! One meter per subscription plan: the log's window, against the cap the
//! plan or the provider gives.

use chrono::{DateTime, Utc};

use super::{Consumption, Entry, Meter, instant};
use crate::domains::agent::runtime::{
    Billing, Plan, Plans, Provider, Quota, Window, provider_key, provider_name,
};

/// Every subscription plan, known or declared, with one meter per window.
pub fn subscription_meters(plans: &Plans, entries: &[Entry], now: DateTime<Utc>) -> Vec<Meter> {
    subscription_meters_with(plans, entries, now, &[])
}

/// [`subscription_meters`], with the provider quotas; one already reset is left
/// out, so an old figure is never current. A provider that reports several
/// windows gets a meter for each; one that reports none falls back on its
/// plan's window.
pub fn subscription_meters_with(
    plans: &Plans,
    entries: &[Entry],
    now: DateTime<Utc>,
    quotas: &[Quota],
) -> Vec<Meter> {
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
        .flat_map(|key| {
            let plan = plans.plan(&key);
            if plan.billing != Billing::Subscription {
                return Vec::new();
            }
            let reports = windows_of(quotas, &key, now);
            if reports.is_empty() {
                let start = plan.window.start(now);
                return vec![meter(&key, &plan, entries, start, plan.window, None, now)];
            }
            let declared = reports.iter().any(|quota| quota.window == plan.window);
            let mut meters: Vec<Meter> = reports
                .into_iter()
                .map(|quota| {
                    let start = quota.window.start_before(quota.reset);
                    meter(&key, &plan, entries, start, quota.window, Some(quota), now)
                })
                .collect();
            if !declared {
                let start = plan.window.start(now);
                meters.push(meter(&key, &plan, entries, start, plan.window, None, now));
            }
            meters
        })
        .collect()
}

/// The provider's current quotas, one per window, the first report winning on
/// a duplicate.
fn windows_of(quotas: &[Quota], key: &str, now: DateTime<Utc>) -> Vec<Quota> {
    let mut windows: Vec<Window> = Vec::new();
    quotas
        .iter()
        .filter(|quota| quota.provider == key && quota.reset > now)
        .filter(|quota| {
            let seen = windows.contains(&quota.window);
            if !seen {
                windows.push(quota.window);
            }
            !seen
        })
        .cloned()
        .collect()
}

/// One window of `key`: its runs, its consumption and its reset.
fn meter(
    key: &str,
    plan: &Plan,
    entries: &[Entry],
    start: DateTime<Utc>,
    window: Window,
    reported: Option<Quota>,
    now: DateTime<Utc>,
) -> Meter {
    let reset = reported
        .as_ref()
        .map_or_else(|| window.reset(now), |quota| quota.reset);
    let runs: Vec<Entry> = entries
        .iter()
        .filter(|entry| {
            entry.provider.as_deref().map(provider_key).as_deref() == Some(key)
                && instant(&entry.ended).is_some_and(|ended| ended >= start && ended <= now)
        })
        .cloned()
        .collect();
    Meter {
        name: provider_name(key),
        consumed: Consumption::since(&runs, key, plan, start),
        reported,
        window,
        start,
        reset,
        provider: key.to_string(),
        plan: *plan,
        runs,
        now,
    }
}
