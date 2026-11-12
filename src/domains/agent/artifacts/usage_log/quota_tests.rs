use crate::domains::agent::runtime::{Billing, Cap, Plan, Plans, Quota, Window};

use super::*;

fn noon() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-12T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn copilot_quota(now: DateTime<Utc>) -> Quota {
    Quota {
        provider: "copilot".to_string(),
        used: 9_193,
        cap: 30_000,
        unit: "AI credits",
        reset: now + Duration::days(19),
    }
}

fn run(tokens: u64, now: DateTime<Utc>) -> Entry {
    Entry {
        session: "ses_1".to_string(),
        project: PathBuf::from("/p"),
        engine: "opencode".to_string(),
        provider: Some("github-copilot".to_string()),
        model: None,
        billing: Billing::Subscription,
        cost: Some(3.0),
        tokens: Tokens {
            input: tokens,
            ..Tokens::default()
        },
        started: stamp(now.timestamp_millis() as u64),
        ended: stamp(now.timestamp_millis() as u64),
    }
}

fn copilot(meters: &[Meter]) -> &Meter {
    meters
        .iter()
        .find(|meter| meter.provider == "copilot")
        .unwrap()
}

#[test]
fn a_provider_reported_quota_fills_the_ring_and_sets_the_reset() {
    let now = noon();
    let quota = copilot_quota(now);
    let meters = subscription_meters_with(
        &Plans::default(),
        &[run(1_234, now)],
        now,
        std::slice::from_ref(&quota),
    );
    let meter = copilot(&meters);
    assert_eq!(meter.reported.as_ref(), Some(&quota));
    assert_eq!(meter.reset, quota.reset, "the provider's reset date");
    let share = meter.share().expect("a reported share");
    assert!((share - 9_193.0 / 30_000.0).abs() < 0.000_1, "{share}");
    assert_eq!(
        meter.consumed.tokens, 1_234,
        "the log still holds the tokens"
    );
}

#[test]
fn a_provider_reported_quota_wins_over_a_declared_cap() {
    let now = noon();
    let mut plans = Plans::default();
    plans.set(
        "copilot",
        Plan {
            billing: Billing::Subscription,
            window: Window::Month,
            price: None,
            cap: Some(Cap::Tokens(1_000)),
        },
    );
    let meters = subscription_meters_with(&plans, &[run(500, now)], now, &[copilot_quota(now)]);
    let share = copilot(&meters).share().unwrap();
    assert!((share - 9_193.0 / 30_000.0).abs() < 0.000_1, "{share}");
}

#[test]
fn a_provider_reported_quota_that_has_reset_is_not_shown() {
    let now = noon();
    let stale = Quota {
        reset: now - Duration::days(1),
        ..copilot_quota(now)
    };
    let meters = subscription_meters_with(&Plans::default(), &[], now, &[stale]);
    let meter = copilot(&meters);
    assert_eq!(meter.reported, None);
    assert_eq!(meter.share(), None, "no old figure as current");
    assert_eq!(meter.reset, Window::Month.reset(now));
}
