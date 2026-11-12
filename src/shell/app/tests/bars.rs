//! The header and status bars: what they name, and their keys, out of sight
//! until ⌘ is held.

use chrono::{SecondsFormat, Utc};
use egui_kittest::kittest::Queryable;

use super::*;
use crate::agent::{Billing, Cap, Plan, Tokens, Window};
use crate::domains::agent::artifacts::usage_log::Entry;

type Board = egui_kittest::Harness<'static, (bool, IboardApp)>;

/// The board with its fonts, its hints sliding in one frame, and a tenth of
/// a second per step.
fn board(dir: &Path) -> Board {
    let app = IboardApp::from_located(discovery::locate(dir).unwrap());
    let builder = egui_kittest::Harness::builder()
        .with_size(window().size())
        .with_step_dt(0.1);
    let harness = kittest_with(app, builder);
    theme::set_reduced_motion(&harness.ctx, true);
    harness
}

/// The board's header names the project, and its path is no longer there.
#[test]
fn the_board_header_names_the_project_without_its_path() {
    let dir = scratch_dir("project-name");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let harness = board(&dir);
    let name = harness.state().1.board.name();
    let path = display_path(&harness.state().1.board.root);
    harness.get_by_label(&name);
    let in_header = harness
        .query_all_by_label(&path)
        .any(|node| node.rect().min.y < theme::BAR_H);
    assert!(!in_header, "the path left the header");
}

fn new_issue_width(harness: &Board) -> f32 {
    harness.get_by_label("New issue").rect().width()
}

/// At rest the status bar lists no keys and New issue shows its label alone.
/// Half a second of ⌘ brings out `⌘N` and the bar's keys; letting go puts
/// them away.
#[test]
fn the_board_shows_its_keys_only_while_command_is_held() {
    let dir = scratch_dir("key-hints");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut harness = board(&dir);
    let at_rest = new_issue_width(&harness);
    assert!(
        harness.query_by_label("filter").is_none(),
        "no keys at rest"
    );

    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    harness.run_steps(7);
    assert!(
        harness.query_by_label("filter").is_some(),
        "the status bar lists them"
    );
    assert!(
        new_issue_width(&harness) > at_rest,
        "⌘N slides out of New issue"
    );

    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    harness.run_steps(1);
    assert!(harness.query_by_label("filter").is_none());
    assert_eq!(new_issue_width(&harness), at_rest);
}

/// A board with an issue per column, its keys out and `Auto` off.
fn board_of_every_column(dir: &Path) -> Board {
    for (column, name) in [
        ("todo", "a.md"),
        ("ready", "b.md"),
        ("doing", "c.md"),
        ("done", "d.md"),
    ] {
        let folder = dir.join("issues").join(column);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(name), "# Title\n").unwrap();
    }
    let mut harness = board(dir);
    harness.state_mut().1.prefs.auto = false;
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    harness.run_steps(7);
    harness
}

/// With Auto on the board reads itself: the idle hints don't offer `⌘R`.
#[test]
fn the_idle_hints_offer_refresh_only_with_auto_off() {
    let dir = scratch_dir("refresh-hint");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut harness = board(&dir);
    harness.state_mut().1.prefs.auto = true;
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    harness.run_steps(7);
    assert!(harness.query_by_label("filter").is_some());
    assert!(harness.query_by_label("refresh").is_none(), "Auto on");
    harness.state_mut().1.prefs.auto = false;
    harness.run_steps(2);
    assert!(harness.query_by_label("refresh").is_some(), "Auto off");
}

/// How many of the status bar's possible hints show.
fn hints_shown(harness: &Board) -> usize {
    [
        "new",
        "filter",
        "refresh",
        "stats",
        "project",
        "open",
        "edit",
        "move",
        "next column",
        "delete",
        "archive",
        "review",
        "ask",
        "board",
        "back",
        "find",
        "file",
        "hunk",
    ]
    .into_iter()
    .filter(|action| harness.query_by_label(action).is_some())
    .count()
}

