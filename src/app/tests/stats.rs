//! Project stats' tests: measuring the history, its Agenda, each block on its
//! own, closing the panel.

use egui_kittest::kittest::{NodeT, Queryable};
use iboard::stats::Stats;

use super::*;
use crate::app::stats::Measure;
use crate::app::stats::blocks::{Part, Phase};
use crate::job::Job;
use crate::scratch_git::git;

/// A window tall enough to show every block, so each starts.
fn tall(app: IboardApp) -> kit::Kit {
    let size = egui::vec2(window().width(), 2000.0);
    kit::kittest_with(app, egui_kittest::Harness::builder().with_size(size))
}

fn settled(harness: &mut kit::Kit) -> bool {
    until(|| {
        harness.run_steps(1);
        !harness
            .state()
            .1
            .session
            .stats
            .as_ref()
            .unwrap()
            .is_loading()
    })
}

/// A read that lasts until it is dropped.
fn slow() -> Job<Result<Stats, String>> {
    Job::spawn(|cancel| {
        while !cancel.is_cancelled() {
            std::thread::sleep(Duration::from_millis(5));
        }
        Err("stopped".to_string())
    })
}

/// A board in a repository of three commits, each adding code.
fn project(tag: &str) -> (Scratch, Located) {
    let dir = scratch_dir(tag);
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    git(&dir, &["init", "-q"]);
    let mut code = String::new();
    for name in ["one", "two", "three"] {
        code.push_str(&format!("fn {name}() {{}}\n"));
        std::fs::write(dir.join("src/lib.rs"), &code).unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", name]);
    }
    let located = discovery::locate(&dir).unwrap();
    (dir, located)
}

/// Polls until the panel has measured: a measure still at work once the test
/// is over would write its cache into the scratch folder, gone by then, and
/// so make it again.
fn measured(app: &mut IboardApp) -> bool {
    until(|| {
        app.poll_stats();
        app.session.stats.as_ref().is_some_and(|s| !s.is_loading())
    })
}

/// The panel measures every commit, shows its figures, trends, watch and
/// files, moves the Agenda's day with `←`, and goes with `esc`.
#[test]
fn the_stats_panel_measures_the_history_and_walks_the_agenda() {
    let (_dir, located) = project("stats");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
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

#[test]
fn the_agendas_range_is_remembered() {
    let (dir, located) = project("stats-range");
    let file = dir.join("user.toml");
    let mut app = IboardApp::from_located(located);
    app.config_file = Some(file.clone());
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.key_press(Key::Num3);
    harness.run_steps(2);
    let mut config = UserConfig::load(Some(&file));
    let remembered = Preferences::load(&mut config).agenda_range;
    assert_eq!(remembered, iboard::agenda::Range::W26);
    assert_eq!(harness.state().1.prefs.agenda_range, remembered);
}

/// Once the history is in, the screen stops asking for frames, and the
/// Agenda still draws its days.
#[test]
fn the_measured_stats_screen_stops_repainting() {
    let (_dir, located) = project("stats-idle");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    theme::set_reduced_motion(&harness.ctx, false);
    assert!(settled(&mut harness));
    harness
        .try_run()
        .expect("the measured screen stops asking for frames");
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    let today = panel.agenda.selected.format("%a %-d %b, ").to_string();
    harness.get_by_label_contains(&today);
}

/// Outside a repository each block says why it has nothing to show.
#[test]
fn the_stats_panel_explains_a_project_without_history() {
    let dir = scratch_dir("stats-none");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_stats();
    let mut harness = kittest(app);
    assert!(settled(&mut harness));
    harness.run_steps(1);
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    assert!(panel.snapshots().is_empty());
    let says = "Couldn't read commit measures: This project has no commits";
    assert!(harness.get_all_by_label_contains(says).count() > 1);
    assert!(harness.get_all_by_label("Retry").count() > 1);
}

/// A failed block says so with Retry and keeps its size, its neighbours
/// stay, and Retry reads it alone.
#[test]
fn a_failed_block_keeps_its_size_and_retries_alone() {
    let (_dir, located) = project("stats-failed");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(1);
    let agenda_y = harness.get_by_label("Agenda").rect().top();
    let panel = harness.state_mut().1.session.stats.as_mut().unwrap();
    panel
        .blocks
        .reading(Part::Files, Job::done(Err("boom".to_string())));
    harness.run_steps(2);
    harness.get_by_label("Couldn't read biggest files: boom");
    harness.get_by_label("No refactor needed now");
    assert_eq!(harness.get_by_label("Agenda").rect().top(), agenda_y);
    assert!(harness.query_by_label("src/lib.rs").is_none());

    kit::press(&mut harness, "Retry");
    assert!(settled(&mut harness));
    harness.run_steps(1);
    harness.get_by_label("src/lib.rs");
    assert!(harness.query_by_label("Retry").is_none());
}

/// A read past its timeout fails that block; the others have long landed.
#[test]
fn a_slow_block_times_out_alone() {
    let (_dir, located) = project("stats-slow");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    let part = Part::Trend(Measure::Complexity);
    let panel = harness.state_mut().1.session.stats.as_mut().unwrap();
    panel.blocks.reading(part, slow());
    harness.run_steps(2);
    let panel = harness.state_mut().1.session.stats.as_mut().unwrap();
    assert!(panel.is_loading());
    assert!(panel.blocks.loaded(Part::Tile(Measure::Code)).is_some());
    panel.blocks.timeout = Duration::ZERO;
    harness.run_steps(2);
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    assert!(!panel.is_loading());
    assert!(matches!(panel.blocks.phase(part), Phase::Failed(_)));
    harness.get_by_label("Lines of code trend");
}

/// The Agenda is a `grid` holding its days as buttons.
#[test]
fn the_agenda_is_a_grid_of_day_buttons() {
    use egui::accesskit::Role;
    let (_dir, located) = project("stats-grid");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);
    let grid = harness.get_by_role_and_label(Role::Grid, "Commits per day");
    let mut days = grid.accesskit_node().children();
    assert!(days.any(|day| day.role() == Role::Button));
}

