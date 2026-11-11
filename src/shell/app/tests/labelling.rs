//! The labels on the board: an active filter, and the LabelPicker's pills.

use super::*;

/// `todo/a.md` and `todo/b.md`, both labelled `bug`, with `a`'s picker open
/// and its field focused.
fn picking() -> (Scratch, IboardApp, egui::Context, PathBuf) {
    let dir = scratch_dir("picker");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let a = todo.join("a.md");
    std::fs::write(&a, "# [bug] A\n").unwrap();
    std::fs::write(todo.join("b.md"), "# [bug] B\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    app.toggle_label_picker(a.clone());
    // Past the popup's fade-in, so its colours are the ones it paints at rest.
    for _ in 0..30 {
        draw(&ctx, &mut app, vec![]);
    }
    assert_eq!(focused(&ctx), Some(field()));
    (dir, app, ctx, a)
}

fn field() -> egui::Id {
    egui::Id::unique("label_picker_field")
}

fn focused(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.memory(|m| m.focused())
}

fn rects(shapes: &[egui::Shape]) -> impl Iterator<Item = &egui::epaint::RectShape> {
    shapes.iter().filter_map(|shape| match shape {
        egui::Shape::Rect(rect) => Some(rect),
        _ => None,
    })
}

#[test]
fn the_active_filter_lights_its_label_on_the_card() {
    let (_dir, mut app, ctx, _) = picking();
    app.session.label_picker = None;
    let lit = |p: &Palette, shapes: &[egui::Shape]| {
        rects(shapes)
            .filter(|rect| rect.rect.height() == 18.0)
            .filter(|rect| {
                rect.fill == p.accent_soft && rect.corner_radius.nw == theme::RADIUS_PILL
            })
            .count()
    };
    let (p, shapes) = painted(&ctx, &mut app, vec![]);
    assert_eq!(lit(p, &shapes), 0);
    app.filter_bar.filter.pick("bug");
    let (p, shapes) = painted(&ctx, &mut app, vec![]);
    assert_eq!(lit(p, &shapes), 2, "on both cards");
}

#[test]
fn the_keyboard_reaches_a_label_the_issue_has_and_typing_goes_back() {
    let (_dir, mut app, ctx, _) = picking();
    draw(
        &ctx,
        &mut app,
        vec![key(Key::ArrowDown, Default::default())],
    );
    let (p, shapes) = painted(&ctx, &mut app, vec![]);
    assert!(focused(&ctx).is_some_and(|id| id != field()));
    let ring = rects(&shapes)
        .any(|rect| rect.stroke.color == p.accent && rect.stroke_kind == egui::StrokeKind::Outside);
    assert!(ring, "a focused pill takes the accent ring");

    draw(&ctx, &mut app, vec![egui::Event::Text("x".to_string())]);
    draw(&ctx, &mut app, vec![]);
    assert_eq!(app.session.label_picker.as_ref().unwrap().text, "x");
    assert_eq!(focused(&ctx), Some(field()));
}

#[test]
fn space_on_a_focused_label_toggles_it_without_typing() {
    let (_dir, mut app, ctx, a) = picking();
    draw(
        &ctx,
        &mut app,
        vec![key(Key::ArrowDown, Default::default())],
    );
    draw(&ctx, &mut app, vec![]);
    let space = vec![
        key(Key::Space, Default::default()),
        egui::Event::Text(" ".to_string()),
    ];
    draw(&ctx, &mut app, space);
    assert!(app.board.find(&a).unwrap().labels.is_empty());
    assert_eq!(app.session.label_picker.as_ref().unwrap().text, "");
}

#[test]
fn a_removed_label_flashes_its_edge() {
    let (_dir, mut app, ctx, a) = picking();
    let flash = |p: &Palette, shapes: &[egui::Shape]| {
        rects(shapes)
            .filter(|rect| rect.stroke.color == p.status_done)
            .count()
    };
    let (p, shapes) = painted(&ctx, &mut app, vec![]);
    assert_eq!(flash(p, &shapes), 0);
    app.remove_label("bug", ctx.input(|i| i.time));
    assert!(app.board.find(&a).unwrap().labels.is_empty());
    let (p, shapes) = painted(&ctx, &mut app, vec![]);
    assert_eq!(flash(p, &shapes), 1);
}
