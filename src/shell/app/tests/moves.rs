//! Moving, reordering, deleting and archiving issues without a drag, and
//! undoing a delete.

use super::*;

/// `M` opens Move to… in the Reader and a column's number sends the issue
/// there.
#[test]
fn m_then_a_number_moves_the_open_issue() {
    let (dir, mut app) = project(
        "move-keys",
        &[("doing/task.md", "# Task\n"), ("ready/", ""), ("done/", "")],
    );
    let file = dir.join("issues/doing/task.md");
    app.open_issue(file.clone());
    let ctx = themed_ctx();
    let none = egui::Modifiers::NONE;
    frame(&ctx, &mut app, Vec::new());
    frame(&ctx, &mut app, vec![key(Key::M, none)]);
    frame(&ctx, &mut app, vec![key(Key::Num3, none)]);
    let moved = dir.join("issues/done/task.md");
    assert!(
        moved.exists() && !file.exists(),
        "3 is Done, after Todo and Ready"
    );
    assert!(matches!(&app.session.screen, Screen::Detail(open, _) if *open == moved));
}

/// A project without a `done/` folder can still send an issue to Done:
/// Move to… offers it in its place and creates the folder.
#[test]
fn move_to_creates_a_standard_column_the_project_lacks() {
    let (dir, mut app) = project(
        "move-missing",
        &[
            ("ready/task.md", "# Task\n"),
            ("doing/", ""),
            ("blocked/", ""),
        ],
    );
    let file = dir.join("issues/ready/task.md");
    app.open_issue(file.clone());
    let names: Vec<String> = app
        .move_targets(&file)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["Todo", "Doing", "Blocked", "Done"]);
    let ctx = themed_ctx();
    let none = egui::Modifiers::NONE;
    frame(&ctx, &mut app, Vec::new());
    frame(&ctx, &mut app, vec![key(Key::M, none)]);
    frame(&ctx, &mut app, vec![key(Key::Num4, none)]);
    let moved = dir.join("issues/done/task.md");
    assert!(moved.exists() && !file.exists(), "4 is Done, created");
}

#[test]
fn moving_an_issue_reparents_the_file_and_refreshes_the_board() {
    let (dir, mut app) = project("move", &[("todo/01-demo.md", "# Demo\n"), ("doing/", "")]);
    let doing = dir.join("issues/doing");
    let file = dir.join("issues/todo/01-demo.md");
    assert!(app.board.find(&file).is_some());

    app.move_issue(&file, &doing);

    assert!(!file.exists(), "source should be gone");
    let moved = doing.join("01-demo.md");
    assert!(moved.exists(), "file should live in the target column");
    assert!(app.error.is_none(), "a clean move reports no error");
    assert_eq!(
        app.board.find(&moved).map(|issue| issue.title.as_str()),
        Some("Demo"),
        "the board adopts the moved issue"
    );
}

#[test]
fn an_issue_is_numbered_in_ready_and_unnumbered_when_taken() {
    let (dir, mut app) = project(
        "ready",
        &[
            ("todo/make-colour-green.md", "# Green\n"),
            ("ready/", ""),
            ("doing/", ""),
        ],
    );
    let ready = dir.join("issues/ready");
    let doing = dir.join("issues/doing");
    let file = dir.join("issues/todo/make-colour-green.md");
    app.session.selected = Some(file.clone());

    app.move_issue(&file, &ready);
    let queued = ready.join("001-make-colour-green.md");
    assert!(queued.exists(), "entering Ready adds the priority number");
    assert_eq!(app.session.selected.as_deref(), Some(queued.as_path()));

    app.move_issue(&queued, &doing);
    assert!(
        doing.join("make-colour-green.md").exists(),
        "leaving Ready drops it"
    );
    assert!(app.error.is_none());
}

#[test]
fn moving_never_overwrites_an_existing_file() {
    let (dir, mut app) = project(
        "clobber",
        &[
            ("todo/01-demo.md", "# Demo\n"),
            ("doing/01-demo.md", "# Other\n"),
        ],
    );
    let doing = dir.join("issues/doing");
    let file = dir.join("issues/todo/01-demo.md");
    let existing = doing.join("01-demo.md");

    app.move_issue(&file, &doing);

    assert!(file.exists(), "source must survive a refused move");
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "# Other\n");
    assert!(app.error.is_some(), "the refusal is surfaced to the user");
    app.refresh();
    assert!(app.error.is_some(), "and the next scan does not wipe it");
}

