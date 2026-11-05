//! The editor's tests: renaming through the title, leaving, conflicts, saving,
//! and adding a section from the reader.

use super::*;

#[test]
fn renaming_an_issue_via_the_editor_moves_the_file() {
    let dir = scratch_dir("rename");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    app.start_editing(&file);
    app.session.editor.as_mut().unwrap().name = "01-renamed".to_string();
    app.flush_autosave(false);

    let renamed = todo.join("01-renamed.md");
    assert!(renamed.exists(), "the issue is renamed on disk");
    assert!(!file.exists(), "the old file name is gone");
    assert!(app.error.is_none(), "a clean rename reports no error");
    assert!(
        matches!(&app.session.screen, Screen::Detail(path, _) if path == &renamed),
        "the detail view follows the renamed file"
    );
    assert_eq!(app.session.editor.as_ref().unwrap().path, renamed);
    assert_eq!(
        app.board.find(&renamed).map(|issue| issue.title.as_str()),
        Some("Demo"),
        "the board adopts the renamed issue"
    );
}

/// Bug: a name edited in the editor left the review panel on the old file.
#[test]
fn the_review_panel_follows_a_rename_in_the_editor() {
    let (_dir, file, mut app) = open_issue_with_review("rename-review");

    app.start_editing(&file);
    app.session.editor.as_mut().unwrap().name = "01-renamed".to_string();
    app.flush_autosave(false);

    let renamed = file.with_file_name("01-renamed.md");
    assert!(renamed.exists(), "the issue is renamed on disk");
    assert_eq!(
        app.session.review.as_ref().map(|review| &review.path),
        Some(&renamed),
        "the review panel follows the renamed file"
    );
    assert_eq!(app.session.selected, Some(renamed), "so does the selection");
}

#[test]
fn renaming_onto_an_existing_issue_is_refused() {
    let dir = scratch_dir("rename-clash");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let existing = todo.join("02-other.md");
    std::fs::write(&existing, "# Other\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    app.start_editing(&file);
    app.session.editor.as_mut().unwrap().name = "02-other.md".to_string();
    app.flush_autosave(false);

    assert!(file.exists(), "a refused rename leaves the file in place");
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "# Other\n");
    assert!(
        app.session.editor.as_ref().unwrap().error.is_some(),
        "the clash is surfaced to the user"
    );
}

/// An app editing `todo/01-demo.md` with an unsaved change.
fn editing_with_unsaved_change(tag: &str, auto: bool) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
    app.prefs.auto = auto;
    app.open_issue(file.clone());
    app.start_editing(&file);
    app.session.editor.as_mut().unwrap().text = "# Demo\n\nmore\n".to_string();
    (dir, file, app)
}

