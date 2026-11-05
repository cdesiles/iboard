//! Dragging cards: between columns, within one, and onto the DropZones.

use super::*;
use crate::app::dock::IssueDrag;

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

/// The dock rises under the dragged card's column only, so a drop meant for
/// a neighbour's bottom never lands in the Trash.
#[test]
fn the_dock_stays_under_the_dragged_cards_column() {
    let dir = scratch_dir("dock-column");
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
    let column = |index: usize| {
        ctx.read_response(egui::Id::new(("column_drop", index)))
            .expect("the column is a drop target")
            .rect
    };
    let band = ctx
        .read_response(egui::Id::new("drop_dock_band"))
        .expect("the dock shows while dragging")
        .rect;
    assert_eq!(band.x_range(), column(0).x_range(), "the dock spans Todo");
    assert!(band.max.x < column(1).min.x, "Done stays a drop target");
    draw(&ctx, &mut app, vec![moved(lifted), button(lifted, false)]);
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
        matches!(&app.session.confirm, Some(Confirm::Delete(paths)) if *paths == [file.clone()]),
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

/// The lifted copy under the pointer is tilted, as the design system draws
/// it: its top edge rises to the right.
#[test]
fn the_dragged_card_is_tilted() {
    let dir = scratch_dir("drag-tilt");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let start = ctx
        .read_response(egui::Id::new(("issue_card", &file)))
        .unwrap()
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(start)]);
    draw(&ctx, &mut app, vec![moved(start), button(start, true)]);
    for _ in 0..5 {
        draw(&ctx, &mut app, vec![moved(start + egui::vec2(40.0, 40.0))]);
    }
    let input = egui::RawInput {
        screen_rect: Some(window()),
        events: vec![moved(start + egui::vec2(60.0, 60.0))],
        ..Default::default()
    };
    let ghost = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new(("issue_ghost", &file)));
    let mut vertices: Vec<egui::Pos2> = Vec::new();
    let mut flat: Vec<String> = Vec::new();
    let mut output = ctx.run_ui(input, |ui| {
        app.draw(ui);
        ui.ctx().graphics(|graphics| {
            for clipped in graphics
                .get(ghost)
                .into_iter()
                .flat_map(|l| l.all_entries())
            {
                match &clipped.shape {
                    egui::Shape::Mesh(mesh) => vertices.extend(mesh.vertices.iter().map(|v| v.pos)),
                    egui::Shape::Noop => {}
                    other => flat.push(format!("{other:?}")),
                }
            }
        });
    });
    output.textures_delta.clear();
    assert!(flat.is_empty(), "the ghost is turned into meshes: {flat:?}");
    assert!(!vertices.is_empty(), "the ghost is drawn");
    let center = vertices
        .iter()
        .fold(egui::Vec2::ZERO, |sum, v| sum + v.to_vec2())
        / vertices.len() as f32;
    let top = vertices.iter().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    assert!(top.x > center.x, "the top-right corner rises");
}

/// A board with a card in `todo/`, `ready/` and `doing/`, its Todo card
/// lifted, the dock all the way in.
fn lifted_card() -> (Scratch, IboardApp, egui::Context) {
    let dir = scratch_dir("lifted");
    for column in ["todo", "ready", "doing"] {
        let folder = dir.join("issues").join(column);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(format!("{column}.md")), "# Demo\n").unwrap();
    }
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let start = card_centre(&ctx, &dir, "todo");
    draw(&ctx, &mut app, vec![moved(start)]);
    draw(&ctx, &mut app, vec![moved(start), button(start, true)]);
    let carried = start + egui::vec2(30.0, 30.0);
    for _ in 0..20 {
        draw(&ctx, &mut app, vec![moved(carried)]);
    }
    assert!(egui::DragAndDrop::has_payload_of_type::<IssueDrag>(&ctx));
    (dir, app, ctx)
}

fn card_centre(ctx: &egui::Context, dir: &Path, column: &str) -> egui::Pos2 {
    let file = dir.join("issues").join(column).join(format!("{column}.md"));
    ctx.read_response(egui::Id::new(("issue_card", &file)))
        .expect("the card is drawn")
        .rect
        .center()
}

#[test]
fn the_drop_line_shows_only_where_position_counts() {
    let (dir, mut app, ctx) = lifted_card();
    let mut lines_over = |column: &str| {
        let over = card_centre(&ctx, &dir, column);
        draw(&ctx, &mut app, vec![moved(over)]);
        let (p, shapes) = painted(&ctx, &mut app, vec![moved(over)]);
        shapes
            .iter()
            .filter(|shape| {
                matches!(shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == p.accent)
            })
            .count()
    };
    assert_eq!(lines_over("ready"), 1);
    assert_eq!(lines_over("doing"), 0);
}

#[test]
fn the_dock_keeps_12pt_of_padding_in_the_dragged_cards_column() {
    let (_dir, _app, ctx) = lifted_card();
    let column = ctx
        .read_response(egui::Id::new(("column_drop", 0)))
        .unwrap()
        .rect;
    let zone = ctx
        .read_response(egui::Id::new("drop_zone_delete"))
        .expect("the dock shows while dragging")
        .rect;
    assert_eq!(zone.min.x - column.min.x, theme::SPACE_4);
    assert_eq!(column.max.x - zone.max.x, theme::SPACE_4);
    assert_eq!(zone.height(), 56.0);
}

#[test]
fn a_drag_starts_once_the_pointer_moved_4pt() {
    let dir = scratch_dir("drag-start");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::write(todo.join("todo.md"), "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let start = card_centre(&ctx, &dir, "todo");
    draw(&ctx, &mut app, vec![moved(start)]);
    draw(&ctx, &mut app, vec![moved(start), button(start, true)]);
    draw(&ctx, &mut app, vec![moved(start + egui::vec2(3.5, 0.0))]);
    assert!(!egui::DragAndDrop::has_any_payload(&ctx));
    let past = start + egui::vec2(theme::DRAG_START + 0.5, 0.0);
    draw(&ctx, &mut app, vec![moved(past)]);
    draw(&ctx, &mut app, vec![moved(past)]);
    assert!(egui::DragAndDrop::has_payload_of_type::<IssueDrag>(&ctx));
}
