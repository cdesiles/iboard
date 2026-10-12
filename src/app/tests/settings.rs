//! The Settings dialog's tests: saving it, its file, and the theme.

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
    assert!(!app.agent.configured());

    app.settings.as_mut().unwrap().form.command = "llm < {prompt}".to_string();
    frame(&mut app, vec![key(Key::Enter, egui::Modifiers::COMMAND)]);
    assert!(app.settings.is_none(), "saved and closed");
    assert!(app.agent.configured());
    assert_eq!(app.agent.label(), "custom · default");

    app.open_config();
    frame(&mut app, vec![key(Key::Escape, egui::Modifiers::NONE)]);
    assert!(app.settings.is_none(), "esc closes the dialog");
    assert!(app.session.review.is_some(), "and leaves the review alone");
}

/// The same flow through egui_kittest: widgets found by their label, as a
/// screen reader sees them.
#[test]
fn settings_save_by_clicking_its_labelled_button() {
    let (_dir, _file, mut app) = open_issue_with_review("kittest-save");
    app.open_title_settings();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(window().size())
        .build_ui_state(
            |ui, (themed, app): &mut (bool, IboardApp)| {
                // Plex is registered on the first frame and usable from the next.
                if !*themed {
                    theme::install(ui.ctx());
                    *themed = true;
                    return;
                }
                app.frame_logic(ui.ctx());
                app.draw(ui);
            },
            (false, app),
        );
    harness.run_steps(2);
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
    let mut harness = egui_kittest::Harness::builder()
        .with_size(window().size())
        .build_ui_state(
            |ui, (themed, app): &mut (bool, IboardApp)| {
                if !*themed {
                    theme::install(ui.ctx());
                    *themed = true;
                    return;
                }
                app.frame_logic(ui.ctx());
                app.draw(ui);
            },
            (false, app),
        );
    harness.run_steps(2);
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
    app.settings.as_mut().unwrap().engine = Engine::Manual;
    save(&mut app);
    let error = app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.save_error.clone());
    assert!(error.is_some_and(|error| error.contains("line 1")));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), broken);
}