/// Wherever you are, the status bar lists four keys at most. `Del` shows only
/// where it deletes, and the Reader always offers `M move`.
#[test]
fn the_status_bar_lists_four_keys_at_most() {
    let dir = scratch_dir("four-hints");
    let mut harness = board_of_every_column(&dir);
    assert_eq!(hints_shown(&harness), 3, "at rest");

    let cases = [
        ("todo/a.md", true),
        ("ready/b.md", true),
        ("doing/c.md", true),
        ("done/d.md", false),
    ];
    for (file, deletes) in cases {
        let path = dir.join("issues").join(file);
        harness.state_mut().1.select(path.clone());
        harness.run_steps(1);
        assert!(hints_shown(&harness) <= 3, "{file} selected");

        harness.state_mut().1.open_issue(path);
        harness.run_steps(1);
        assert!(hints_shown(&harness) <= 4, "{file} open");
        assert!(
            harness.query_by_label("move").is_some(),
            "{file} offers M move"
        );
        assert_eq!(
            harness.query_by_label("delete").is_some(),
            deletes,
            "{file}"
        );
        harness.state_mut().1.back_to_board();
        harness.run_steps(1);
    }
}

/// With the composer open, the keys are the ones it answers to.
#[test]
fn the_status_bar_names_the_composers_keys() {
    let dir = scratch_dir("composer-hints");
    let mut harness = board_of_every_column(&dir);
    harness.state_mut().1.open_composer();
    harness.run_steps(1);
    assert!(harness.query_all_by_label("add & edit").next().is_some());
    assert!(harness.query_all_by_label("close").next().is_some());
}

/// Holding ⌘ lists every key of the screen on one sheet, and letting go puts
/// it away with the hints beside the controls.
#[test]
fn holding_command_lists_every_key_on_a_sheet() {
    let dir = scratch_dir("shortcuts-sheet");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut harness = board(&dir);
    assert!(
        harness.query_by_label("Keyboard shortcuts").is_none(),
        "no sheet at rest"
    );

    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    harness.run_steps(7);
    harness.get_by_label("Keyboard shortcuts");
    harness.get_by_value("⌘N New issue");
    harness.get_by_value("⌘↩ Next column");

    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    harness.run_steps(1);
    assert!(harness.query_by_label("Keyboard shortcuts").is_none());
}

