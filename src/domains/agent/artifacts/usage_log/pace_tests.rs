use crate::domains::agent::runtime::{Billing, Cap, Plan, Plans, Window};

use super::*;

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

fn run(id: &str, model: &str, tokens: u64, at: DateTime<Utc>) -> Entry {
    Entry {
        session: id.to_string(),
        project: PathBuf::from("/p"),
        engine: "opencode".to_string(),
        provider: Some("claude".to_string()),
        model: Some(model.to_string()),
        billing: Billing::Subscription,
        cost: Some(0.0),
        tokens: Tokens {
            input: tokens,
            ..Tokens::default()
        },
        started: stamp(at.timestamp_millis() as u64),
        ended: stamp(at.timestamp_millis() as u64),
    }
}

fn plans(window: Window, cap: Option<Cap>) -> Plans {
    let mut plans = Plans::default();
    plans.set(
        "claude",
        Plan {
            billing: Billing::Subscription,
            window,
            price: None,
            cap,
        },
    );
    plans
}

fn meter(meters: &[Meter]) -> &Meter {
    meters
        .iter()
        .find(|meter| meter.provider == "claude")
        .unwrap()
}

#[test]
fn a_week_under_pace_keeps_its_projection_to_itself() {
    let now = at("2026-11-12T12:00:00Z");
    let plans = plans(Window::Week, Some(Cap::Tokens(100)));
    let entries = [run("ses_1", "opus", 42, now)];

    let meters = subscription_meters(&plans, &entries, now);
    let meter = meter(&meters);

    assert_eq!(meter.elapsed(), Some(0.5));
    assert!(!meter.over_pace());
    assert_eq!(meter.cap_reached_on(), None);
}

#[test]
fn a_week_over_pace_names_the_day_the_cap_is_hit() {
    let now = at("2026-11-12T12:00:00Z");
    let plans = plans(Window::Week, Some(Cap::Tokens(100)));
    let entries = [run("ses_1", "opus", 62, now)];

    let meters = subscription_meters(&plans, &entries, now);
    let meter = meter(&meters);

    assert!(meter.over_pace());
    assert_eq!(meter.cap_reached_on().as_deref(), Some("Sat"));
}

#[test]
fn the_first_moment_of_a_window_has_no_pace_yet() {
    let now = at("2026-11-09T00:00:00Z");
    let plans = plans(Window::Week, Some(Cap::Tokens(100)));

    let meters = subscription_meters(&plans, &[], now);
    let meter = meter(&meters);

    assert_eq!(meter.elapsed(), Some(0.0));
    assert!(!meter.over_pace());
    assert_eq!(meter.cap_reached_on(), None);
    let days = meter.per_day();
    assert_eq!(days.len(), 7);
    assert!(days[0].today);
    assert!(days[1..].iter().all(|day| !day.today && day.tokens == 0));
}

#[test]
fn a_month_over_pace_names_the_date_the_cap_is_hit() {
    let now = at("2026-11-12T12:00:00Z");
    let plans = plans(Window::Month, Some(Cap::Tokens(100)));
    let entries = [run("ses_1", "opus", 50, now)];

    let meters = subscription_meters(&plans, &entries, now);
    let meter = meter(&meters);

    let elapsed = meter.elapsed().unwrap();
    assert!((elapsed - 11.5 / 30.0).abs() < 1e-5);
    assert_eq!(meter.cap_reached_on().as_deref(), Some("Nov 24"));
    assert_eq!(meter.per_day().len(), 30);
}

#[test]
fn no_cap_has_no_pace_but_keeps_its_days_and_models() {
    let now = at("2026-11-12T12:00:00Z");
    let plans = plans(Window::Week, None);
    let entries = [run("ses_1", "opus", 30, now)];

    let meters = subscription_meters(&plans, &entries, now);
    let meter = meter(&meters);

    assert_eq!(meter.share(), None);
    assert_eq!(meter.elapsed(), None);
    assert!(!meter.over_pace());
    assert_eq!(meter.cap_reached_on(), None);
    assert_eq!(meter.per_day()[3].tokens, 30);
    assert_eq!(meter.by_model()[0].tokens, 30);
}

#[test]
fn days_and_models_ignore_runs_outside_the_window() {
    let now = at("2026-11-12T12:00:00Z");
    let plans = plans(Window::Week, Some(Cap::Tokens(100)));
    let entries = [
        run("ses_before", "opus", 99, at("2026-11-08T12:00:00Z")),
        run("ses_monday", "opus", 10, at("2026-11-09T12:00:00Z")),
        run("ses_wednesday", "sonnet", 20, at("2026-11-11T12:00:00Z")),
        run("ses_today", "sonnet", 30, now),
        run("ses_after", "opus", 99, at("2026-11-16T12:00:00Z")),
    ];

    let meters = subscription_meters(&plans, &entries, now);
    let meter = meter(&meters);

    let days = meter.per_day();
    let tokens: Vec<u64> = days.iter().map(|day| day.tokens).collect();
    assert_eq!(tokens, [10, 0, 20, 30, 0, 0, 0]);
    assert_eq!(days.iter().filter(|day| day.today).count(), 1);
    assert!(days[3].today);

    let models = meter.by_model();
    let split: Vec<(&str, u64)> = models
        .iter()
        .map(|model| (model.model.as_str(), model.tokens))
        .collect();
    assert_eq!(split, [("sonnet", 50), ("opus", 10)]);
    assert!((models[0].share - 50.0 / 60.0).abs() < 1e-6);
}
