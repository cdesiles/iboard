//! The Settings dialog's tests: saving it, its file, the theme and Auto.

use super::*;

/// `⌘↩` in the settings dialog saves; a custom command without
/// `{prompt}` keeps it open, and `esc` there never cancels the review.
#[test]
fn the_settings_dialog_saves_with_command_enter() {
    let (_dir, _file, mut app) = open_issue_with_review("settings-save");
    let ctx = themed_ctx();
    let frame = |app: &mut IboardApp, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| app.draw(ui)).textures_delta.clear();
    };

    app.open_config();
    app.settings.as_mut().unwrap().form = AgentConfig {
        tool: agent::Tool::Custom,
        command: "llm".to_string(),
        ..AgentConfig::default()
    };
    frame(&mut app, vec![key(Key::Enter, egui::Modifiers::COMMAND)]);
    assert!(app.settings.is_some(), "an invalid command keeps it open");
    assert!(!app.prefs.agent.configured());

    app.settings.as_mut().unwrap().form.command = "llm < {prompt}".to_string();
    frame(&mut app, vec![key(Key::Enter, egui::Modifiers::COMMAND)]);
    assert!(app.settings.is_none(), "saved and closed");
    assert!(app.prefs.agent.configured());
    assert_eq!(app.prefs.agent.label(), "custom · default");

    app.open_config();
    frame(&mut app, vec![key(Key::Escape, egui::Modifiers::NONE)]);
    assert!(app.settings.is_none(), "esc closes the dialog");
    assert!(app.session.review.is_some(), "and leaves the review alone");
}

/// Settings › AI build: the Timeout select opens on 30 minutes, and picking
/// from it is saved with `[build]`.
#[test]
fn the_build_timeout_is_picked_and_saved_with_the_build_section() {
    use egui_kittest::kittest::Queryable;
    let (dir, _file, mut app) = open_issue_with_review("build-timeout");
    let config = dir.join("iboard.toml");
    app.config_file = Some(config.clone());
    app.open_settings(settings_ui::Tab::Build);
    let mut harness = kittest(app);

    harness.get_by_label("30 minutes").click();
    harness.run_steps(2);
    harness.get_by_label("No limit").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().1.settings.as_ref().unwrap().build_timeout,
        agent::BuildTimeout::NONE
    );

    harness.get_by_label("Save").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().1.prefs.build_timeout,
        agent::BuildTimeout::NONE
    );
    let text = std::fs::read_to_string(&config).unwrap();
    assert!(
        text.contains("[build]") && text.contains("timeout = \"none\""),
        "{text}"
    );
}

/// Settings › AI build: the number of build agents, the same count Doing's
/// header sets, is shown and saved with `[build]`.
#[test]
fn the_build_agent_count_is_picked_and_saved_with_the_build_section() {
    use egui_kittest::kittest::Queryable;
    let (dir, _file, mut app) = open_issue_with_review("build-agents");
    let config = dir.join("iboard.toml");
    app.config_file = Some(config.clone());
    app.open_settings(settings_ui::Tab::Build);
    let mut harness = kittest(app);

    harness.get_by_label("1 agent").click();
    harness.run_steps(2);
    harness.get_by_label("4 agents").click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().1.settings.as_ref().unwrap().agents,
        4,
        "the tab shows the chosen count"
    );

    harness.get_by_label("Save").click();
    harness.run_steps(2);
    assert_eq!(harness.state().1.prefs.agents, 4);
    assert_eq!(harness.state().1.session.agents, 4, "and the board uses it");
    let text = std::fs::read_to_string(&config).unwrap();
    assert!(
        text.contains("[build]") && text.contains("agents = 4"),
        "{text}"
    );
}

/// Settings › General: the agent log switch is saved with `[notifications]`.
#[test]
fn the_agent_log_switch_is_saved_with_the_notifications_section() {
    use egui_kittest::kittest::Queryable;
    let (dir, _file, mut app) = open_issue_with_review("notifications-save");
    let config = dir.join("iboard.toml");
    app.config_file = Some(config.clone());
    app.open_settings(settings_ui::Tab::General);
    let mut harness = kittest(app);

    harness
        .get_by_label("Open the agent log on a Doing issue")
        .click();
    harness.run_steps(2);
    assert!(
        !harness
            .state()
            .1
            .settings
            .as_ref()
            .unwrap()
            .notifications
            .auto_log
    );
    harness.run_steps(12);
    harness.get_by_label("Save").click();
    harness.run_steps(2);
    assert!(!harness.state().1.prefs.notifications.auto_log);
    let text = std::fs::read_to_string(&config).unwrap();
    assert!(
        text.contains("[notifications]") && text.contains("auto_log = false"),
        "{text}"
    );
}

