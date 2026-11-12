//! The Usage screen: where Tools › Usage… lands, its rail, its range and the
//! two blank states.

use std::path::{Path, PathBuf};

use chrono::Utc;
use egui_kittest::kittest::NodeT;
use egui_kittest::kittest::Queryable;

use super::*;
use crate::agent::{AgentConfig, Billing, Tokens};
use crate::domains::agent::artifacts::usage_log::Entry;
use crate::usage::Range;
/// A configured OpenCode, with no provider connected yet.
fn opencode() -> AgentConfig {
    AgentConfig {
        configured: true,
        ..AgentConfig::default()
    }
}

/// One finished run of `provider` in `project`, yesterday.
fn entry(project: &Path, provider: &str) -> Entry {
    let at = Utc::now() - chrono::Duration::days(1);
    Entry {
        session: format!("ses_{provider}"),
        project: project.to_path_buf(),
        engine: "opencode".to_string(),
        provider: Some(provider.to_string()),
        model: Some("model".to_string()),
        billing: Billing::Metered,
        cost: Some(1.0),
        tokens: Tokens {
            input: 1_000,
            ..Tokens::default()
        },
        started: at.to_rfc3339(),
        ended: at.to_rfc3339(),
    }
}

#[test]
fn the_tools_menu_request_opens_the_screen() {
    let (_dir, mut app) = project("usage-open", &[("todo/", "")]);
    assert!(!app.usage.open);
    app.menu_picks(false, false, false, true);
    assert!(app.usage.open);
}

#[test]
fn esc_returns_to_the_board() {
    let (_dir, mut app) = project("usage-esc", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.usage_entries = vec![entry(&app.located.project_dir, "deepseek")];
    app.open_usage();
    let mut harness = kittest(app);
    harness.get_by_label("Usage");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().1.usage.open);
}

#[test]
fn no_tool_says_to_configure_one_and_opens_settings() {
    let (_dir, mut app) = project("usage-no-tool", &[("todo/", "")]);
    app.open_usage();
    let mut harness = kittest(app);
    harness.get_by_label("Configure a tool first");
    press(&mut harness, "Open Settings");
    assert!(harness.state().1.settings.is_some(), "Settings opens");
}

