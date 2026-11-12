//! The Usage screen: where Tools › Usage… lands, its rail, its range and the
//! two blank states.

use std::path::PathBuf;

use chrono::Utc;
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
