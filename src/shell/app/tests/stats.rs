//! Project stats' tests: measuring the history, its Agenda, each block on its
//! own, closing the panel.

use crate::stats::Stats;
use egui_kittest::kittest::{NodeT, Queryable};

use super::*;
use crate::agent::{Billing, Tokens};
use crate::domains::agent::artifacts::usage_log::Entry;
use crate::job::Job;
use crate::scratch_git::git;
use crate::shell::app::stats::Measure;
use crate::shell::app::stats::blocks::{Part, Phase};

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
        "Refactor watch",
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
    assert_eq!(remembered, crate::agenda::Range::W26);
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
        ["Biggest files", "Agenda"].map(|label| harness.get_by_label(label).rect().top())
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

/// The ProjectMenu's Stats row opens the panel, as Tools › Project Stats… does,
/// and the menu closes behind it.
#[test]
fn the_project_menus_stats_row_opens_the_panel() {
    let (_dir, located) = project("stats-button");
    let app = IboardApp::from_located(located);
    let mut harness = kittest(app);
    assert!(harness.state().1.session.stats.is_none());
    assert!(
        harness.query_by_label("Stats").is_none(),
        "not in the header"
    );
    harness.get_by_label("Switch project").click();
    harness.run_steps(2);
    harness.get_by_label("Stats").click();
    harness.run_steps(2);
    assert!(harness.state().1.session.stats.is_some());
    assert!(harness.state().1.project_menu.is_none());
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

/// `project`'s three commits with `v0.1.0` on the first, `v0.1.1` on the
/// third and `nightly`, which isn't a release, on the second.
fn tagged(tag: &str) -> (Scratch, Located) {
    let (dir, located) = project(tag);
    git(&dir, &["tag", "v0.1.0", "HEAD~2"]);
    git(&dir, &["tag", "nightly", "HEAD~1"]);
    git(&dir, &["tag", "v0.1.1", "HEAD"]);
    (dir, located)
}

fn release_label(harness: &kit::Kit, tag: &str) -> String {
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    let stats = panel.blocks.loaded(Part::Releases).unwrap();
    let release = stats.releases.shown.iter().find(|r| r.tag == tag).unwrap();
    release.describe()
}

/// The lane reads each release aloud, and hovering a name turns on its tooltip
/// and moves the shared crosshair to its commit.
#[test]
fn hovering_a_release_says_what_shipped_and_moves_the_crosshair() {
    let (_dir, located) = tagged("stats-releases");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);

    let first = release_label(&harness, "v0.1.0");
    assert!(
        first.starts_with("v0.1.0 · ") && first.ends_with(" · 1 commit"),
        "{first}"
    );
    let second = release_label(&harness, "v0.1.1");
    assert!(second.ends_with(" · 2 commits since v0.1.0"), "{second}");
    assert!(harness.query_by_label_contains("nightly").is_none());

    let spot = harness.get_by_label(&first).rect().center();
    harness.event(moved(spot));
    harness.run_steps(3);
    assert_eq!(
        harness.get_all_by_label(&first).count(),
        2,
        "and its tooltip"
    );
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    assert_eq!(panel.hovered, Some(0));
    assert_eq!(harness.get_all_by_label(&second).count(), 1);
}

/// Unticked, the lane goes, the charts move up by its 16pt, and the choice is
/// saved next to the pattern, keeping comments; a new tag shows on Refresh.
#[test]
fn the_releases_checkbox_is_saved_and_takes_the_lane_away() {
    let (dir, located) = tagged("stats-releases-off");
    let file = dir.join("user.toml");
    std::fs::write(&file, "# mine\n[stats]\nrelease_tags = \"v*\" # ours\n").unwrap();
    let mut app = IboardApp::from_located(located);
    app.config_file = Some(file.clone());
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);
    let label = release_label(&harness, "v0.1.0");
    harness.get_by_label(&label);
    let chart_top = |harness: &kit::Kit| harness.get_by_label("Lines of code trend").rect().top();
    let with_lane = chart_top(&harness);

    harness.get_by_label("Releases").click();
    harness.run_steps(3);

    assert!(harness.query_by_label(&label).is_none());
    assert_eq!(with_lane - chart_top(&harness), 16.0);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# mine\n[stats]\nrelease_tags = \"v*\" # ours\nshow_releases = false\n"
    );
    assert!(!harness.state().1.prefs.releases.show);

    harness.get_by_label("Releases").click();
    harness.run_steps(3);
    git(&dir, &["tag", "v0.2.0"]);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::R);
    harness.run_steps(1);
    assert!(settled(&mut harness));
    harness.run_steps(2);
    harness.get_by_label(&release_label(&harness, "v0.2.0"));
}