#[test]
fn a_tool_without_a_provider_says_so() {
    let (_dir, mut app) = project("usage-no-provider", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.open_usage();
    let harness = kittest(app);
    harness.get_by_label("OpenCode has no provider connected yet");
    harness.get_by_label("Connect one in OpenCode.");
    assert!(
        harness.query_all_by_label("Open Settings").next().is_none(),
        "nothing to set in iboard"
    );
}

#[test]
fn the_rail_is_absent_with_one_provider_and_with_none() {
    let (dir, mut app) = project("usage-rail", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.open_usage();
    let mut harness = kittest(app);
    assert!(!harness.state().1.usage.rail_shown());
    assert!(harness.query_all_by_label("All providers").next().is_none());

    let app = &mut harness.state_mut().1;
    app.usage_entries = vec![entry(&dir, "deepseek")];
    app.refresh_usage_view();
    harness.run_steps(2);
    assert_eq!(harness.state().1.usage.view.providers.len(), 1);
    assert!(!harness.state().1.usage.rail_shown());
    assert!(
        harness.query_all_by_label("All providers").next().is_none(),
        "one provider needs no rail"
    );
    harness.get_by_label("DeepSeek");
}

#[test]
fn two_providers_bring_the_rail_and_the_arrows_move_it() {
    let (dir, mut app) = project("usage-two", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.usage_entries = vec![entry(&dir, "deepseek"), entry(&dir, "copilot")];
    app.open_usage();
    let mut harness = kittest(app);
    assert!(harness.state().1.usage.rail_shown());
    harness.get_by_role_and_label(egui::Role::Tab, "All providers");
    harness.get_by_role_and_label(egui::Role::RadioButton, "30d");
    press(&mut harness, "7d");
    assert_eq!(harness.state().1.usage.range, Range::D7);
    harness.key_press(Key::ArrowDown);
    harness.run_steps(2);
    assert_eq!(
        harness.state().1.usage.provider.as_deref(),
        Some("deepseek")
    );
    harness.key_press(Key::ArrowUp);
    harness.run_steps(2);
    assert!(harness.state().1.usage.provider.is_none());
}

#[test]
fn the_scope_follows_the_board() {
    let parent = scratch_dir("usage-scope");
    let first = parent.join("alpha");
    let second = parent.join("beta");
    for dir in [&first, &second] {
        std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    }
    let mut app = IboardApp::from_located(discovery::locate(&first).unwrap());
    app.prefs.agent = opencode();
    app.workspace = Some(crate::workspace::Workspace {
        groups: vec![crate::workspace::Group {
            name: "Clients".to_string(),
            repos: vec![first.clone(), second.clone()],
        }],
    });
    app.open_usage();
    assert_eq!(
        app.usage.view.scope,
        vec![crate::shell::recents::canonical(&first)]
    );
    app.open_group("Clients");
    let mut scope: Vec<PathBuf> = app.usage.view.scope.clone();
    scope.sort();
    let mut expected = vec![
        crate::shell::recents::canonical(&first),
        crate::shell::recents::canonical(&second),
    ];
    expected.sort();
    assert_eq!(scope, expected);
    assert_eq!(app.usage.range, Range::D30, "the range stays 30d");
}

/// One finished run of `provider`, `days_ago` days back, with `tokens`.
fn entry_on(project: &Path, provider: &str, billing: Billing, tokens: u64, days_ago: i64) -> Entry {
    let at = Utc::now() - chrono::Duration::days(days_ago);
    Entry {
        session: format!("ses_{provider}_{days_ago}"),
        project: project.to_path_buf(),
        engine: "opencode".to_string(),
        provider: Some(provider.to_string()),
        model: Some("model".to_string()),
        billing,
        cost: Some(1.0),
        tokens: Tokens {
            input: tokens,
            ..Tokens::default()
        },
        started: at.to_rfc3339(),
        ended: at.to_rfc3339(),
    }
}

#[test]
fn spend_is_hidden_on_a_subscription() {
    let (dir, mut app) = project("usage-subscription", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.usage_entries = vec![entry_on(&dir, "copilot", Billing::Subscription, 2_000, 1)];
    app.open_usage();
    let harness = kittest(app);
    harness.get_by_label("API-EQUIVALENT");
    assert!(
        harness.query_all_by_label("Spend").next().is_none(),
        "a subscription charts tokens only"
    );
}

#[test]
fn the_project_split_is_hidden_for_one_project() {
    let (dir, mut app) = project("usage-one-project", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.usage_entries = vec![entry_on(&dir, "deepseek", Billing::Metered, 1_000, 1)];
    app.open_usage();
    let harness = kittest(app);
    harness.get_by_label("By model");
    assert!(
        harness.query_all_by_label("By project").next().is_none(),
        "one project needs no split"
    );
}

#[test]
fn the_readout_follows_the_arrow_keys() {
    let (dir, mut app) = project("usage-readout", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.usage_entries = vec![
        entry_on(&dir, "deepseek", Billing::Metered, 1_000, 0),
        entry_on(&dir, "deepseek", Billing::Metered, 2_000, 1),
    ];
    app.open_usage();
    let mut harness = kittest(app);
    harness.get_by_label_contains("Today · ");
    let chart = harness
        .get_all_by_role(egui::Role::Image)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.contains("per day"))
        })
        .expect("the per-day chart");
    chart.click();
    harness.run_steps(2);
    harness.key_press(Key::ArrowLeft);
    harness.run_steps(2);
    let yesterday = (Utc::now() - chrono::Duration::days(1))
        .format("%a %-d %b")
        .to_string();
    assert!(
        harness
            .query_all_by_label_contains(&yesterday)
            .next()
            .is_some(),
        "the readout names yesterday"
    );
    assert!(
        harness.query_by_label_contains("Today · ").is_none(),
        "the readout left today"
    );
}

#[test]
fn a_provider_that_never_ran_says_so_under_its_head() {
    let (dir, mut app) = project("usage-never-ran", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.usage_entries = vec![entry_on(&dir, "copilot", Billing::Subscription, 1_000, 200)];
    app.open_usage();
    let harness = kittest(app);
    harness.get_by_label("No agent run yet.");
    harness.get_by_label("A finished run through Copilot lands here.");
}

#[test]
fn a_range_without_a_run_says_so_in_place_of_the_chart() {
    let (dir, mut app) = project("usage-empty-range", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.usage_entries = vec![entry_on(&dir, "deepseek", Billing::Metered, 1_000, 40)];
    app.open_usage();
    let harness = kittest(app);
    harness.get_by_label("No agent run in this range.");
}

#[test]
fn copilots_reported_window_reads_as_premium_credits() {
    let (dir, mut app) = project("usage-plan", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.usage_entries = vec![entry_on(&dir, "copilot", Billing::Subscription, 1_000, 1)];
    app.session.quotas = vec![crate::agent::Quota {
        provider: "copilot".to_string(),
        window: crate::agent::Window::Month,
        measure: crate::agent::Measure::Count {
            used: 9_212,
            cap: 30_000,
            unit: "AI credits",
        },
        reset: Utc::now() + chrono::Duration::days(19),
    }];
    app.open_usage();
    let harness = kittest(app);
    harness.get_by_label("Premium credits");
    harness.get_by_label("This month, whatever the range");
    harness.get_by_label("9,212 of 30,000 AI credits · from GitHub");
}

#[test]
fn a_by_provider_row_opens_that_provider() {
    let (dir, mut app) = project("usage-by-provider", &[("todo/", "")]);
    app.prefs.agent = opencode();
    app.usage_entries = vec![
        entry_on(&dir, "deepseek", Billing::Metered, 1_000, 1),
        entry_on(&dir, "copilot", Billing::Subscription, 2_000, 1),
    ];
    app.open_usage();
    let mut harness = kittest(app);
    let row = harness
        .get_all_by_role(egui::Role::Button)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("DeepSeek · "))
        })
        .expect("DeepSeek's row in By provider");
    row.click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().1.usage.provider.as_deref(),
        Some("deepseek")
    );
}