/// Placeholders sit at the blocks' final size: nothing moves as they fill.
/// "Loading stats" is one label on the whole screen, gone once all are in.
#[test]
fn nothing_moves_as_the_blocks_fill() {
    let (_dir, located) = project("stats-still");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let blocks = &mut app.session.stats.as_mut().unwrap().blocks;
    let rows = Measure::ALL
        .into_iter()
        .flat_map(|measure| [Part::Tile(measure), Part::Trend(measure)]);
    for part in rows.chain([Part::Watch, Part::Files, Part::Agenda]) {
        blocks.reading(part, slow());
    }
    let mut harness = tall(app);
    let tops = |harness: &kit::Kit| {
        ["BIGGEST FILES", "Agenda"].map(|label| harness.get_by_label(label).rect().top())
    };
    let before = tops(&harness);
    assert_eq!(harness.get_all_by_label("Loading stats").count(), 1);

    let panel = harness.state_mut().1.session.stats.as_mut().unwrap();
    panel.refresh();
    assert!(settled(&mut harness));
    harness.run_steps(2);
    assert_eq!(tops(&harness), before);
    assert!(harness.query_by_label("Loading stats").is_none());
    assert!(
        harness
            .state()
            .1
            .session
            .stats
            .as_ref()
            .unwrap()
            .blocks
            .loaded(Part::Files)
            .is_some()
    );
}

/// A block off screen reads nothing until it is seen.
#[test]
fn a_block_below_the_fold_waits_until_it_is_seen() {
    let (_dir, located) = project("stats-lazy");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = kittest(app);
    assert!(settled(&mut harness));
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    assert!(!panel.blocks.started(Part::Agenda));
    assert!(panel.blocks.started(Part::Tile(Measure::Code)));

    harness.set_size(egui::vec2(window().width(), 2000.0));
    harness.run_steps(2);
    assert!(settled(&mut harness));
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    assert!(panel.blocks.loaded(Part::Agenda).is_some());
}

