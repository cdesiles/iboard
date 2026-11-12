use super::*;
use crate::domains::agent::runtime::Tokens;
use chrono::{Days, SecondsFormat};

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-12T15:30:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn day(now: DateTime<Utc>, days_ago: u64) -> NaiveDate {
    now.with_timezone(&Local).date_naive() - Days::new(days_ago)
}

fn stamp(now: DateTime<Utc>, days_ago: u64) -> String {
    (now - Duration::days(days_ago as i64)).to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[allow(clippy::too_many_arguments)]
fn run(
    session: &str,
    project: &str,
    provider: &str,
    billing: Billing,
    cost: f64,
    tokens: u64,
    model: Option<&str>,
    now: DateTime<Utc>,
    days_ago: u64,
) -> Entry {
    let ended = stamp(now, days_ago);
    Entry {
        session: session.to_string(),
        project: PathBuf::from(project),
        engine: "opencode".to_string(),
        provider: Some(provider.to_string()),
        model: model.map(str::to_string),
        billing,
        cost: Some(cost),
        tokens: Tokens {
            input: tokens,
            ..Tokens::default()
        },
        started: ended.clone(),
        ended,
    }
}

#[test]
fn a_group_of_two_projects_sums_both_and_one_project_reads_one() {
    let now = now();
    let entries = [
        run(
            "a",
            "/app",
            "deepseek",
            Billing::Metered,
            0.5,
            100,
            Some("deepseek-chat"),
            now,
            1,
        ),
        run(
            "b",
            "/site",
            "deepseek",
            Billing::Metered,
            0.25,
            50,
            Some("deepseek-chat"),
            now,
            2,
        ),
    ];
    let group = UsageView::read(&entries, &["/app".into(), "/site".into()], now);
    let deepseek = group.provider("deepseek").unwrap();
    assert_eq!(deepseek.totals(Range::D30).runs, 2);
    assert_eq!(deepseek.totals(Range::D30).tokens, 150);
    assert_eq!(deepseek.totals(Range::D30).spent, 0.75);
    assert_eq!(deepseek.projects(Range::D30).len(), 2);
    assert_eq!(deepseek.projects(Range::D30)[0].project, "app");

    let one = UsageView::read(&entries, &["/app".into()], now);
    let deepseek = one.provider("deepseek").unwrap();
    assert_eq!(deepseek.totals(Range::D30).runs, 1);
    assert_eq!(deepseek.totals(Range::D30).tokens, 100);
    assert_eq!(deepseek.projects(Range::D30).len(), 1);
}

#[test]
fn a_day_without_a_run_is_absent_from_the_days() {
    let now = now();
    let entries = [
        run(
            "old",
            "/app",
            "deepseek",
            Billing::Metered,
            0.5,
            100,
            None,
            now,
            3,
        ),
        run(
            "new",
            "/app",
            "deepseek",
            Billing::Metered,
            0.25,
            50,
            None,
            now,
            1,
        ),
    ];
    let view = UsageView::read(&entries, &["/app".into()], now);
    let deepseek = view.provider("deepseek").unwrap();
    assert_eq!(deepseek.days.len(), 2);
    assert_eq!(deepseek.days[0].day, day(now, 3));
    assert_eq!(deepseek.days[1].day, day(now, 1));
}

#[test]
fn a_range_with_no_run_reads_empty_and_has_no_busiest_day() {
    let now = now();
    let entries = [run(
        "old",
        "/app",
        "deepseek",
        Billing::Metered,
        0.5,
        100,
        None,
        now,
        40,
    )];
    let view = UsageView::read(&entries, &["/app".into()], now);
    let range = view.provider("deepseek").unwrap().range(Range::D7);
    assert!(range.is_empty());
    assert!(range.days.is_empty());
    assert_eq!(range.busiest, None);
}

#[test]
fn the_delta_is_against_the_previous_range_or_none_without_history() {
    let now = now();
    let entries = [
        run(
            "before",
            "/app",
            "deepseek",
            Billing::Metered,
            0.5,
            200,
            None,
            now,
            35,
        ),
        run(
            "now",
            "/app",
            "deepseek",
            Billing::Metered,
            0.5,
            100,
            None,
            now,
            1,
        ),
    ];
    let view = UsageView::read(&entries, &["/app".into()], now);
    let range = view.provider("deepseek").unwrap().range(Range::D30);
    assert_eq!(range.change, Some(-50.0));
    assert_eq!(range.previous.tokens, 200);
    assert!(range.has_previous());

    let fresh = [run(
        "now",
        "/app",
        "deepseek",
        Billing::Metered,
        0.5,
        100,
        None,
        now,
        1,
    )];
    let view = UsageView::read(&fresh, &["/app".into()], now);
    let range = view.provider("deepseek").unwrap().range(Range::D30);
    assert_eq!(range.change, None);
    assert_eq!(range.previous.runs, 0);
    assert!(!range.has_previous());
}

#[test]
fn a_provider_that_never_ran_keeps_its_row_and_reads_empty() {
    let now = now();
    let entries = [run(
        "a",
        "/app",
        "deepseek",
        Billing::Metered,
        0.5,
        100,
        None,
        now,
        1,
    )];
    let listed = [
        ListedProvider::from_provider(Provider::DeepSeek),
        ListedProvider::from_provider(Provider::Copilot),
    ];
    let view = UsageView::read_with(&entries, &["/app".into()], now, &listed);
    let copilot = view.provider("copilot").unwrap();
    assert_eq!(copilot.name, "Copilot");
    assert_eq!(copilot.billing, Billing::Subscription);
    assert!(!copilot.has_run());
    assert!(copilot.totals.is_empty());
    assert_eq!(copilot.days.len(), 0);
}

#[test]
fn a_subscription_is_api_equivalent_and_never_spent() {
    let now = now();
    let entries = [run(
        "sub",
        "/app",
        "github-copilot",
        Billing::Subscription,
        3.0,
        250,
        Some("gpt-5"),
        now,
        1,
    )];
    let view = UsageView::read(&entries, &["/app".into()], now);
    let copilot = view.provider("copilot").unwrap();
    assert_eq!(copilot.billing, Billing::Subscription);
    assert_eq!(copilot.totals(Range::D30).spent, 0.0);
    assert_eq!(copilot.totals(Range::D30).equivalent, 3.0);
    assert_eq!(view.spent(Range::D30), 0.0);
    assert_eq!(view.equivalent(Range::D30), 3.0);
}

#[test]
fn the_busiest_day_is_the_most_tokens() {
    let now = now();
    let entries = [
        run(
            "a",
            "/app",
            "deepseek",
            Billing::Metered,
            0.5,
            100,
            None,
            now,
            3,
        ),
        run(
            "b",
            "/app",
            "deepseek",
            Billing::Metered,
            0.5,
            400,
            None,
            now,
            2,
        ),
    ];
    let view = UsageView::read(&entries, &["/app".into()], now);
    let busiest = view
        .provider("deepseek")
        .unwrap()
        .busiest(Range::D7)
        .unwrap();
    assert_eq!(busiest.day, day(now, 2));
    assert_eq!(busiest.tokens, 400);
}

#[test]
fn the_previous_range_keeps_its_models_and_projects() {
    let now = now();
    let entries = [
        run(
            "before",
            "/app",
            "deepseek",
            Billing::Metered,
            0.5,
            200,
            Some("deepseek-reasoner"),
            now,
            35,
        ),
        run(
            "now",
            "/site",
            "deepseek",
            Billing::Metered,
            0.5,
            100,
            Some("deepseek-chat"),
            now,
            1,
        ),
    ];
    let view = UsageView::read(&entries, &["/app".into(), "/site".into()], now);
    let range = view.provider("deepseek").unwrap().range(Range::D30);
    assert_eq!(range.models[0].model, "deepseek-chat");
    assert_eq!(range.previous_models[0].model, "deepseek-reasoner");
    assert_eq!(range.previous_projects[0].project, "app");
    assert_eq!(range.previous_projects[0].tokens, 200);
}