/// The same flow through egui_kittest: widgets found by their label, as a
/// screen reader sees them.
#[test]
fn settings_save_by_clicking_its_labelled_button() {
    let (_dir, _file, mut app) = open_issue_with_review("kittest-save");
    app.open_title_settings();
    let mut harness = kittest(app);
    assert!(harness.state().1.settings.is_some());

    egui_kittest::kittest::Queryable::get_by_label(&harness, "Save").click();
    harness.run_steps(2);

    assert!(harness.state().1.settings.is_none(), "Save closed Settings");
}

/// The theme is light until Settings › General picks Dark; saving draws the
/// window dark at once and writes `[appearance]`.
#[test]
fn the_theme_is_light_until_settings_picks_dark() {
    use egui_kittest::kittest::Queryable;
    let (dir, _file, mut app) = open_issue_with_review("theme");
    let config = dir.join("iboard.toml");
    app.config_file = Some(config.clone());
    app.open_settings(settings_ui::Tab::General);
    let mut harness = kittest(app);
    assert_eq!(harness.ctx.theme(), egui::Theme::Light, "light by default");

    harness.get_by_label("Dark").click();
    harness.run_steps(2);
    assert_eq!(harness.ctx.theme(), egui::Theme::Light, "not before Save");
    harness.get_by_label("Save").click();
    harness.run_steps(2);

    assert!(harness.state().1.settings.is_none(), "Save closed Settings");
    assert_eq!(harness.ctx.theme(), egui::Theme::Dark);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "[appearance]\ntheme = \"dark\"\n"
    );
}

/// Auto is on with no toggle on the board; only Settings › General turns it
/// off, which brings Refresh back and writes `[auto]`.
#[test]
fn auto_is_on_and_only_settings_turns_it_off() {
    use egui_kittest::kittest::Queryable;
    let (dir, _file, mut app) = open_issue_with_review("auto");
    let config = dir.join("iboard.toml");
    app.config_file = Some(config.clone());
    app.session.screen = Screen::Board;
    let mut harness = kittest(app);
    assert!(harness.state().1.prefs.auto, "on by default");
    assert!(harness.query_by_label("Auto").is_none(), "not on the board");
    assert!(harness.query_by_label("Refresh").is_none());

    harness
        .state_mut()
        .1
        .open_settings(settings_ui::Tab::General);
    harness.run_steps(2);
    harness
        .get_by_label("Reload and save automatically")
        .click();
    harness.run_steps(2);
    harness.get_by_label("Save").click();
    harness.run_steps(2);

    assert!(!harness.state().1.prefs.auto);
    assert!(
        harness.query_by_label("Refresh").is_some(),
        "Refresh is back"
    );
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "[auto]\nenabled = false\n"
    );
}

/// Save writes only the sections that changed, keeps the rest of the file
/// as written, and never writes over a file that does not parse.
#[test]
fn saving_settings_never_loses_the_users_file() {
    let (dir, _file, mut app) = open_issue_with_review("settings-file");
    let ctx = themed_ctx();
    let save = |app: &mut IboardApp| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events: vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| app.draw(ui)).textures_delta.clear();
    };
    let config = dir.join("iboard.toml");
    let mine = "# mine\n[delete]\nconfirm = true # keep asking\n";
    std::fs::write(&config, mine).unwrap();
    app.config_file = Some(config.clone());

    app.open_title_settings();
    save(&mut app);
    assert!(app.settings.is_none());
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        mine,
        "nothing changed"
    );

    app.open_title_settings();
    app.settings.as_mut().unwrap().engine = Engine::Agent;
    save(&mut app);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        format!("{mine}\n[titles]\nengine = \"agent\"\n")
    );

    let broken = "[titles\nengine = \"agent\"\n";
    std::fs::write(&config, broken).unwrap();
    app.open_title_settings();
    app.settings.as_mut().unwrap().engine = Engine::Off;
    save(&mut app);
    let error = app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.save_error.clone());
    assert!(error.is_some_and(|error| error.contains("line 1")));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), broken);
}

