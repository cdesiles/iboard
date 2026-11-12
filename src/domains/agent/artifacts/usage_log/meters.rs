//! One meter per subscription plan: the log's window, against the cap the
//! plan or the provider gives.

use chrono::{DateTime, Utc};

use super::{Consumption, Entry, Meter, instant};
use crate::domains::agent::runtime::{
    Billing, Plans, Provider, Quota, provider_key, provider_name,
};

/// Every subscription plan, known or declared, with its window's use.
pub fn subscription_meters(plans: &Plans, entries: &[Entry], now: DateTime<Utc>) -> Vec<Meter> {
    subscription_meters_with(plans, entries, now, &[])
}

/// [`subscription_meters`], with the provider quotas; one already reset is left
/// out, so an old figure is never current.
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
        .filter_map(|key| {
            let plan = plans.plan(&key);
            if plan.billing != Billing::Subscription {
                return None;
            }
            let reported = quotas
                .iter()
                .find(|quota| quota.provider == key && quota.reset > now)
                .cloned();
            let start = plan.window.start(now);
            let reset = reported
                .as_ref()
                .map_or_else(|| plan.window.reset(now), |quota| quota.reset);
            let runs: Vec<Entry> = entries
                .iter()
                .filter(|entry| {
                    entry.provider.as_deref().map(provider_key).as_deref() == Some(key.as_str())
                        && instant(&entry.ended).is_some_and(|ended| ended >= start && ended <= now)
                })
                .cloned()
                .collect();
            Some(Meter {
                name: provider_name(&key),
                consumed: Consumption::of(&runs, &key, &plan, now),
                reported,
                reset,
                provider: key,
                plan,
                runs,
                now,
            })
        })
        .collect()
}
