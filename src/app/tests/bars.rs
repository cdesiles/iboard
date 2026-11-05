//! The header and status bars: what they name, and their keys, out of sight
//! until ⌘ is held.

use super::*;
use egui_kittest::kittest::Queryable;

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

/// The board's header names the project, then its issues path.
#[test]
fn the_board_header_names_the_project_before_the_path() {
    let dir = scratch_dir("project-name");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let harness = board(&dir);
    let name = harness.state().1.board.name();
    let path = display_path(&harness.state().1.board.root);
    let name = harness.get_by_label(&name).rect();
    let path = harness.get_by_label(&path).rect();
    assert!(name.max.x <= path.min.x, "{name:?} then {path:?}");
}

fn stats_width(harness: &Board) -> f32 {
    harness.get_by_label("Stats").rect().width()
}

/// At rest the status bar lists no keys and Stats shows its label alone.
/// Half a second of ⌘ brings out `⌘I` and the bar's keys; letting go puts
/// them away.
#[test]
fn the_board_shows_its_keys_only_while_command_is_held() {
    let dir = scratch_dir("key-hints");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut harness = board(&dir);
    let at_rest = stats_width(&harness);
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
    assert!(stats_width(&harness) > at_rest, "⌘I slides out of Stats");

    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    harness.run_steps(1);
    assert!(harness.query_by_label("filter").is_none());
    assert_eq!(stats_width(&harness), at_rest);
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

/// How many of the status bar's possible hints show.
fn hints_shown(harness: &Board) -> usize {
    [
        "new", "filter", "refresh", "stats", "project", "open", "edit", "move", "delete",
        "archive", "update", "ask", "back", "find", "file", "hunk",
    ]
    .into_iter()
    .filter(|action| harness.query_by_label(action).is_some())
    .count()
}

/// Wherever you are, the status bar lists three keys at most, and `Del` only
/// where it deletes.
#[test]
fn the_status_bar_lists_three_keys_at_most() {
    let dir = scratch_dir("three-hints");
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
        assert!(hints_shown(&harness) <= 3, "{file} open");
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

/// Refresh shows `⌘R` as its key, except on a Todo issue, where it refines.
#[test]
fn refresh_shows_its_key_where_it_has_it() {
    let dir = scratch_dir("refresh-key");
    let mut harness = board_of_every_column(&dir);
    let width = |harness: &Board| harness.get_by_label("Refresh").rect().width();
    let on_board = width(&harness);

    harness
        .state_mut()
        .1
        .open_issue(dir.join("issues/todo/a.md"));
    harness.run_steps(1);
    assert!(width(&harness) < on_board, "⌘R refines here");
}
