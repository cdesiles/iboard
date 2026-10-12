//! Dragging cards: between columns, within one, and onto the DropZones.

use super::*;

/// Simulates a real pointer drag from a `todo/` card onto the `doing/`
/// column, so the drag-source/drop-zone wiring is exercised, not just the
/// move helper.
#[test]
fn dragging_a_card_onto_another_column_moves_the_file() {
    let dir = scratch_dir("drag");
    let todo = dir.join("issues/todo");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(&doing).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    // An existing card in the target column, so the drop lands on it.
    let other = doing.join("01-other.md");
    std::fs::write(&other, "# Other\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    let ctx = themed_ctx();

    draw(&ctx, &mut app, vec![]);

    let card_id = egui::Id::new(("issue_card", &file));
    let start = ctx
        .read_response(card_id)
        .expect("the card is a drag source")
        .rect
        .center();
    // Aim at the card already in the target column, to prove that a card
    // under the pointer does not swallow the column drop.
    let target_card_id = egui::Id::new(("issue_card", &other));
    let target = ctx
        .read_response(target_card_id)
        .expect("the target column has a card")
        .rect
        .center();

    // Hover, press, then move well past the drag threshold.
    draw(&ctx, &mut app, vec![moved(start)]);
    draw(&ctx, &mut app, vec![moved(start), button(start, true)]);
    draw(&ctx, &mut app, vec![moved(start + egui::vec2(30.0, 0.0))]);
    draw(&ctx, &mut app, vec![moved(target)]);
    draw(&ctx, &mut app, vec![moved(target)]);
    // Release over the target column.
    draw(&ctx, &mut app, vec![moved(target), button(target, false)]);

    let moved_file = doing.join("01-demo.md");
    assert!(
        moved_file.exists(),
        "dragging the card should move the file into the target column"
    );
    assert!(!file.exists(), "the original file should be gone");
}

/// Drives a real pointer drag of `file`'s card onto the DropZones dock
/// zone `zone` (or, with `None`, cancels it with `esc` instead), and
/// returns the app afterwards.
fn drag_card_to_zone(app: &mut IboardApp, file: &Path, zone: Option<&str>) {
    let ctx = themed_ctx();

    draw(&ctx, app, vec![]);
    let start = ctx
        .read_response(egui::Id::new(("issue_card", file)))
        .expect("the card is a drag source")
        .rect
        .center();
    draw(&ctx, app, vec![moved(start)]);
    draw(&ctx, app, vec![moved(start), button(start, true)]);
    let lifted = start + egui::vec2(30.0, 30.0);
    // Let the dock slide all the way in.
    for _ in 0..20 {
        draw(&ctx, app, vec![moved(lifted)]);
    }
    assert!(
        egui::DragAndDrop::has_payload_of_type::<IssueDrag>(&ctx),
        "the card is being dragged"
    );
    let Some(zone) = zone else {
        draw(
            &ctx,
            app,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(
            !egui::DragAndDrop::has_any_payload(&ctx),
            "esc cancels the drag"
        );
        draw(&ctx, app, vec![moved(lifted), button(lifted, false)]);
        return;
    };
    let target = ctx
        .read_response(egui::Id::new(zone))
        .expect("the dock shows while dragging")
        .rect
        .center();
    draw(&ctx, app, vec![moved(target)]);
    draw(&ctx, app, vec![moved(target)]);
    draw(&ctx, app, vec![moved(target), button(target, false)]);
}

#[test]
fn dropping_a_done_card_on_archive_archives_it() {
    let dir = scratch_dir("dock-archive");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    let file = done.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    drag_card_to_zone(&mut app, &file, Some("drop_zone_archive"));

    assert!(!file.exists(), "the card left done/");
    assert!(dir.join("issues/.archive/01-demo.md").exists());
    assert_eq!(
        app.session.notice.as_ref().map(|(text, _)| text.as_str()),
        Some("Archived 01-demo.md")
    );
}

/// A card outside Done only gets the Delete zone: Archive is a Done-only
/// exit, so the dock does not show it at all.
#[test]
fn a_card_outside_done_only_gets_delete() {
    let dir = scratch_dir("dock-archive-off");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(dir.join("issues/done")).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    let ctx = themed_ctx();

    draw(&ctx, &mut app, vec![]);
    let start = ctx
        .read_response(egui::Id::new(("issue_card", &file)))
        .expect("the card is a drag source")
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(start)]);
    draw(&ctx, &mut app, vec![moved(start), button(start, true)]);
    let lifted = start + egui::vec2(30.0, 30.0);
    for _ in 0..20 {
        draw(&ctx, &mut app, vec![moved(lifted)]);
    }
    assert!(
        ctx.read_response(egui::Id::new("drop_zone_delete"))
            .is_some(),
        "Delete is always offered"
    );
    assert!(
        ctx.read_response(egui::Id::new("drop_zone_archive"))
            .is_none(),
        "Archive is only shown for Done cards"
    );
    // Cancel the drag: the card stays where it was.
    draw(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    draw(&ctx, &mut app, vec![moved(lifted), button(lifted, false)]);

    assert!(file.exists(), "the card goes back to its place");
    assert!(!dir.join("issues/.archive").exists());
}

#[test]
fn dropping_a_card_on_delete_asks_first() {
    let dir = scratch_dir("dock-delete");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    drag_card_to_zone(&mut app, &file, Some("drop_zone_delete"));

    assert!(
        matches!(&app.session.confirm, Some(Confirm::Delete(path)) if *path == file),
        "the drop opens the confirmation"
    );
    assert!(file.exists(), "nothing is removed until confirmed");
    app.delete_issue(&file);
    assert!(!file.exists());
    assert_eq!(
        app.session.notice.as_ref().map(|(text, _)| text.as_str()),
        Some("Deleted 01-demo.md")
    );
}

#[test]
fn esc_cancels_a_drag_and_leaves_the_card() {
    let dir = scratch_dir("dock-cancel");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    drag_card_to_zone(&mut app, &file, None);

    assert!(file.exists());
    assert!(app.session.confirm.is_none());
}

#[test]
fn a_archives_the_selected_done_card() {
    let dir = scratch_dir("archive-key");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    let file = done.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
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
fn dropping_a_todo_card_at_a_position_renumbers_the_column() {
    let dir = scratch_dir("reorder-todo");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let hello = todo.join("001-hello.md");
    let byebye = todo.join("002-byebye.md");
    std::fs::write(&hello, "# Hello\n").unwrap();
    std::fs::write(&byebye, "# Bye bye\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

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
    let dir = scratch_dir("reorder-ready");
    let todo = dir.join("issues/todo");
    let ready = dir.join("issues/ready");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(&ready).unwrap();
    let moved = todo.join("later.md");
    let first = ready.join("001-first.md");
    std::fs::write(&moved, "# Later\n").unwrap();
    std::fs::write(&first, "# First\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    app.place_issue(&moved, &ready, Some(first.clone()));

    assert!(ready.join("001-later.md").exists(), "it takes priority 1");
    assert!(ready.join("002-first.md").exists(), "the other moves down");
    assert!(app.error.is_none(), "{:?}", app.error);
}

/// A real pointer drag of the last Todo card onto the top of the column
/// renumbers the files, exercising the drop-anchor wiring end to end.
#[test]
fn dragging_a_card_within_a_column_reorders_it() {
    let dir = scratch_dir("drag-reorder");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let first = todo.join("001-first.md");
    let second = todo.join("002-second.md");
    std::fs::write(&first, "# First\n").unwrap();
    std::fs::write(&second, "# Second\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    let ctx = themed_ctx();

    draw(&ctx, &mut app, vec![]);
    let start = ctx
        .read_response(egui::Id::new(("issue_card", &second)))
        .expect("the card is a drag source")
        .rect
        .center();
    // Just above the first card's middle: insert before it.
    let target = ctx
        .read_response(egui::Id::new(("issue_card", &first)))
        .expect("the target card is drawn")
        .rect
        .center()
        - egui::vec2(0.0, 6.0);

    draw(&ctx, &mut app, vec![moved(start)]);
    draw(&ctx, &mut app, vec![moved(start), button(start, true)]);
    draw(&ctx, &mut app, vec![moved(start + egui::vec2(0.0, 30.0))]);
    draw(&ctx, &mut app, vec![moved(target)]);
    draw(&ctx, &mut app, vec![moved(target)]);
    draw(&ctx, &mut app, vec![moved(target), button(target, false)]);

    assert!(
        todo.join("001-second.md").exists() && todo.join("002-first.md").exists(),
        "dropping the second card on top renumbers the column"
    );
    assert!(!first.exists() && !second.exists(), "old names are gone");
}

/// The delete icon is its own click target: using it must not open the
/// issue underneath it.
#[test]
fn the_card_delete_icon_asks_to_delete_without_opening() {
    let dir = scratch_dir("card-delete");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    let ctx = themed_ctx();

    draw(&ctx, &mut app, vec![]);
    let icon = ctx
        .read_response(egui::Id::new(("card_delete", &file)))
        .expect("the card draws a delete icon")
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
