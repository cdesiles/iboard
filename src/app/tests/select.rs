//! The multi-selection: shift-clicks, its clear button, and actions and drags
//! that carry every picked card.

use super::*;

/// A board with `todo/a.md`, `todo/b.md`, `todo/c.md` and an empty `doing/`.
fn board(tag: &str) -> (Scratch, IboardApp, [PathBuf; 3]) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(dir.join("issues/doing")).unwrap();
    let files = ["a", "b", "c"].map(|name| {
        let file = todo.join(format!("{name}.md"));
        std::fs::write(&file, format!("# {name}\n")).unwrap();
        file
    });
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    (dir, app, files)
}

fn card_center(ctx: &egui::Context, path: &Path) -> egui::Pos2 {
    ctx.read_response(egui::Id::new(("issue_card", path)))
        .expect("the card is drawn")
        .rect
        .center()
}

/// A press and release at `at`, `shift` held or not.
fn click(ctx: &egui::Context, app: &mut IboardApp, at: egui::Pos2, shift: bool) {
    let modifiers = egui::Modifiers {
        shift,
        ..Default::default()
    };
    let held = egui::Event::ModifiersChanged(modifiers);
    draw(ctx, app, vec![held.clone(), moved(at)]);
    draw(ctx, app, vec![held.clone(), button(at, true)]);
    draw(ctx, app, vec![held, button(at, false)]);
}

#[test]
fn shift_clicks_pick_cards_and_unpick_them() {
    let (_dir, mut app, [a, b, _]) = board("pick");
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);

    click(&ctx, &mut app, card_center(&ctx, &a), true);
    click(&ctx, &mut app, card_center(&ctx, &b), true);
    assert!(matches!(app.session.screen, Screen::Board), "nothing opens");
    assert_eq!(app.session.picked, [a.clone(), b.clone()]);

    click(&ctx, &mut app, card_center(&ctx, &a), true);
    assert_eq!(
        app.session.picked,
        std::slice::from_ref(&b),
        "a picked card leaves"
    );
}

#[test]
fn a_plain_click_voids_the_selection() {
    let (_dir, mut app, [a, b, c]) = board("void");
    app.pick(a);
    app.pick(b);
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);

    click(&ctx, &mut app, card_center(&ctx, &c), false);
    assert!(app.session.picked.is_empty());
    assert_eq!(app.session.selected.as_deref(), Some(c.as_path()));
}

#[test]
fn the_column_holding_the_selection_offers_to_clear_it() {
    let (_dir, mut app, [a, b, _]) = board("clear");
    app.pick(a);
    app.pick(b);
    let todo = app.board.todo_column().unwrap().dir.clone();
    let index = |dir: &Path| app.board.columns.iter().position(|c| c.dir == dir);
    let (todo, doing) = (
        index(&todo).unwrap(),
        index(&app.board.root.join("doing")).unwrap(),
    );
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let clear = |index: usize| egui::Id::new(("clear_picked", index));
    assert!(ctx.read_response(clear(doing)).is_none());
    let at = ctx.read_response(clear(todo)).expect("shown").rect.center();

    click(&ctx, &mut app, at, false);
    assert!(app.session.picked.is_empty() && app.session.selected.is_none());
}

#[test]
fn dragging_a_picked_card_carries_the_selection() {
    let (dir, mut app, [a, b, c]) = board("carry");
    app.pick(a.clone());
    app.pick(c.clone());
    let doing = dir.join("issues/doing");

    drag_card_to_column(&mut app, &a, &doing);

    assert!(doing.join("a.md").exists() && doing.join("c.md").exists());
    assert!(b.exists(), "an unpicked card stays");
    assert_eq!(
        app.session.picked,
        [doing.join("a.md"), doing.join("c.md")],
        "the selection follows its files"
    );
}

/// Presses on `path`'s card and drops it over the column of `dir`.
fn drag_card_to_column(app: &mut IboardApp, path: &Path, dir: &Path) {
    let ctx = themed_ctx();
    draw(&ctx, app, vec![]);
    let index = app.board.columns.iter().position(|c| c.dir == dir).unwrap();
    let target = ctx
        .read_response(egui::Id::new(("column_drop", index)))
        .unwrap()
        .rect
        .center();
    let start = card_center(&ctx, path);
    draw(&ctx, app, vec![moved(start)]);
    draw(&ctx, app, vec![moved(start), button(start, true)]);
    draw(&ctx, app, vec![moved(start + egui::vec2(30.0, 0.0))]);
    draw(&ctx, app, vec![moved(target)]);
    draw(&ctx, app, vec![moved(target)]);
    draw(&ctx, app, vec![moved(target), button(target, false)]);
}

#[test]
fn picked_cards_land_together_in_front_of_the_anchor() {
    let dir = scratch_dir("land");
    let ready = dir.join("issues/ready");
    std::fs::create_dir_all(&ready).unwrap();
    for name in ["001-a.md", "002-b.md", "003-c.md", "004-d.md"] {
        std::fs::write(ready.join(name), "# x\n").unwrap();
    }
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.pick(ready.join("001-a.md"));
    app.pick(ready.join("004-d.md"));

    app.place_issue(
        &ready.join("001-a.md"),
        &ready,
        Some(ready.join("003-c.md")),
    );

    let mut names: Vec<String> = std::fs::read_dir(&ready)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["001-b.md", "002-a.md", "003-d.md", "004-c.md"]);
}

#[test]
fn deleting_a_picked_card_deletes_the_selection() {
    let (_dir, mut app, [a, b, c]) = board("delete");
    app.pick(a.clone());
    app.pick(b.clone());

    app.request_delete(b.clone());
    assert!(
        matches!(&app.session.confirm, Some(Confirm::Delete(paths)) if *paths == [a.clone(), b.clone()]),
        "one confirmation for both"
    );

    app.confirm_delete = false;
    app.request_delete(a.clone());
    assert!(!a.exists() && !b.exists() && c.exists());
    assert!(app.session.picked.is_empty());
    app.undo_last();
    assert!(a.exists() && b.exists(), "⌘Z brings both back");
}

#[test]
fn a_label_added_on_a_picked_card_goes_on_the_selection() {
    let (_dir, mut app, [a, b, c]) = board("label");
    app.pick(a.clone());
    app.pick(b.clone());
    app.toggle_label_picker(a.clone());

    app.add_label("ux", 0.0);

    for (path, labelled) in [(&a, true), (&b, true), (&c, false)] {
        let issue = app.board.find(path).unwrap();
        assert_eq!(issue.labels.contains(&"ux".to_string()), labelled);
    }
}
