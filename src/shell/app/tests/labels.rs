//! The labels on an issue: the chip a labeled issue shows, the LabelPicker's
//! keyboard, and the columns that offer no picker.

use super::*;

/// A labeled issue shows its chip on the card and in the reader header,
/// with the label split off the display title, without rewriting the file.
#[test]
fn renders_a_labeled_issue() {
    let dir = scratch_dir("labels");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("labeled.md");
    let raw_markdown = "# [Discovery] Hey look!\n\nA labeled idea.\n";
    std::fs::write(&file, raw_markdown).unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
    let ctx = themed_ctx();

    let issue = app.board.find(&file).unwrap();
    assert_eq!(issue.title, "Hey look!");
    assert_eq!(issue.labels, ["discovery"]);

    draw(&ctx, &mut app, vec![]); // board: chip beside the card title

    app.open_issue(file.clone());
    draw(&ctx, &mut app, vec![]); // reader: chip in the head strip

    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        raw_markdown,
        "the raw file is never rewritten for a label"
    );
}

/// `L` opens the LabelPicker on the selected card; `↩` writes the typed
/// label and keeps the picker open, `↓ ↩` adds a board label, and `esc`
/// closes without writing what is left in the field.
#[test]
fn the_label_picker_adds_labels_from_the_keyboard() {
    let dir = scratch_dir("label-picker");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let login = todo.join("login.md");
    std::fs::write(&login, "# [Bug] Fix login\n\nA bug.\n").unwrap();
    std::fs::write(todo.join("export.md"), "# [Feature] Export\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.session.selected = Some(login.clone());

    let ctx = themed_ctx();
    let key = |key: Key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let field = egui::Id::unique("label_picker_field");

    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![key(Key::L)]);
    assert!(app.session.label_picker.is_some(), "L opens the picker");
    frame(&ctx, &mut app, vec![]);
    assert!(ctx.memory(|m| m.has_focus(field)), "the field takes focus");

    frame(&ctx, &mut app, vec![egui::Event::Text("Tech debt".into())]);
    frame(&ctx, &mut app, vec![key(Key::Enter)]);
    assert_eq!(
        std::fs::read_to_string(&login).unwrap(),
        "# [Bug] [tech-debt] Fix login\n\nA bug.\n"
    );
    let picker = app
        .session
        .label_picker
        .as_ref()
        .expect("the picker stays open");
    assert!(picker.text.is_empty(), "the field is cleared");
    frame(&ctx, &mut app, vec![]);
    assert!(
        ctx.memory(|m| m.has_focus(field)),
        "focus stays in the field"
    );

    // The board's labels, most used first: bug, which it has, then feature.
    // A frame for the field's focus filter to settle, as egui sets it the
    // frame after focus lands.
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![key(Key::ArrowDown)]);
    frame(&ctx, &mut app, vec![key(Key::Tab)]);
    frame(&ctx, &mut app, vec![key(Key::Enter)]);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(
        app.board.find(&login).unwrap().labels,
        ["bug", "feature", "tech-debt"],
        "labels are shown alphabetically"
    );

    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![egui::Event::Text("left over".into())]);
    frame(&ctx, &mut app, vec![key(Key::Escape)]);
    assert!(app.session.label_picker.is_none(), "esc closes the picker");
    assert!(!std::fs::read_to_string(&login).unwrap().contains("left"));
    assert!(login.exists(), "the file name never changes");
}

#[test]
fn a_doing_issue_offers_no_label_picker() {
    let dir = scratch_dir("label-doing");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&doing).unwrap();
    let wip = doing.join("wip.md");
    std::fs::write(&wip, "# [bug] Work in progress\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.session.selected = Some(wip.clone());

    let ctx = themed_ctx();
    frame(&ctx, &mut app, vec![]);
    let add = egui::Id::unique(("card_add_label", &wip));
    assert!(ctx.read_response(add).is_none(), "no Add label on the card");
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: Key::L,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.session.label_picker.is_none(), "L opens no picker");
    app.toggle_label_picker(wip.clone());
    assert!(app.session.label_picker.is_none());
}
