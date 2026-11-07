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
        "next column",
        "delete",
        "archive",
        "update",
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

/// Wherever you are, the status bar lists three keys at most. `Del` shows only
/// where it deletes, and gives way to `⌘U` in the Reader on Ready.
#[test]
fn the_status_bar_lists_three_keys_at_most() {
    let dir = scratch_dir("three-hints");
    let mut harness = board_of_every_column(&dir);
    assert_eq!(hints_shown(&harness), 3, "at rest");

    let cases = [
        ("todo/a.md", true),
        ("ready/b.md", false),
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
/// leaves the header for the `…` menu, with no key on Todo, where `⌘R` refines.
#[test]
fn refresh_shows_its_key_where_it_has_it() {
    let dir = scratch_dir("refresh-key");
    let mut harness = board_of_every_column(&dir);
    harness.get_by_label("Refresh");

    let todo = dir.join("issues/todo/a.md");
    let ready = dir.join("issues/ready/b.md");
    harness.state_mut().1.open_issue(todo.clone());
    harness.run_steps(1);
    assert!(harness.query_by_label("Refresh").is_none(), "in the menu");
    harness.get_by_label("More actions").click();
    harness.run_steps(2);
    harness.get_by_label("Refresh");

    let app = &harness.state().1;
    assert_eq!(app.refresh_key(&todo), "", "⌘R refines here");
    assert_eq!(app.refresh_key(&ready), "⌘R");
}