/// With no tag matching, the checkbox is disabled and says why.
#[test]
fn the_checkbox_is_disabled_when_no_tag_matches() {
    let (dir, located) = project("stats-releases-none");
    git(&dir, &["tag", "rel-1"]);
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);
    assert!(
        harness
            .get_by_label("Releases")
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label("Releases").hover();
    harness.run_steps(4);
    harness.get_by_label("No release tags match v*");
}

/// `[stats] release_tags` picks another pattern.
#[test]
fn release_tags_pick_another_pattern() {
    let (dir, located) = project("stats-releases-pattern");
    git(&dir, &["tag", "rel-1"]);
    let mut app = IboardApp::from_located(located);
    app.prefs.releases.tags = "rel-*".to_string();
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);
    assert!(
        !harness
            .get_by_label("Releases")
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label(&release_label(&harness, "rel-1"));
}

/// The Agenda counts the open project and the recent ones, groups the day
/// panel by project, and filters from a "By project" row until its token's ×
/// or `esc`.
#[test]
fn the_agenda_counts_every_supervised_project() {
    let (_dir, located) = project("agenda-supervised");
    let (other, _) = project("agenda-other");
    let other_name = crate::files::file_name(&other);
    let mut app = IboardApp::from_located(located);
    app.recents = vec![crate::shell::recents::canonical(&other)];
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);

    let panel = harness.state().1.session.stats.as_ref().unwrap();
    let days = &panel.blocks.loaded(Part::Agenda).unwrap().agenda;
    assert_eq!(days.projects.len(), 2);
    assert_eq!(days.days.values().map(Vec::len).sum::<usize>(), 6);
    harness.get_by_label("6 commits · 2 projects · 1 active day");
    let row = format!("{other_name}, 3 commits, last today");
    harness.get_by_label(&row);
    harness.get_by_label(&format!("{other_name} · 3"));

    harness.get_by_label(&row).click();
    harness.run_steps(2);
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    assert_eq!(panel.agenda.filter(), Some(other_name.as_str()));
    harness.get_by_label("3 commits · 1 active day");
    harness.get_by_label(&format!("Remove {other_name}"));

    press(&mut harness, &format!("Remove {other_name}"));
    harness.get_by_label("6 commits · 2 projects · 1 active day");
    assert!(
        harness
            .state()
            .1
            .session
            .stats
            .as_ref()
            .unwrap()
            .agenda
            .filter()
            .is_none()
    );

    harness.get_by_label(&row).click();
    harness.run_steps(2);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    let panel = harness.state().1.session.stats.as_ref().unwrap();
    assert!(panel.agenda.filter().is_none(), "esc clears the filter");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(
        harness.state().1.session.stats.is_none(),
        "then closes Stats"
    );
}

/// While it reads, the Agenda says how many projects it is reading.
#[test]
fn the_agenda_says_how_many_projects_it_reads() {
    let (_dir, located) = project("agenda-loading");
    let (other, _) = project("agenda-loading-other");
    let mut app = IboardApp::from_located(located);
    app.recents = vec![crate::shell::recents::canonical(&other)];
    app.open_stats();
    let panel = app.session.stats.as_mut().unwrap();
    panel.blocks.reading(Part::Agenda, slow());
    let mut harness = tall(app);
    harness.run_steps(1);
    harness.get_by_label("Reading git history in 2 projects…");
}

/// A supervised folder git cannot read keeps its row and error while the
/// others still count.
#[test]
fn a_supervised_folder_git_cannot_read_still_leaves_the_others() {
    let (_dir, located) = project("agenda-broken");
    let bare = scratch_dir("agenda-bare");
    let bare_name = crate::files::file_name(&bare);
    let mut app = IboardApp::from_located(located);
    app.recents = vec![crate::shell::recents::canonical(&bare)];
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);

    let panel = harness.state().1.session.stats.as_ref().unwrap();
    let days = &panel.blocks.loaded(Part::Agenda).unwrap().agenda;
    assert_eq!(days.projects.len(), 1);
    assert_eq!(days.failed.len(), 1);
    assert_eq!(days.failed[0].0, bare_name);
    harness.get_by_label("3 commits · 1 project · 1 active day");
    harness.get_by_label(&bare_name);
    harness.get_by_label("Retry");
}

/// A day past its five commits says how many more, and in which project.
#[test]
fn the_agenda_names_the_project_of_a_days_other_commits() {
    let (dir, located) = project("agenda-more");
    for name in ["four", "five", "six", "seven"] {
        git(&dir, &["commit", "-q", "--allow-empty", "-m", name]);
    }
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let name = app.board.name();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);
    harness.get_by_label(&format!("2 more in {name}"));
}

