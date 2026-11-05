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