#[test]
fn dropping_a_todo_card_at_a_position_renumbers_the_column() {
    let (dir, mut app) = project(
        "reorder-todo",
        &[
            ("todo/001-hello.md", "# Hello\n"),
            ("todo/002-byebye.md", "# Bye bye\n"),
        ],
    );
    let todo = dir.join("issues/todo");
    let hello = todo.join("001-hello.md");
    let byebye = todo.join("002-byebye.md");

    // Drop Bye bye in front of Hello.
    app.place_issue(&byebye, &todo, Some(hello.clone()));

    assert!(
        todo.join("001-byebye.md").exists(),
        "the dropped card becomes 001"
    );
    assert!(
        todo.join("002-hello.md").exists(),
        "the displaced card becomes 002"
    );
    assert!(!byebye.exists() && !hello.exists(), "old names are gone");
    assert!(app.error.is_none(), "{:?}", app.error);

    let names: Vec<String> = app.board.columns[0]
        .issues
        .iter()
        .map(|issue| issue.file_name())
        .collect();
    assert_eq!(names, ["001-byebye.md", "002-hello.md"]);
}

#[test]
fn moving_a_todo_card_into_ready_at_a_position_renumbers_it() {
    let (dir, mut app) = project(
        "reorder-ready",
        &[
            ("todo/later.md", "# Later\n"),
            ("ready/001-first.md", "# First\n"),
        ],
    );
    let ready = dir.join("issues/ready");
    let moved = dir.join("issues/todo/later.md");
    let first = ready.join("001-first.md");

    app.place_issue(&moved, &ready, Some(first.clone()));

    assert!(ready.join("001-later.md").exists(), "it takes priority 1");
    assert!(ready.join("002-first.md").exists(), "the other moves down");
    assert!(app.error.is_none(), "{:?}", app.error);
}

#[test]
fn deleting_an_issue_asks_first_then_removes_the_file() {
    let (dir, mut app) = project("delete", &[("todo/01-demo.md", "# Demo\n")]);
    let file = dir.join("issues/todo/01-demo.md");
    app.session.selected = Some(file.clone());
    app.open_issue(file.clone());

    app.request_delete(file.clone());
    assert!(
        app.session.confirm.is_some(),
        "the delete waits for confirmation"
    );
    assert!(file.exists(), "nothing is removed until confirmed");

    app.delete_issue(&file);
    assert!(!file.exists(), "the file is gone");
    assert!(app.session.confirm.is_none());
    assert!(app.session.selected.is_none());
    assert!(
        matches!(app.session.screen, Screen::Board),
        "deleting the open issue returns to the board"
    );
}

#[test]
fn with_asking_off_delete_is_immediate_and_command_z_restores() {
    let (dir, mut app) = project(
        "delete-no-ask",
        &[("doing/sidebar.md", "# Sidebar\n\nCollapse it.\n")],
    );
    let file = dir.join("issues/doing/sidebar.md");
    assert!(app.prefs.confirm_delete, "asking is on by default");

    app.request_delete(file.clone());
    assert!(app.session.confirm.is_some(), "on: the dialog opens");
    assert!(file.exists());
    app.session.confirm = None;

    app.prefs.confirm_delete = false;
    app.request_delete(file.clone());
    assert!(app.session.confirm.is_none(), "off: no dialog");
    assert!(!file.exists(), "off: deleted at once");
    assert!(
        dir.join("Trash/files/sidebar.md").is_file(),
        "the test's own Trash"
    );
    assert!(app.session.undo.is_some());

    app.undo_last();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Sidebar\n\nCollapse it.\n",
        "⌘Z puts it back as it was"
    );
    assert!(app.board.find(&file).is_some(), "and on the board");
    assert_eq!(app.session.selected.as_deref(), Some(file.as_path()));
    assert!(app.session.undo.is_none(), "once");
}