/// A signal's detail lines up with its headline, past the icon's column, and
/// the panels head themselves in plain case.
#[test]
fn the_watch_indents_its_details_under_plain_headed_panels() {
    let (_dir, located) = project("stats-watch");
    let mut app = IboardApp::from_located(located);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);
    for title in ["Trends", "Refactor watch", "Biggest files"] {
        harness.get_by_label(title);
    }
    let title = harness.get_by_label("Refactor watch").rect();
    let headline = harness.get_by_label("Cycle time is steady").rect();
    let detail = harness.get_by_label("too few issues closed to tell").rect();
    assert_eq!(detail.left(), headline.left());
    assert!(headline.left() >= title.left() + 24.0, "{headline:?}");
}

/// The local day of `dir`'s newest commit, as the stats read it.
fn commit_day(dir: &Path) -> String {
    git(dir, &["log", "-1", "--format=%cI"])
        .trim()
        .chars()
        .take(10)
        .collect()
}

fn logged(
    session: &str,
    project: &Path,
    provider: &str,
    billing: Billing,
    cost: f64,
    tokens: u64,
    day: &str,
) -> Entry {
    let at = format!("{day}T10:30:00.000Z");
    Entry {
        session: session.to_string(),
        project: project.to_path_buf(),
        engine: "opencode".to_string(),
        provider: Some(provider.to_string()),
        model: None,
        billing,
        cost: Some(cost),
        tokens: Tokens {
            input: tokens,
            ..Tokens::default()
        },
        started: at.clone(),
        ended: at,
    }
}

fn write_log(path: &Path, entries: &[Entry]) {
    let text: String = entries
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap() + "\n")
        .collect();
    std::fs::write(path, text).unwrap();
}

/// The cost block totals the log's metered dollars and names its providers,
/// with a subscription's dollars kept out of the spend.
#[test]
fn the_cost_block_totals_the_log_and_names_each_provider() {
    let (dir, located) = project("stats-cost");
    let day = commit_day(&dir);
    let project = std::fs::canonicalize(&dir).unwrap();
    let log = dir.join("usage.jsonl");
    write_log(
        &log,
        &[
            logged(
                "ses_1",
                &project,
                "deepseek",
                Billing::Metered,
                0.25,
                1_000,
                &day,
            ),
            logged(
                "ses_2",
                &project,
                "github-copilot",
                Billing::Subscription,
                1.5,
                500,
                &day,
            ),
        ],
    );
    let mut app = IboardApp::from_located(located);
    app.usage_log_file = Some(log);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(2);

    harness.get_by_label("Cost");
    let total = harness
        .get_all_by_label("$0.25")
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .expect("the cost total");
    harness.get_by_label("spent");
    harness.get_by_label("· 1.5k tokens");
    harness.get_by_label("· $1.50 API-equivalent");
    harness.get_by_label_contains("DeepSeek · $0.25 paid");
    harness.get_by_label_contains("Copilot · 500 tokens");

    total.hover();
    harness.run_steps(4);
    assert!(
        harness
            .query_all_by_label_contains("API-equivalent · $1.50, never charged")
            .count()
            > 0
    );
}

/// With no run in the trends' days, the block says so instead of drawing a
/// zero bar.
#[test]
fn a_range_without_a_run_shows_the_empty_state() {
    let (dir, located) = project("stats-cost-empty");
    let mut app = IboardApp::from_located(located);
    app.usage_log_file = Some(dir.join("usage.jsonl"));
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    harness.run_steps(1);
    harness.get_by_label("No agent run in this range.");
    harness.get_by_label_contains("no run");
    assert!(harness.query_by_label("spent").is_none());
}

/// A failed cost read says why, keeps the other blocks, and Retry reads the
/// log again on its own.
#[test]
fn a_failed_cost_block_retries_alone() {
    let (dir, located) = project("stats-cost-failed");
    let day = commit_day(&dir);
    let project = std::fs::canonicalize(&dir).unwrap();
    let log = dir.join("usage.jsonl");
    write_log(
        &log,
        &[logged(
            "ses_1",
            &project,
            "deepseek",
            Billing::Metered,
            0.25,
            1_000,
            &day,
        )],
    );
    let mut app = IboardApp::from_located(located);
    app.usage_log_file = Some(log);
    app.open_stats();
    let mut harness = tall(app);
    assert!(settled(&mut harness));
    let panel = harness.state_mut().1.session.stats.as_mut().unwrap();
    panel
        .blocks
        .reading(Part::Cost, Job::done(Err("boom".to_string())));
    harness.run_steps(2);
    harness.get_by_label("Couldn't read the usage log: boom");
    harness.get_by_label("No refactor needed now");

    kit::press(&mut harness, "Retry");
    assert!(settled(&mut harness));
    harness.run_steps(1);
    harness.get_by_label_contains("DeepSeek · $0.25 paid");
}
