//! The composer's tests: what happens to a ticket just written.

use super::*;

/// Bug: a new ticket is renumbered to position 1 a moment after it shows,
/// so a quick click on it, or on its Add label action, held the old name:
/// "This issue no longer exists". Everything open on it follows the rename.
#[test]
fn a_ticket_clicked_while_it_is_renumbered_stays_open() {
    let dir = scratch_dir("compose-race");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::write(todo.join("001-old.md"), "# Old\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    app.write_composed("Fresh", "", false);
    let written = todo.join("fresh.md");
    assert_eq!(app.selected.as_deref(), Some(written.as_path()));
    assert!(
        app.pending_front.is_some(),
        "renumbered at the end of the frame"
    );

    app.open_issue(written.clone());
    app.toggle_label_picker(written.clone());
    draw(&themed_ctx(), &mut app, vec![]);

    let renamed = todo.join("001-fresh.md");
    assert!(renamed.is_file() && !written.exists());
    assert!(matches!(&app.screen, Screen::Detail(path) if *path == renamed));
    assert!(app.board.find(&renamed).is_some(), "the reader finds it");
    assert_eq!(
        app.label_picker
            .as_ref()
            .map(|picker| picker.path.as_path()),
        Some(renamed.as_path()),
        "the LabelPicker stays open on it"
    );
    assert_eq!(app.new_card.as_ref().map(|(path, _)| path), Some(&renamed));
}