/// `⌘Z` waits for as long as nothing else happens; a key or a click ends it.
#[test]
fn undo_lasts_until_the_next_action() {
    let (dir, mut app) = project(
        "undo-until-next-action",
        &[("todo/sidebar.md", "# Sidebar\n")],
    );
    let file = dir.join("issues/todo/sidebar.md");
    app.prefs.confirm_delete = false;
    let ctx = themed_ctx();
    let trash = |app: &mut IboardApp| {
        std::fs::write(&file, "# Sidebar\n").unwrap();
        app.refresh();
        app.request_delete(file.clone());
    };

    trash(&mut app);
    frame(&ctx, &mut app, vec![moved(egui::pos2(5.0, 5.0))]);
    assert!(
        app.session.undo.is_some(),
        "the pointer moving is no action"
    );
    frame(
        &ctx,
        &mut app,
        vec![key(Key::ArrowDown, egui::Modifiers::NONE)],
    );
    assert!(app.session.undo.is_none(), "a key press is");

    trash(&mut app);
    frame(&ctx, &mut app, vec![button(egui::pos2(5.0, 5.0), true)]);
    assert!(app.session.undo.is_none(), "a click too");

    trash(&mut app);
    frame(&ctx, &mut app, cmd(Key::Z));
    assert!(file.exists(), "⌘Z still reaches it");
}