#[test]
fn leaving_with_auto_on_saves() {
    let (_dir, file, mut app) = editing_with_unsaved_change("leave-auto", true);

    app.show_preview();

    assert_eq!(app.session.reading(), Some(file.as_path()));
    assert!(
        app.session.leave_prompt.is_none(),
        "no question when the save worked"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n\nmore\n");
}

#[test]
fn leaving_with_auto_off_asks_first() {
    let (_dir, file, mut app) = editing_with_unsaved_change("leave-ask", false);

    app.back_to_board();
    assert_eq!(
        app.session.leave_prompt,
        Some(Leave::Board),
        "the editor asks"
    );
    assert_eq!(app.session.editing(), Some(file.as_path()), "and stays put");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n");

    app.answer_leave(LeaveChoice::KeepEditing);
    assert!(app.session.leave_prompt.is_none());
    assert_eq!(app.session.editing(), Some(file.as_path()));

    app.back_to_board();
    app.answer_leave(LeaveChoice::Save);
    assert!(
        matches!(app.session.screen, Screen::Board),
        "saved, then left"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n\nmore\n");
}

#[test]
fn discarding_on_leave_keeps_the_file() {
    let (_dir, file, mut app) = editing_with_unsaved_change("leave-discard", false);

    app.open_composer();
    assert_eq!(app.session.leave_prompt, Some(Leave::Composer));
    app.answer_leave(LeaveChoice::Discard);

    assert!(matches!(app.session.screen, Screen::Board));
    assert!(
        app.session.composer.is_some(),
        "the composer opens after leaving"
    );
    assert!(!app.has_unsaved_edits());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n");
}

/// Bug: back on the board, `E` on the card the editor still held only set
/// the Edit mode, and the board stayed on screen.
#[test]
fn e_on_the_board_edits_the_card_the_editor_still_holds() {
    let dir = scratch_dir("edit-again");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let ctx = themed_ctx();
    let press = |app: &mut IboardApp, pressed: Key| {
        frame(&ctx, app, vec![key(pressed, egui::Modifiers::NONE)]);
    };

    app.open_issue(file.clone());
    press(&mut app, Key::E);
    press(&mut app, Key::Escape);
    press(&mut app, Key::Escape);
    assert!(matches!(app.session.screen, Screen::Board));
    assert!(app.session.editor.is_some(), "the editor keeps its buffer");

    press(&mut app, Key::E);
    assert_eq!(
        app.session.editing(),
        Some(file.as_path()),
        "the editor opens on it"
    );
}

#[test]
fn a_conflict_asks_even_with_auto_on() {
    let (_dir, file, mut app) = editing_with_unsaved_change("leave-conflict", true);
    std::fs::write(&file, "# Changed elsewhere\n").unwrap();
    app.refresh();

    app.show_preview();

    assert_eq!(
        app.session.leave_prompt,
        Some(Leave::Reader),
        "autosave cannot resolve it"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Changed elsewhere\n"
    );
}

/// Bug: `⌘S` in the editor did not save.
#[test]
fn command_s_saves_the_editor() {
    let dir = scratch_dir("command-s");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.prefs.auto = false;
    app.start_editing(&file);
    app.session.editor.as_mut().unwrap().text = "# Demo\n\nMore.\n".into();

    let ctx = themed_ctx();
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    let s = egui::Event::Key {
        key: Key::S,
        physical_key: Some(Key::S),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    };
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::ModifiersChanged(egui::Modifiers::COMMAND), s],
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n\nMore.\n");
    assert_eq!(
        app.session.reading(),
        Some(file.as_path()),
        "like the Save ⌘S button, it shows the saved issue"
    );
}

#[test]
fn dirty_tracks_baseline() {
    let mut editor = EditorState::new(PathBuf::from("t.md"), "hello".to_string());
    assert!(!editor.is_dirty());
    editor.text.push_str(" world");
    assert!(editor.is_dirty());
    editor.text = "hello".to_string();
    assert!(!editor.is_dirty());
}

/// The reader sheet draws ghost blocks for a bare issue and chips for a
/// partly written one; adding a section opens the editor with the heading
/// appended, unsaved, and leaves the file alone.
#[test]
fn reader_sheet_offers_missing_sections() {
    let dir = scratch_dir("sheet");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let bare = todo.join("bare.md");
    let partial = todo.join("partial.md");
    std::fs::write(&bare, "# Bare\n\nJust an idea.\n").unwrap();
    std::fs::write(
        &partial,
        "# Partial\n\n## Context\n\nWhy.\n\n- [x] a\n- [ ] b\n",
    )
    .unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
    let ctx = themed_ctx();

    app.open_issue(bare.clone());
    draw(&ctx, &mut app, vec![]); // ghost blocks
    app.open_issue(partial.clone());
    draw(&ctx, &mut app, vec![]); // "Add a section" chips and checklist progress

    app.add_section(&partial, "Acceptance criteria");
    assert_eq!(app.session.editing(), Some(partial.as_path()));
    assert!(
        app.session.focus_editor,
        "the caret goes under the new heading"
    );
    let editor = app.session.editor.as_ref().unwrap();
    assert!(
        editor
            .text
            .ends_with("- [ ] b\n\n## Acceptance criteria\n\n- [ ] ")
    );
    assert!(editor.is_dirty());
    assert!(
        !std::fs::read_to_string(&partial)
            .unwrap()
            .contains("Acceptance")
    );
    draw(&ctx, &mut app, vec![]);
}