/// The sheet follows the screen: the Editor lists Save and the text's undo,
/// where the board lists neither.
#[test]
fn the_shortcuts_sheet_follows_the_screen() {
    let dir = scratch_dir("shortcuts-editor");
    let file = dir.join("issues/todo/a.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "# Title\n").unwrap();
    let mut harness = board(&dir);
    harness.state_mut().1.open_issue(file.clone());
    harness.state_mut().1.start_editing(&file);
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    harness.run_steps(7);
    harness.get_by_value("⌘S Save");
    harness.get_by_value("⌘Z Undo typing");
    assert!(
        harness.query_by_value("⌘↩ Next column").is_none(),
        "no cards here"
    );
}

/// With Auto on the board reads itself: the sheet doesn't offer `⌘R`.
#[test]
fn the_shortcuts_sheet_offers_refresh_only_with_auto_off() {
    let dir = scratch_dir("shortcuts-refresh");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut harness = board(&dir);
    harness.state_mut().1.prefs.auto = true;
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    harness.run_steps(7);
    assert!(harness.query_by_value("⌘R Refresh").is_none(), "Auto on");
    harness.state_mut().1.prefs.auto = false;
    harness.run_steps(1);
    harness.get_by_value("⌘R Refresh");
}

/// A modal owns the screen and already shows its own keys: no sheet over it.
#[test]
fn the_shortcuts_sheet_stays_off_a_modal() {
    let dir = scratch_dir("shortcuts-modal");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut harness = board(&dir);
    harness.state_mut().1.open_composer();
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    harness.run_steps(7);
    assert!(harness.query_by_label("Keyboard shortcuts").is_none());
}

/// A split draft's checkbox reads as what it includes.
#[test]
fn a_split_drafts_checkbox_has_a_name() {
    let mut drafts = vec![split::SplitDraft {
        title: "First part".to_string(),
        body: String::new(),
        include: true,
    }];
    let mut harness = egui_kittest::Harness::new_ui_state(
        move |ui, themed: &mut bool| {
            if !std::mem::replace(themed, true) {
                theme::install(ui.ctx());
                return;
            }
            split::drafts_ui(ui, &mut drafts);
        },
        false,
    );
    harness.run_steps(2);
    harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Include First part");
}

/// With Auto off Refresh is the board's button with its `⌘R`; on an issue it
/// is a header button too, with no key on Todo, where `⌘R` refines.
#[test]
fn refresh_shows_its_key_where_it_has_it() {
    let dir = scratch_dir("refresh-key");
    let mut harness = board_of_every_column(&dir);
    harness.get_by_label("Refresh");

    let todo = dir.join("issues/todo/a.md");
    let ready = dir.join("issues/ready/b.md");
    harness.state_mut().1.open_issue(todo.clone());
    harness.run_steps(2);
    harness.get_by_label("Refresh");

    let app = &harness.state().1;
    assert_eq!(app.refresh_key(&todo), "", "⌘R refines here");
    assert_eq!(app.refresh_key(&ready), "⌘R");
}

/// One entry of `provider` in the app's usage log, as if it just finished.
fn log_run(app: &mut IboardApp, provider: &str, tokens: u64, cost: f64) {
    log_run_of(app, provider, None, tokens, cost);
}

/// [`log_run`] with the model the run used.
fn log_run_of(app: &mut IboardApp, provider: &str, model: Option<&str>, tokens: u64, cost: f64) {
    let at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    app.usage_entries.push(Entry {
        session: format!("ses_{provider}_{tokens}"),
        project: app.located.project_dir.clone(),
        engine: "opencode".to_string(),
        provider: Some(provider.to_string()),
        model: model.map(str::to_string),
        billing: Billing::Subscription,
        cost: Some(cost),
        tokens: Tokens {
            input: tokens,
            ..Tokens::default()
        },
        started: at.clone(),
        ended: at,
    });
}

/// A subscription with a token cap fills the pill with its share, names it
/// with its reset, and opens the popover with the link to Settings.
#[test]
fn a_subscription_pill_reads_its_cap_and_opens_the_usage_popover() {
    let dir = scratch_dir("subscription-pill");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let mut harness = kittest(app);
    let app = &mut harness.state_mut().1;
    app.prefs.usage.set(
        "claude",
        Plan {
            billing: Billing::Subscription,
            window: Window::Day,
            price: None,
            cap: Some(Cap::Tokens(1_000)),
        },
    );
    log_run(app, "claude", 210, 0.42);
    harness.run_steps(2);

    harness.get_by_label_contains("Claude.ai · 21% of the day used");
    assert!(
        harness
            .query_all_by_label_contains("DeepSeek · ")
            .next()
            .is_none(),
        "a metered provider is not in the pill"
    );

    harness
        .get_all_by_label_contains("Claude.ai · 21% of the day used")
        .next()
        .unwrap()
        .click();
    harness.run_steps(2);
    assert!(
        harness.query_all_by_label("Usage").next().is_some(),
        "the popover is up"
    );
    harness.get_by_label("Under pace");
    press(&mut harness, "Settings › AI");
    let dialog = harness.state().1.settings.as_ref().expect("Settings opens");
    assert_eq!(dialog.tab, settings_ui::Tab::Review);
}

/// Over pace, the pill says so where the pointer and VoiceOver read it.
#[test]
fn a_subscription_over_pace_says_so_on_the_pill() {
    let dir = scratch_dir("subscription-over-pace");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let mut harness = kittest(app);
    let app = &mut harness.state_mut().1;
    app.prefs.usage.set(
        "claude",
        Plan {
            billing: Billing::Subscription,
            window: Window::Day,
            price: None,
            cap: Some(Cap::Tokens(1_000)),
        },
    );
    log_run(app, "claude", 1_000, 2.0);
    harness.run_steps(2);

    harness.get_by_label_contains("Claude.ai · 100% of the day used · over pace");
}

/// No cap is no percentage: the pill reads `Usage`, and its tooltip still
/// gives the tokens the window burnt.
#[test]
fn a_subscription_with_no_cap_reads_usage_and_its_tokens() {
    let dir = scratch_dir("subscription-unknown");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let mut harness = kittest(app);
    let app = &mut harness.state_mut().1;
    app.prefs.usage.set(
        "claude",
        Plan {
            billing: Billing::Subscription,
            window: Window::Week,
            price: None,
            cap: None,
        },
    );
    log_run(app, "claude", 3_100_000, 3.0);
    harness.run_steps(2);

    harness.get_by_label_contains("Claude.ai · No cap set · 3.1M tokens this week");
    assert!(
        harness
            .query_all_by_label_contains("% of the week used")
            .next()
            .is_none(),
        "no made-up percentage"
    );
    harness
        .get_all_by_label_contains("Claude.ai · No cap set")
        .next()
        .unwrap()
        .click();
    harness.run_steps(2);
    harness.get_by_label("Claude.ai");
    assert!(
        harness.query_all_by_label("No cap").next().is_some(),
        "the row knows no share"
    );
}

/// Copilot's share, used and cap come from GitHub's report, in AI credits,
/// with its reset date and no cap to type.
#[test]
fn a_copilot_quota_reports_ai_credits_and_its_reset() {
    let dir = scratch_dir("subscription-copilot-quota");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let mut harness = kittest(app);
    let app = &mut harness.state_mut().1;
    app.prefs
        .usage
        .set("claude", plan(Billing::Metered, Window::Week, None));
    app.session.quotas = vec![crate::agent::Quota {
        provider: "copilot".to_string(),
        window: Window::Month,
        measure: crate::agent::Measure::Count {
            used: 9_193,
            cap: 30_000,
            unit: "AI credits",
        },
        reset: Utc::now() + chrono::Duration::days(19),
    }];
    log_run(app, "copilot", 500, 3.0);
    harness.run_steps(2);

    harness.get_by_label_contains("Copilot · 31% of the month used");
}

/// Claude.ai reports a session and a week: the pill names both windows, and
/// the popover gives each its own row.
#[test]
fn a_provider_with_a_session_and_a_week_shows_two_rows() {
    let dir = scratch_dir("subscription-two-windows");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let mut harness = kittest(app);
    let app = &mut harness.state_mut().1;
    let now = Utc::now();
    app.session.quotas = vec![
        crate::agent::Quota {
            provider: "claude".to_string(),
            window: Window::Session,
            measure: crate::agent::Measure::Share(0.62),
            reset: now + chrono::Duration::hours(2),
        },
        crate::agent::Quota {
            provider: "claude".to_string(),
            window: Window::Week,
            measure: crate::agent::Measure::Share(0.31),
            reset: now + chrono::Duration::days(4),
        },
    ];
    log_run(app, "claude", 210, 0.42);
    harness.run_steps(2);

    harness.get_by_label_contains("Claude.ai · Session · 62% of the session used");
    harness.get_by_label_contains("Claude.ai · Week · 31% of the week used");
    harness
        .get_all_by_label_contains("Claude.ai · Session · 62% of the session used")
        .next()
        .unwrap()
        .click();
    harness.run_steps(10);
    harness.get_by_label("Claude.ai · Session");
    harness.get_by_label("Claude.ai · Week");
    harness.get_by_label("62%");
    harness.get_by_label("31%");
}

/// A board with no subscription plan shows the spend alone: a metered cap
/// never fills a share.
#[test]
fn the_usage_pill_is_only_for_subscriptions() {
    let dir = scratch_dir("subscription-metered");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let mut harness = kittest(app);
    let app = &mut harness.state_mut().1;
    for provider in ["anthropic", "deepseek", "claude", "copilot"] {
        app.prefs.usage.set(
            provider,
            Plan {
                billing: Billing::Metered,
                cap: Some(Cap::Money(10.0)),
                ..Plan::default()
            },
        );
    }
    harness.run_steps(2);
    harness.get_by_label_contains("Today $0.00, this week $0.00");
    assert!(
        harness
            .query_all_by_label_contains("No cap set")
            .next()
            .is_none(),
        "no usage pill"
    );
    assert!(
        harness
            .query_all_by_label_contains("% of the")
            .next()
            .is_none(),
        "no pill fills from a metered cap"
    );
    assert!(
        harness
            .query_all_by_label_contains("tokens burnt")
            .next()
            .is_none()
    );
}

/// Off the board, with no plan window to show, the bar carries no trigger at
/// all.
#[test]
fn no_trigger_without_a_spend_or_a_plan_window() {
    let dir = scratch_dir("usage-none");
    let file = dir.join("issues/todo/a.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "# Title\n").unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let mut harness = kittest(app);
    let app = &mut harness.state_mut().1;
    for provider in ["anthropic", "deepseek", "claude", "copilot"] {
        app.prefs
            .usage
            .set(provider, plan(Billing::Metered, Window::Week, None));
    }
    app.open_issue(file);
    harness.run_steps(2);

    assert!(
        harness
            .query_all_by_label_contains("Today $")
            .next()
            .is_none(),
        "no spend off the board"
    );
    assert!(
        harness
            .query_all_by_label_contains("% of the")
            .next()
            .is_none(),
        "no plan window"
    );
}

/// A plan for `provider` as Settings declares it.
fn plan(billing: Billing, window: Window, cap: Option<Cap>) -> Plan {
    Plan {
        billing,
        window,
        price: None,
        cap,
    }
}

/// A board with Claude.ai capped by the week and Copilot with no cap, its
/// usage popover open.
fn usage_popover(tag: &str) -> Board {
    let dir = scratch_dir(tag);
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let mut harness = kittest(app);
    let app = &mut harness.state_mut().1;
    app.prefs.usage.set(
        "claude",
        plan(
            Billing::Subscription,
            Window::Week,
            Some(Cap::Tokens(1_000)),
        ),
    );
    app.prefs
        .usage
        .set("copilot", plan(Billing::Subscription, Window::Month, None));
    log_run_of(app, "claude", Some("claude-sonnet-4-5"), 210, 0.42);
    log_run_of(app, "copilot", Some("gpt-5"), 80, 0.10);
    harness.run_steps(2);
    harness
        .get_all_by_label_contains("Claude.ai · 21% of the week used")
        .next()
        .unwrap()
        .click();
    harness.run_steps(5);
    harness
}

/// The popover names a known row with its share and verdict, and an unknown
/// one with `No cap` and its reset. The spend is its first row: today, the
/// week and the last session.
#[test]
fn the_usage_popover_names_a_known_and_an_unknown_row() {
    let harness = usage_popover("usage-rows");

    harness.get_by_label("Today");
    harness.get_by_label("$0.00");
    harness.get_by_label("This week · $0.00");
    harness.get_by_label("Last session · $0.00");
    harness.get_by_label("Claude.ai");
    harness.get_by_label("21%");
    harness.get_by_label("Under pace");
    harness.get_by_label("Copilot");
    harness.get_by_label("No cap");
    harness.get_by_label_contains("Resets in");
}

/// The foot's link opens Settings on the AI review tab and closes the popover.
#[test]
fn the_popover_foot_opens_settings_on_the_ai_tab() {
    let mut harness = usage_popover("usage-settings");
    press(&mut harness, "Settings › AI");

    let dialog = harness.state().1.settings.as_ref().expect("Settings opens");
    assert_eq!(dialog.tab, settings_ui::Tab::Review);
    assert!(
        harness.query_all_by_label("Usage").next().is_none(),
        "the popover closes"
    );
}

/// `Full usage` opens Tools › Usage… and closes the popover.
#[test]
fn full_usage_opens_the_usage_screen_and_closes_the_popover() {
    let mut harness = usage_popover("usage-full");
    press(&mut harness, "Full usage");

    assert!(harness.state().1.usage.open, "the Usage screen opens");
    assert!(
        harness.query_all_by_label("Full usage").next().is_none(),
        "the popover closes"
    );
}

/// `esc` closes the popover, as a click outside does.
#[test]
fn escape_closes_the_usage_popover() {
    let mut harness = usage_popover("usage-escape");
    assert!(
        harness.query_all_by_label("Usage").next().is_some(),
        "the popover is up"
    );

    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(
        harness.query_all_by_label("Usage").next().is_none(),
        "esc closed it"
    );
}

/// An entry of `project` the log keeps, ended now.
fn spend_entry(project: &Path, session: &str, billing: Billing, cost: f64) -> Entry {
    let at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    Entry {
        session: session.to_string(),
        project: project.to_path_buf(),
        engine: "opencode".to_string(),
        provider: Some("deepseek".to_string()),
        model: None,
        billing,
        cost: Some(cost),
        tokens: Tokens::default(),
        started: at.clone(),
        ended: at,
    }
}

/// The board's trigger carries what the projects spent today; a click opens
/// the popover with the week and the last finished session.
#[test]
fn the_board_shows_what_it_spent_today() {
    let (_dir, app) = project("board-spend", &[("todo/a.md", "# A\n")]);
    let mut harness = kittest(app);
    let root = harness.state().1.located.project_dir.clone();
    harness.state_mut().1.usage_entries = vec![spend_entry(&root, "ses_1", Billing::Metered, 0.42)];
    harness.run_steps(2);

    harness
        .get_by_label_contains("Today $0.42, this week $0.42")
        .click();
    harness.run_steps(4);
    harness.get_by_label("$0.42");
    harness.get_by_label("This week · $0.42");
    harness.get_by_label("Last session · $0.42");
}

/// A board that spent nothing says `$0.00`.
#[test]
fn a_board_that_spent_nothing_shows_zero() {
    let (_dir, app) = project("board-spend-nothing", &[("todo/a.md", "# A\n")]);
    let harness = kittest(app);
    harness.get_by_label_contains("Today $0.00, this week $0.00");
}

/// While an agent works, the popover names the build and adds what the live
/// session has cost so far to the log's finished spend today.
#[test]
fn the_board_builds_its_figure_from_the_live_session() {
    let (_dir, app) = project("board-spend-live", &[("todo/a.md", "# A\n")]);
    let mut harness = kittest(app);
    let root = harness.state().1.located.project_dir.clone();
    let app = &mut harness.state_mut().1;
    app.usage_entries = vec![spend_entry(&root, "ses_1", Billing::Metered, 0.30)];
    app.session.live_spend = agent::spend::Live {
        sessions: 1,
        cost: 0.12,
    };
    harness.run_steps(2);

    harness
        .get_by_label_contains("Building $0.42, this week $0.30")
        .click();
    harness.run_steps(4);
    harness.get_by_label("Building");
    harness.get_by_label("$0.42");
}

/// The keys slide in to the left of the trigger, which stays where it is.
#[test]
fn the_spend_stays_put_while_the_keys_slide_in() {
    let dir = scratch_dir("board-spend-hints");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut harness = board(&dir);
    let root = harness.state().1.located.project_dir.clone();
    harness.state_mut().1.usage_entries = vec![spend_entry(&root, "ses_1", Billing::Metered, 0.42)];
    harness.run_steps(2);
    let label = "Today $0.42, this week $0.42";
    let at_rest = harness.get_by_label_contains(label).rect();

    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    harness.run_steps(7);
    assert_eq!(harness.get_by_label_contains(label).rect(), at_rest);
    assert!(
        harness.get_by_label("filter").rect().max.x <= at_rest.min.x,
        "the hints slide in to its left"
    );
}

/// A group board sums the spend of its projects.
#[test]
fn a_group_board_sums_its_projects_spend() {
    let parent = scratch_dir("board-spend-group");
    for name in ["app", "site"] {
        std::fs::create_dir_all(parent.join(name).join("issues/todo")).unwrap();
    }
    let config = parent.join("iboard.toml");
    let (app_dir, site_dir) = (parent.join("app"), parent.join("site"));
    std::fs::write(
        &config,
        format!(
            "[[workspace.group]]\nname = \"Clients\"\nrepos = [{:?}, {:?}]\n",
            app_dir.display().to_string(),
            site_dir.display().to_string(),
        ),
    )
    .unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&app_dir).unwrap());
    app.config_file = Some(config.clone());
    let mut user = UserConfig::load(Some(&config));
    app.workspace = user
        .section::<crate::workspace::WorkspaceSection>("workspace")
        .workspace();
    app.open_group("Clients");
    settled(&mut app);
    app.usage_entries = vec![
        spend_entry(&app_dir, "ses_app", Billing::Metered, 0.25),
        spend_entry(&site_dir, "ses_site", Billing::Metered, 0.40),
    ];
    let harness = kittest(app);

    harness.get_by_label_contains("Today $0.65");
}
