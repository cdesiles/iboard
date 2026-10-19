//! Project stats' tests: measuring the history, walking it, closing the panel.

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

/// The panel measures every commit, shows the newest, walks back with `←`,
/// and goes with `esc`.
#[test]
fn the_stats_panel_measures_and_walks_the_history() {
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
    assert_eq!(panel.selected, 2);
    harness.get_by_label("Trend across commits");
    harness.get_by_label("three");
    harness.get_by_label("Cycle time");

    harness.key_press(Key::ArrowLeft);
    harness.run_steps(2);
    assert_eq!(
        harness.state().1.session.stats.as_ref().unwrap().selected,
        1
    );
    harness.get_by_label("two");

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