/// Settings › AI review starts on Ticket only; picking Ticket + project
/// context writes `scope` into `[ai]` and keeps the file's comments.
#[test]
fn the_review_scope_is_saved_with_the_review_agent() {
    use egui_kittest::kittest::Queryable;
    let (dir, _file, mut app) = open_issue_with_review("review-scope-save");
    let config = dir.join("iboard.toml");
    let mine = "# mine\n[ai]\ntool = \"claude\" # my agent\nmodel = \"default\"\n";
    std::fs::write(&config, mine).unwrap();
    let mut user = settings::UserConfig::load(Some(&config));
    app.prefs.agent = user.section::<agent::AiSection>("ai").agent().0;
    assert_eq!(
        app.prefs.agent.scope,
        ReviewMode::Quick,
        "an existing file is quick"
    );
    app.config_file = Some(config.clone());
    app.open_config();
    let mut harness = kittest(app);
    harness.get_by_label(ReviewMode::Quick.scope_name());
    harness.get_by_label(ReviewMode::Deep.scope_name()).click();
    harness.run_steps(2);
    harness.get_by_label("Save").click();
    harness.run_steps(2);

    assert!(harness.state().1.settings.is_none(), "Save closed Settings");
    assert_eq!(harness.state().1.prefs.agent.scope, ReviewMode::Deep);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        format!("{mine}scope = \"project\"\n")
    );
}

/// The fix under an unavailable engine sits inside its radio row, and the
/// click goes to the button.
#[test]
fn the_titles_fix_button_takes_its_click_from_the_row() {
    let (_dir, _file, mut app) = open_issue_with_review("titles-fix");
    app.prefs.agent = AgentConfig::default();
    app.open_title_settings();
    let mut harness = kittest(app);
    let engine = harness.state().1.settings.as_ref().unwrap().engine;

    press(&mut harness, "Set up AI review");

    let settings = harness.state().1.settings.as_ref().unwrap();
    assert_eq!(settings.tab, settings_ui::Tab::Review);
    assert_eq!(settings.engine, engine, "the row stays unpicked");
}

/// A row's lines start under its name, not on an indent of their own.
#[test]
fn a_title_engines_help_lines_up_with_its_name() {
    let (_dir, _file, mut app) = open_issue_with_review("titles-indent");
    app.open_title_settings();
    let harness = kittest(app);
    let painted_x = |line: &str| {
        harness
            .output()
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.text() == line => Some(text.pos.x),
                _ => None,
            })
    };
    let name = painted_x(titles::Engine::Agent.name());
    assert!(name.is_some());
    assert_eq!(name, painted_x(titles::Engine::Agent.help()));
}

/// `tab` reaches a Titles row, which shows the focus ring, and `space` picks it.
#[test]
fn a_focused_title_engine_shows_its_ring_and_space_picks_it() {
    use egui_kittest::kittest::Queryable;
    let (_dir, _file, mut app) = open_issue_with_review("titles-focus");
    app.open_title_settings();
    let mut harness = kittest(app);
    harness.state_mut().1.settings.as_mut().unwrap().engine = titles::Engine::Off;
    let row = harness
        .get_all_by_label(titles::Engine::Agent.name())
        .map(|node| node.rect())
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .unwrap();

    let mut reached = false;
    for _ in 0..40 {
        harness.key_press(egui::Key::Tab);
        harness.run_steps(2);
        let focused = harness.ctx.memory(|m| m.focused());
        reached = focused
            .and_then(|id| harness.ctx.read_response(id))
            .is_some_and(|response| response.rect == row);
        if reached {
            break;
        }
    }
    assert!(reached, "tab reaches the row");
    assert!(harness.output().shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::Shape::Rect(shape)
            if shape.stroke.width == 2.0 && shape.rect == row.expand(2.0))
    }));

    harness.key_press(egui::Key::Space);
    harness.run_steps(2);
    assert_eq!(
        harness.state().1.settings.as_ref().unwrap().engine,
        titles::Engine::Agent
    );
}