/// `⌘R` keeps the old numbers until the new ones land, and picks up new
/// commits.
#[test]
fn refresh_keeps_the_old_numbers_until_the_new_ones_land() {
    let (dir, located) = project("stats-refresh");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let ctx = themed_ctx();
    assert!(until(|| {
        frame(&ctx, &mut app, Vec::new());
        app.session.stats.as_ref().is_some_and(|s| !s.is_loading())
    }));
    std::fs::write(dir.join("src/more.rs"), "fn more() {}\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "more"]);
    let command = egui::Modifiers::COMMAND;
    let press = vec![egui::Event::ModifiersChanged(command), key(Key::R, command)];
    frame(&ctx, &mut app, press);
    let part = Part::Trend(Measure::Code);
    let panel = app.session.stats.as_ref().unwrap();
    match panel.blocks.phase(part) {
        Phase::Loaded { stats, stale } => {
            assert!(stale);
            assert_eq!(stats.snapshots.len(), 3);
        }
        _ => panic!("the old numbers went away"),
    }
    assert!(until(|| {
        frame(&ctx, &mut app, Vec::new());
        app.session.stats.as_ref().is_some_and(|s| !s.is_loading())
    }));
    let panel = app.session.stats.as_ref().unwrap();
    assert_eq!(panel.snapshots().len(), 4);
    assert!(matches!(
        panel.blocks.phase(part),
        Phase::Loaded { stale: false, .. }
    ));
}

/// The board's Stats button opens the panel, as Tools › Project Stats… does.
#[test]
fn the_stats_button_opens_the_panel() {
    let (_dir, located) = project("stats-button");
    let app = IboardApp::from_located(located);
    let mut harness = kittest(app);
    assert!(harness.state().1.session.stats.is_none());
    harness.get_by_label("Stats").click();
    harness.run_steps(2);
    assert!(harness.state().1.session.stats.is_some());
    assert!(measured(&mut harness.state_mut().1));
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
    assert!(measured(&mut app));
}

/// The screen's head replaces the board's: the crumbs, Refresh, and the
/// project leading back to the board.
#[test]
fn the_stats_head_replaces_the_board_header() {
    let (_dir, located) = project("stats-head");
    let mut app = IboardApp::from_located(located);
    let project = app.board.name();
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(1);
    harness.get_by_label("Stats");
    harness.get_by_label("Refresh");
    assert!(harness.query_by_label("New issue").is_none());
    harness.get_by_label(&project).click();
    harness.run_steps(2);
    assert!(harness.state().1.session.stats.is_none());
}

/// A column head sorts as the segmented control does.
#[test]
fn a_column_head_sorts_the_files() {
    let (dir, located) = project("stats-sort");
    std::fs::write(
        dir.join("src/branchy.rs"),
        "fn b(x: bool) {\n    if x {}\n    if x {}\n    if x {}\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/long.rs"), "const A: u8 = 1;\n".repeat(20)).unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "more"]);
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(1);
    let top = |harness: &kit::Kit, path: &str| harness.get_by_label(path).rect().top();
    assert!(top(&harness, "src/long.rs") < top(&harness, "src/branchy.rs"));

    harness.get_by_label("Sort by Complexity").click();
    harness.run_steps(2);
    assert!(top(&harness, "src/branchy.rs") < top(&harness, "src/long.rs"));
}

/// A click on a row lists the file's commits; `esc` closes the history and
/// leaves the stats up.
#[test]
fn a_file_row_opens_its_history() {
    let (_dir, located) = project("stats-history");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(1);
    harness.get_by_label("src/lib.rs").click();
    harness.run_steps(2);
    // The Agenda's day lists the same subjects, under the popup.
    assert!(until(|| {
        harness.run_steps(1);
        harness.get_all_by_label("three").count() == 2
    }));
    for subject in ["one", "two"] {
        assert_eq!(harness.get_all_by_label(subject).count(), 2);
    }

    harness.key_press(Key::Escape);
    harness.run_steps(2);
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    assert!(panel.history.is_none());
    assert_eq!(harness.get_all_by_label("three").count(), 1);
}
