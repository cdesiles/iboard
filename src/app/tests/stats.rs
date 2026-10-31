//! Project stats' tests: measuring the history, its Agenda, closing the panel.

use egui_kittest::kittest::Queryable;

use super::*;

/// A board in a repository of three commits, each adding code.
fn project(tag: &str) -> (Scratch, Located) {
    let dir = scratch_dir(tag);
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&*dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    };
    git(&["init", "-q"]);
    let mut code = String::new();
    for name in ["one", "two", "three"] {
        code.push_str(&format!("fn {name}() {{}}\n"));
        std::fs::write(dir.join("src/lib.rs"), &code).unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", name]);
    }
    let located = discovery::locate(&dir).unwrap();
    (dir, located)
}

/// The panel measures every commit, shows its figures, trends, watch and
/// files, moves the Agenda's day with `←`, and goes with `esc`.
#[test]
fn the_stats_panel_measures_the_history_and_walks_the_agenda() {
    let (_dir, located) = project("stats");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
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
    assert!(until(|| {
        harness.run_steps(1);
        !harness
            .state()
            .1
            .session
            .stats
            .as_ref()
            .unwrap()
            .is_loading()
    }));
    harness.run_steps(2);
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    let code: Vec<usize> = panel.snapshots().iter().map(|s| s.code).collect();
    assert_eq!(code, [1, 2, 3]);
    let today = panel.agenda.selected;
    for label in [
        "MEDIAN CYCLE TIME",
        "Lines of code trend",
        "REFACTOR WATCH",
        "No refactor needed now",
        "src/lib.rs",
        "Agenda",
    ] {
        harness.get_by_label(label);
    }

    harness.key_press(Key::ArrowLeft);
    harness.run_steps(2);
    let selected = harness
        .state()
        .1
        .session
        .stats
        .as_ref()
        .unwrap()
        .agenda
        .selected;
    assert_eq!(selected, today - chrono::Duration::days(7));

    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(harness.state().1.session.stats.is_none());
}

/// Outside a repository the panel says why it has nothing to show.
#[test]
fn the_stats_panel_explains_a_project_without_history() {
    let dir = scratch_dir("stats-none");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_stats();
    let ctx = themed_ctx();
    assert!(until(|| {
        frame(&ctx, &mut app, Vec::new());
        app.session.stats.as_ref().is_some_and(|s| !s.is_loading())
    }));
    assert!(app.session.stats.as_ref().unwrap().snapshots().is_empty());
}

/// The board's Stats button opens the panel, as Tools › Project Stats… does.
#[test]
fn the_stats_button_opens_the_panel() {
    let (_dir, located) = project("stats-button");
    let app = IboardApp::from_located(located);
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
    assert!(harness.state().1.session.stats.is_none());
    harness.get_by_label("Stats").click();
    harness.run_steps(2);
    assert!(harness.state().1.session.stats.is_some());
}

/// `⌘I` on the board opens the panel too.
#[test]
fn cmd_i_opens_the_stats() {
    let (_dir, located) = project("stats-key");
    let mut app = IboardApp::from_located(located);
    let ctx = themed_ctx();
    let command = egui::Modifiers::COMMAND;
    let press = vec![egui::Event::ModifiersChanged(command), key(Key::I, command)];
    frame(&ctx, &mut app, press);
    assert!(app.session.stats.is_some());
}
