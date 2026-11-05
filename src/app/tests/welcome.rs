//! The WelcomeScreen, from its window to the board its buttons open.

use egui_kittest::kittest::Queryable;

use super::super::welcome::nowhere;
use super::*;

thread_local! {
    static PICKED: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn picked() -> Vec<PathBuf> {
    PICKED.with(|picked| picked.borrow().clone())
}

/// The screen, with `folders` as what the picker answers.
fn welcome(folders: Vec<PathBuf>) -> Kit {
    PICKED.with(|picked| *picked.borrow_mut() = folders);
    let mut app = IboardApp::from_located(nowhere());
    app.folder_picker = picked;
    kittest(app)
}

#[test]
fn the_screen_stands_alone_with_its_two_actions() {
    let harness = welcome(Vec::new());
    assert!(harness.state().1.welcoming());
    harness.get_by_label("Open a folder…");
    harness.get_by_label("Create issues in a folder…");
    assert!(harness.query_by_label("Refresh").is_none());
}

#[test]
fn cancelling_the_picker_stays_on_the_screen() {
    let mut harness = welcome(Vec::new());
    press(&mut harness, "Open a folder…");
    press(&mut harness, "Create issues in a folder…");
    let app = &harness.state().1;
    assert!(app.welcoming());
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.recents.is_empty());
}

#[test]
fn opening_a_project_leaves_the_screen() {
    let project = scratch_dir("welcome-open");
    std::fs::create_dir_all(project.join("issues/todo")).unwrap();
    let mut harness = welcome(vec![project.to_path_buf()]);
    press(&mut harness, "Open a folder…");
    let app = &harness.state().1;
    assert!(!app.welcoming());
    assert_eq!(app.located.project_dir, project.to_path_buf());
    assert_eq!(app.ordered_projects(), [project.to_path_buf()]);
    assert!(app.scan_error.is_none(), "{:?}", app.scan_error);
}

#[test]
fn a_folder_without_issues_is_offered_them_from_the_error_strip() {
    let empty = scratch_dir("welcome-empty");
    let mut harness = welcome(vec![empty.to_path_buf()]);
    press(&mut harness, "Open a folder…");
    assert!(harness.state().1.welcoming());
    assert!(!empty.join("issues").exists());
    press(&mut harness, "Create issues/");
    assert!(!harness.state().1.welcoming());
    assert!(empty.join("issues/todo/.gitkeep").is_file());
}

#[test]
fn creating_issues_opens_the_new_board() {
    let empty = scratch_dir("welcome-create");
    let mut harness = welcome(vec![empty.to_path_buf()]);
    press(&mut harness, "Create issues in a folder…");
    let app = &harness.state().1;
    assert!(!app.welcoming());
    assert_eq!(app.located.project_dir, empty.to_path_buf());
    for column in discovery::INIT_COLUMNS {
        assert!(empty.join("issues").join(column).is_dir(), "{column}");
    }
}