/// The delete icon is its own click target: using it must not open the
/// issue underneath it.
#[test]
fn the_card_delete_icon_asks_to_delete_without_opening() {
    let (dir, mut app) = project("card-delete", &[("todo/01-demo.md", "# Demo\n")]);
    let file = dir.join("issues/todo/01-demo.md");

    let ctx = themed_ctx();

    draw(&ctx, &mut app, vec![]);
    let card = ctx
        .read_response(egui::Id::unique(("issue_card", &file)))
        .unwrap()
        .rect
        .center();
    draw(&ctx, &mut app, vec![egui::Event::PointerMoved(card)]);
    draw(&ctx, &mut app, vec![egui::Event::PointerMoved(card)]);
    let icon = ctx
        .read_response(egui::Id::unique(("card_delete", &file)))
        .expect("the hovered card draws a delete icon")
        .rect
        .center();
    draw(&ctx, &mut app, vec![egui::Event::PointerMoved(icon)]);
    draw(
        &ctx,
        &mut app,
        vec![
            egui::Event::PointerMoved(icon),
            egui::Event::PointerButton {
                pos: icon,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    draw(
        &ctx,
        &mut app,
        vec![egui::Event::PointerButton {
            pos: icon,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );

    assert!(app.session.confirm.is_some(), "the icon asks to delete");
    assert!(
        matches!(app.session.screen, Screen::Board),
        "and does not open the issue"
    );
}

/// Done issues are archived, never deleted: no trash icon, and `Del` on the
/// selected card does nothing.
#[test]
fn done_cards_cannot_be_deleted() {
    let (dir, mut app) = project("done-no-delete", &[("done/01-demo.md", "# Demo\n")]);
    let file = dir.join("issues/done/01-demo.md");
    app.session.selected = Some(file.clone());

    let ctx = themed_ctx();
    frame(&ctx, &mut app, vec![]);
    assert!(
        ctx.read_response(egui::Id::unique(("card_archive", &file)))
            .is_some()
    );
    assert!(
        ctx.read_response(egui::Id::unique(("card_delete", &file)))
            .is_none()
    );

    frame(&ctx, &mut app, vec![key(Key::Delete, Default::default())]);
    assert!(app.session.confirm.is_none(), "nothing to confirm");
    assert!(file.exists());
}

/// Done issues keep the labels they have: no Add label, and `L` opens nothing.
#[test]
fn done_cards_cannot_be_labelled() {
    let (dir, mut app) = project("done-no-label", &[("done/01-demo.md", "# Demo\n")]);
    let file = dir.join("issues/done/01-demo.md");
    app.session.selected = Some(file.clone());

    let ctx = themed_ctx();
    frame(&ctx, &mut app, vec![]);
    assert!(
        ctx.read_response(egui::Id::unique(("card_add_label", &file)))
            .is_none()
    );
    frame(&ctx, &mut app, vec![key(Key::L, Default::default())]);
    assert!(app.session.label_picker.is_none());
}

#[test]
fn archiving_a_done_issue_hides_it_in_the_dot_archive() {
    let (dir, mut app) = project("archive", &[("done/01-demo.md", "# Demo\n")]);
    let file = dir.join("issues/done/01-demo.md");
    assert!(app.board.find(&file).is_some());

    app.archive_issue(&file);
    let archived = dir.join("issues/.archive/01-demo.md");
    assert!(archived.exists(), "the issue is kept in .archive");
    assert!(!file.exists(), "and is gone from Done");
    assert!(app.board.find(&file).is_none(), "the board drops it");
    assert!(
        app.error.is_none(),
        "a clean archive reports no error: {:?}",
        app.error
    );

    // A second issue with the same name does not overwrite the first.
    std::fs::write(&file, "# Again\n").unwrap();
    app.refresh();
    app.archive_issue(&file);
    assert!(dir.join("issues/.archive/01-demo-2.md").exists());
}

#[test]
fn a_archives_the_selected_done_card() {
    let (dir, mut app) = project("archive-key", &[("done/01-demo.md", "# Demo\n")]);
    let file = dir.join("issues/done/01-demo.md");
    app.session.selected = Some(file.clone());

    let ctx = themed_ctx();
    let input = egui::RawInput {
        events: vec![egui::Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |ui| {
        app.frame_logic(ui.ctx());
    });

    assert!(dir.join("issues/.archive/01-demo.md").exists());
}

#[test]
fn archive_all_clears_done() {
    let (dir, mut app) = project(
        "archive-all",
        &[
            ("done/a.md", "# A\n"),
            ("done/b.md", "# B\n"),
            ("todo/", ""),
        ],
    );
    let a = dir.join("issues/done/a.md");
    let b = dir.join("issues/done/b.md");
    let before = app.board_generation;

    app.archive_all(&[a.clone(), b.clone()]);
    assert_eq!(app.board_generation, before + 1, "the board is read once");
    assert!(!a.exists() && !b.exists());
    assert!(dir.join("issues/.archive/a.md").exists());
    assert_eq!(
        app.session.notice.as_ref().map(|(text, _)| text.as_str()),
        Some("Archived 2 issues")
    );
}

/// Done's count gives way to "Archive all" while its header is hovered.
#[test]
fn hovering_the_done_header_offers_archive_all() {
    let (dir, mut app) = project("archive-all-head", &[("done/a.md", "# A\n"), ("todo/", "")]);
    let a = dir.join("issues/done/a.md");
    let index = app
        .board
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("done"))
        .unwrap();
    let archive_all = egui::Id::unique(("archive_all", index));

    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    assert!(ctx.read_response(archive_all).is_none(), "hidden at rest");
    let column = ctx
        .read_response(egui::Id::unique(("column_drop", index)))
        .unwrap()
        .rect;
    let head = egui::pos2(column.center().x, column.min.y + theme::CONTROL_H / 2.0);
    draw(&ctx, &mut app, vec![moved(head)]);
    let at = ctx
        .read_response(archive_all)
        .expect("shown on hover")
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(!a.exists() && dir.join("issues/.archive/a.md").exists());
}

/// Opens everything that can hold `file`, each slot once.
fn hold_everywhere(app: &mut IboardApp, file: &Path) {
    app.prefs.idesign = true;
    app.prefs.agent = fake_agent("printf '<html></html>' # {prompt}");
    app.start_enrichment(file);
    app.toggle_ask(file);
    app.toggle_agent_log(file);
    app.open_commits(file);
    let session = &mut app.session;
    session.picked = vec![file.to_path_buf()];
    session.confirm = Some(Confirm::Delete(vec![file.to_path_buf()]));
    session.new_card = Some((file.to_path_buf(), Instant::now()));
    session.pending_front = Some((PathBuf::new(), file.to_path_buf()));
    session.editor = Some(EditorState::new(file.to_path_buf(), String::new()));
    session.label_picker = Some(LabelPicker::new(file.to_path_buf()));
    session.refine = Some(super::super::refine::Refinement::new(
        file.to_path_buf(),
        String::new(),
    ));
    session.notes = Some(super::super::comments::Notes {
        path: file.to_path_buf(),
        comments: Vec::new(),
        draft: None,
    });
}

#[test]
fn a_rename_follows_the_issue_everywhere_it_is_held() {
    let (_dir, file, mut app) = super::review::open_issue_with_review("rename-all");
    hold_everywhere(&mut app, &file);
    assert_eq!(app.session.issue_paths_mut().count(), 15);
    let new = file.with_file_name("02-demo.md");

    app.apply_renames(&[(file, new.clone())]);

    assert!(app.session.issue_paths_mut().all(|path| *path == new));
}

/// A deleted issue closes everything open on it but the enrichment, an
/// agent at work.
#[test]
fn a_delete_lets_go_of_the_issue_everywhere_it_is_held() {
    let (_dir, file, mut app) = super::review::open_issue_with_review("delete-all");
    hold_everywhere(&mut app, &file);

    app.delete_issue(&file);

    assert_eq!(app.session.issue_paths_mut().count(), 1);
    assert!(app.session.enrichment.is_some());
    assert!(matches!(app.session.screen, Screen::Board));
}
