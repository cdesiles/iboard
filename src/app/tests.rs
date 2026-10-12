//! The board window's tests, driven through real egui frames.

use super::enrich::enrichment_dir;
use super::nav::step_selection;
use super::*;

mod review;
use review::open_issue_with_review;

/// A context with the design system installed, past the first frame so
/// the Plex font families are loaded.
fn themed_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    theme::install(&ctx);
    egui_extras::install_image_loaders(&ctx);
    ctx.run_ui(egui::RawInput::default(), |_| {})
        .textures_delta
        .clear();
    ctx
}

/// Polls `done` until it holds, for as long as a loaded machine may need.
fn until(mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// A folder for one test, removed when it drops, even when the test
/// fails. It reads as the folder's `Path`.
struct Scratch(tempfile::TempDir);

impl std::ops::Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        self.0.path()
    }
}

fn scratch_dir(tag: &str) -> Scratch {
    let dir = tempfile::Builder::new()
        .prefix(&format!("iboard-{tag}-"))
        .tempdir()
        .unwrap();
    Scratch(dir)
}

/// The 1280×800 window every UI test draws in.
fn window() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))
}

fn run(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    draw: impl FnMut(&mut egui::Ui),
) -> egui::PlatformOutput {
    let input = egui::RawInput {
        screen_rect: Some(window()),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, draw);
    output.textures_delta.clear();
    output.platform_output
}

/// One frame of `app` that only draws, `events` as its input.
fn draw(
    ctx: &egui::Context,
    app: &mut IboardApp,
    events: Vec<egui::Event>,
) -> egui::PlatformOutput {
    run(ctx, events, |ui| app.draw(ui))
}

/// One frame of `app` as eframe runs it: its logic, then the drawing.
fn frame(
    ctx: &egui::Context,
    app: &mut IboardApp,
    events: Vec<egui::Event>,
) -> egui::PlatformOutput {
    run(ctx, events, |ui| {
        app.frame_logic(ui.ctx());
        app.draw(ui);
    })
}

fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn moved(pos: egui::Pos2) -> egui::Event {
    egui::Event::PointerMoved(pos)
}

/// The primary button going down, or up, at `pos`.
fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
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

/// Drives the board, the composer, the checkbox reader and the editor through a real
/// egui pass, so runtime mistakes surface without a display.
#[test]
fn renders_board_reader_and_editor() {
    let dir = scratch_dir("render");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\n- [ ] one\n- [ ] two\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    let ctx = themed_ctx();

    draw(&ctx, &mut app, vec![]); // board

    app.open_issue(file.clone());
    draw(&ctx, &mut app, vec![]); // rendered preview with mutable checkboxes

    app.start_editing(&file);
    draw(&ctx, &mut app, vec![]); // editor

    app.screen = Screen::Board;
    app.open_composer();
    app.composer.as_mut().unwrap().title = "Export board".to_string();
    draw(&ctx, &mut app, vec![]); // board with the quick-add composer open

    app.composer = None;
    app.move_selection(Key::ArrowDown);
    draw(&ctx, &mut app, vec![]); // board with a selected card

    app.auto = false;
    app.start_editing(&file);
    app.editor.as_mut().unwrap().text.push_str("more\n");
    app.show_preview();
    draw(&ctx, &mut app, vec![]); // editor asking before leaving
}

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
    assert_eq!(app.detail_mode, DetailMode::Edit);
    assert!(app.focus_editor, "the caret goes under the new heading");
    let editor = app.editor.as_ref().unwrap();
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

#[test]
fn moving_an_issue_reparents_the_file_and_refreshes_the_board() {
    let dir = scratch_dir("move");
    let todo = dir.join("issues/todo");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(&doing).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
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
    let dir = scratch_dir("ready");
    let todo = dir.join("issues/todo");
    let ready = dir.join("issues/ready");
    let doing = dir.join("issues/doing");
    for d in [&todo, &ready, &doing] {
        std::fs::create_dir_all(d).unwrap();
    }
    let file = todo.join("make-colour-green.md");
    std::fs::write(&file, "# Green\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
    app.selected = Some(file.clone());

    app.move_issue(&file, &ready);
    let queued = ready.join("001-make-colour-green.md");
    assert!(queued.exists(), "entering Ready adds the priority number");
    assert_eq!(app.selected.as_deref(), Some(queued.as_path()));

    app.move_issue(&queued, &doing);
    assert!(
        doing.join("make-colour-green.md").exists(),
        "leaving Ready drops it"
    );
    assert!(app.error.is_none());
}

#[test]
fn opening_another_project_rebuilds_the_board_and_remembers_it() {
    let alpha = scratch_dir("switch-alpha");
    let beta = scratch_dir("switch-beta");
    std::fs::create_dir_all(alpha.join("issues/todo")).unwrap();
    std::fs::write(alpha.join("issues/todo/a.md"), "# Alpha\n").unwrap();
    std::fs::create_dir_all(beta.join("issues/ready")).unwrap();
    std::fs::write(beta.join("issues/ready/b.md"), "# Beta\n").unwrap();

    let located = discovery::locate(&alpha).unwrap();
    let mut app = IboardApp::from_located(located);
    // What belongs to alpha must not follow the switch.
    let a = alpha.join("issues/todo/a.md");
    app.undo = Some(Deleted {
        path: a.clone(),
        trashed: None,
        content: Vec::new(),
        at: Instant::now(),
    });
    app.notice = Some(("Archived a.md".to_string(), Instant::now()));
    app.pending_front = Some((alpha.join("issues/todo"), a.clone()));
    app.design_error = Some("alpha has no design".to_string());
    app.focused_column = Some(alpha.join("issues/todo"));

    app.open_project(beta.to_path_buf());

    assert!(app.undo.is_none(), "⌘Z cannot restore into the old project");
    assert!(app.notice.is_none() && app.pending_front.is_none());
    assert!(app.design_error.is_none() && app.design_system.is_none());
    assert!(app.focused_column.is_none());
    assert!(app.error.is_none());
    assert_eq!(
        recents::canonical(&app.board.root),
        recents::canonical(&beta.join("issues")),
        "the board now belongs to the target project"
    );
    assert!(matches!(app.screen, Screen::Board));
    assert_eq!(app.recents, vec![recents::canonical(&beta)]);
    assert_eq!(
        app.pending_title.as_deref(),
        beta.file_name().and_then(|name| name.to_str()),
        "the window takes the new project's name"
    );

    // A folder with no issues directory reports an error and changes nothing.
    let empty = scratch_dir("switch-empty");
    let before = app.board.root.clone();
    app.open_project(empty.to_path_buf());
    assert!(app.error.is_some());
    assert_eq!(app.board.root, before);
}

#[test]
fn moving_never_overwrites_an_existing_file() {
    let dir = scratch_dir("clobber");
    let todo = dir.join("issues/todo");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(&doing).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let existing = doing.join("01-demo.md");
    std::fs::write(&existing, "# Other\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    app.move_issue(&file, &doing);

    assert!(file.exists(), "source must survive a refused move");
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "# Other\n");
    assert!(app.error.is_some(), "the refusal is surfaced to the user");
    app.refresh();
    assert!(app.error.is_some(), "and the next scan does not wipe it");
}

#[test]
fn a_scan_error_clears_itself_once_the_board_reads_again() {
    let dir = scratch_dir("scan-error");
    let issues = dir.join("issues");
    std::fs::create_dir_all(issues.join("todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    std::fs::rename(&issues, dir.join("gone")).unwrap();
    app.refresh();
    assert!(app.scan_error.is_some());
    assert!(app.error.is_none(), "a scan is not an action");

    std::fs::rename(dir.join("gone"), &issues).unwrap();
    app.refresh();
    assert!(app.scan_error.is_none());
}

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
        app.notice.as_ref().map(|(text, _)| text.as_str()),
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
        matches!(&app.confirm, Some(Confirm::Delete(path)) if *path == file),
        "the drop opens the confirmation"
    );
    assert!(file.exists(), "nothing is removed until confirmed");
    app.delete_issue(&file);
    assert!(!file.exists());
    assert_eq!(
        app.notice.as_ref().map(|(text, _)| text.as_str()),
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
    assert!(app.confirm.is_none());
}

#[test]
fn a_archives_the_selected_done_card() {
    let dir = scratch_dir("archive-key");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    let file = done.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.selected = Some(file.clone());

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

/// `⌘F` opens the filter with its suggestions; `↓ ↩` picks one as a token,
/// `⌫` removes it, and `esc` clears before it leaves the field.
#[test]
fn the_filter_picks_suggestions_from_the_keyboard() {
    let dir = scratch_dir("filter-keys");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::write(todo.join("login.md"), "# [Bug] Fix login\n\nA bug.\n").unwrap();
    std::fs::write(todo.join("export.md"), "# [Feature] Export\n\nA feature.\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    let ctx = themed_ctx();
    let focused = |ctx: &egui::Context| ctx.memory(|m| m.has_focus(filter_id()));

    frame(&ctx, &mut app, vec![]);
    // What `⌘F` sets.
    app.focus_filter = true;
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    assert!(focused(&ctx), "the filter takes focus");

    frame(
        &ctx,
        &mut app,
        vec![key(Key::ArrowDown, egui::Modifiers::NONE)],
    );
    assert_eq!(app.chip_cursor, Some(0), "↓ reaches the first chip");
    frame(&ctx, &mut app, vec![key(Key::Enter, egui::Modifiers::NONE)]);
    assert_eq!(app.filter.tokens, vec!["bug"]);
    assert!(focused(&ctx), "picking keeps focus in the field");
    assert_eq!(app.visible_grid().concat().len(), 1, "only the bug matches");

    frame(
        &ctx,
        &mut app,
        vec![key(Key::Escape, egui::Modifiers::NONE)],
    );
    assert_eq!(
        app.chip_cursor, None,
        "esc goes back from the chips to the text"
    );
    assert!(focused(&ctx), "leaving the chips keeps focus");
    frame(
        &ctx,
        &mut app,
        vec![key(Key::Backspace, egui::Modifiers::NONE)],
    );
    assert!(
        app.filter.tokens.is_empty(),
        "⌫ in an empty field drops the token"
    );

    app.filter.pick("feature");
    frame(
        &ctx,
        &mut app,
        vec![key(Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.filter.is_empty(), "the first esc clears");
    assert!(focused(&ctx));
    frame(
        &ctx,
        &mut app,
        vec![key(Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(!focused(&ctx), "the second esc leaves the field");
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
    app.selected = Some(login.clone());

    let ctx = themed_ctx();
    let key = |key: Key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let field = egui::Id::new("label_picker_field");

    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![key(Key::L)]);
    assert!(app.label_picker.is_some(), "L opens the picker");
    frame(&ctx, &mut app, vec![]);
    assert!(ctx.memory(|m| m.has_focus(field)), "the field takes focus");

    frame(&ctx, &mut app, vec![egui::Event::Text("Tech debt".into())]);
    frame(&ctx, &mut app, vec![key(Key::Enter)]);
    assert_eq!(
        std::fs::read_to_string(&login).unwrap(),
        "# [Bug] [tech-debt] Fix login\n\nA bug.\n"
    );
    let picker = app.label_picker.as_ref().expect("the picker stays open");
    assert!(picker.text.is_empty(), "the field is cleared");
    frame(&ctx, &mut app, vec![]);
    assert!(
        ctx.memory(|m| m.has_focus(field)),
        "focus stays in the field"
    );

    // The board's labels, most used first, minus the two it has: feature.
    frame(&ctx, &mut app, vec![key(Key::ArrowDown)]);
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
    assert!(app.label_picker.is_none(), "esc closes the picker");
    assert!(!std::fs::read_to_string(&login).unwrap().contains("left"));
    assert!(login.exists(), "the file name never changes");
}

/// A press and release without movement is still a click, not a drag.
#[test]
fn clicking_a_card_still_opens_it() {
    let dir = scratch_dir("click");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    let ctx = themed_ctx();

    draw(&ctx, &mut app, vec![]);
    let card_id = egui::Id::new(("issue_card", &file));
    let start = ctx.read_response(card_id).unwrap().rect.center();

    draw(&ctx, &mut app, vec![egui::Event::PointerMoved(start)]);
    draw(
        &ctx,
        &mut app,
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
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
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );

    assert!(
        matches!(app.screen, Screen::Detail(path) if path == file),
        "a plain click should open the issue"
    );
}

/// Hover actions are not drag handles: over the card the cursor is the
/// grab hand, but over an action button it is the pointer.
#[test]
fn hovering_a_card_action_shows_the_pointer_cursor() {
    let dir = scratch_dir("cursor");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    let ctx = themed_ctx();

    draw(&ctx, &mut app, vec![]);
    let center = ctx
        .read_response(egui::Id::new(("issue_card", &file)))
        .unwrap()
        .rect
        .center();

    // The card body is a drag handle.
    draw(&ctx, &mut app, vec![egui::Event::PointerMoved(center)]);
    assert_eq!(
        draw(&ctx, &mut app, vec![egui::Event::PointerMoved(center)]).cursor_icon,
        egui::CursorIcon::Grab,
        "the card invites a drag"
    );

    // Its Delete action is a click target, not a drag handle.
    let delete = ctx
        .read_response(egui::Id::new(("card_delete", &file)))
        .expect("actions show on hover")
        .rect
        .center();
    draw(&ctx, &mut app, vec![egui::Event::PointerMoved(delete)]);
    assert_eq!(
        draw(&ctx, &mut app, vec![egui::Event::PointerMoved(delete)]).cursor_icon,
        egui::CursorIcon::PointingHand,
        "an action button is not a drag handle"
    );
}

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
    app.editor.as_mut().unwrap().name = "01-renamed".to_string();
    app.flush_autosave(false);

    let renamed = todo.join("01-renamed.md");
    assert!(renamed.exists(), "the issue is renamed on disk");
    assert!(!file.exists(), "the old file name is gone");
    assert!(app.error.is_none(), "a clean rename reports no error");
    assert!(
        matches!(&app.screen, Screen::Detail(path) if path == &renamed),
        "the detail view follows the renamed file"
    );
    assert_eq!(app.editor.as_ref().unwrap().path, renamed);
    assert_eq!(
        app.board.find(&renamed).map(|issue| issue.title.as_str()),
        Some("Demo"),
        "the board adopts the renamed issue"
    );
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
    app.editor.as_mut().unwrap().name = "02-other.md".to_string();
    app.flush_autosave(false);

    assert!(file.exists(), "a refused rename leaves the file in place");
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "# Other\n");
    assert!(
        app.editor.as_ref().unwrap().error.is_some(),
        "the clash is surfaced to the user"
    );
}

/// A glyph missing from a font stack draws as nothing, so check the key
/// symbols of the shortcut hints against the fonts that draw them.
#[test]
fn shortcut_glyphs_exist_in_their_fonts() {
    let ctx = themed_ctx();
    for font in [theme::meta(), theme::ui_button()] {
        for glyph in ['⌘', '↩'] {
            assert!(
                ctx.fonts_mut(|f| f.has_glyph(&font, glyph)),
                "{glyph} is missing from {font:?}"
            );
        }
    }
}

fn grid(columns: &[&[&str]]) -> Vec<Vec<PathBuf>> {
    columns
        .iter()
        .map(|paths| paths.iter().map(PathBuf::from).collect())
        .collect()
}

/// At the minimum window width the three columns fit, one gutter in from
/// each edge, without scrolling, even with the composer open.
#[test]
fn columns_fit_the_narrowest_window_with_gutters() {
    let dir = scratch_dir("fit");
    let mut cards = Vec::new();
    for column in ["todo", "ready", "doing", "done"] {
        let folder = dir.join("issues").join(column);
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("01-card.md");
        std::fs::write(
            &file,
            "# Card\n\nA summary long enough to wrap a few times.\n",
        )
        .unwrap();
        cards.push(file);
    }

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
    // The composer's thicker frame must not widen Todo either.
    app.open_composer();
    let ctx = themed_ctx();
    let width = 880.0;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 480.0),
        )),
        ..Default::default()
    };
    for _ in 0..2 {
        ctx.run_ui(input.clone(), |ui| app.draw(ui))
            .textures_delta
            .clear();
    }

    let gutter = theme::SPACE_5;
    for card in &cards {
        let rect = ctx
            .read_response(egui::Id::new(("issue_card", card)))
            .expect("every card is drawn")
            .rect;
        assert!(
            rect.min.x >= gutter - 0.5 && rect.max.x <= width - gutter + 0.5,
            "{} at {rect:?} is outside the gutters",
            card.display()
        );
    }
}

#[test]
fn arrows_step_through_cards_and_skip_empty_columns() {
    let board = grid(&[&["a1", "a2", "a3"], &[], &["c1"]]);
    let step = |at, key| step_selection(&board, at, key);

    assert_eq!(step(None, Key::ArrowRight), Some((0, 0)), "first card");
    assert_eq!(step(Some((0, 0)), Key::ArrowDown), Some((0, 1)));
    assert_eq!(
        step(Some((0, 2)), Key::ArrowDown),
        Some((0, 2)),
        "stops at the end"
    );
    assert_eq!(
        step(Some((0, 0)), Key::ArrowUp),
        Some((0, 0)),
        "stops at the top"
    );
    assert_eq!(
        step(Some((0, 2)), Key::ArrowRight),
        Some((2, 0)),
        "skips the empty column and clamps the row"
    );
    assert_eq!(step(Some((2, 0)), Key::ArrowLeft), Some((0, 0)));
    assert_eq!(
        step(Some((2, 0)), Key::ArrowRight),
        Some((2, 0)),
        "last column"
    );
    assert_eq!(
        step_selection(&grid(&[&[], &[]]), None, Key::ArrowDown),
        None
    );
}

#[test]
fn selection_follows_the_filter_and_survives_opening() {
    let dir = scratch_dir("select");
    let todo = dir.join("issues/todo");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(&doing).unwrap();
    std::fs::write(todo.join("01-alpha.md"), "# Alpha\n").unwrap();
    std::fs::write(todo.join("02-beta.md"), "# Beta\n").unwrap();
    std::fs::write(doing.join("01-gamma.md"), "# Gamma\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    app.move_selection(Key::ArrowDown);
    assert_eq!(app.selected, Some(todo.join("01-alpha.md")));
    app.move_selection(Key::ArrowDown);
    app.move_selection(Key::ArrowRight);
    assert_eq!(app.selected, Some(doing.join("01-gamma.md")));

    // Filtered out cards are skipped.
    app.filter.text = "beta".to_string();
    app.selected = None;
    app.move_selection(Key::ArrowDown);
    assert_eq!(app.selected, Some(todo.join("02-beta.md")));

    // Back from the reader, the card is still selected.
    app.open_issue(todo.join("02-beta.md"));
    app.back_to_board();
    assert_eq!(app.selected, Some(todo.join("02-beta.md")));
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
    app.auto = auto;
    app.open_issue(file.clone());
    app.start_editing(&file);
    app.editor.as_mut().unwrap().text = "# Demo\n\nmore\n".to_string();
    (dir, file, app)
}

#[test]
fn leaving_with_auto_on_saves() {
    let (_dir, file, mut app) = editing_with_unsaved_change("leave-auto", true);

    app.show_preview();

    assert_eq!(app.detail_mode, DetailMode::Preview);
    assert!(
        app.leave_prompt.is_none(),
        "no question when the save worked"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n\nmore\n");
}

#[test]
fn leaving_with_auto_off_asks_first() {
    let (_dir, file, mut app) = editing_with_unsaved_change("leave-ask", false);

    app.back_to_board();
    assert_eq!(app.leave_prompt, Some(Leave::Board), "the editor asks");
    assert_eq!(app.detail_mode, DetailMode::Edit, "and stays put");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n");

    app.answer_leave(LeaveChoice::KeepEditing);
    assert!(app.leave_prompt.is_none());
    assert_eq!(app.detail_mode, DetailMode::Edit);

    app.back_to_board();
    app.answer_leave(LeaveChoice::Save);
    assert!(matches!(app.screen, Screen::Board), "saved, then left");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n\nmore\n");
}

#[test]
fn discarding_on_leave_keeps_the_file() {
    let (_dir, file, mut app) = editing_with_unsaved_change("leave-discard", false);

    app.open_composer();
    assert_eq!(app.leave_prompt, Some(Leave::Composer));
    app.answer_leave(LeaveChoice::Discard);

    assert!(matches!(app.screen, Screen::Board));
    assert!(app.composer.is_some(), "the composer opens after leaving");
    assert!(!app.has_unsaved_edits());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n");
}

#[test]
fn a_conflict_asks_even_with_auto_on() {
    let (_dir, file, mut app) = editing_with_unsaved_change("leave-conflict", true);
    std::fs::write(&file, "# Changed elsewhere\n").unwrap();
    app.refresh();

    app.show_preview();

    assert_eq!(
        app.leave_prompt,
        Some(Leave::Reader),
        "autosave cannot resolve it"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Changed elsewhere\n"
    );
}

#[test]
fn the_composer_creates_issues_and_stays_open() {
    let dir = scratch_dir("compose");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    app.open_composer();
    app.composer.as_mut().unwrap().title = "Fix login redirect".to_string();
    app.submit_composer(false);

    let created = todo.join("fix-login-redirect.md");
    assert_eq!(
        std::fs::read_to_string(&created).unwrap(),
        "# Fix login redirect\n"
    );
    let composer = app.composer.as_ref().expect("the composer stays open");
    assert!(
        composer.title.is_empty(),
        "the field clears for the next idea"
    );
    assert!(matches!(app.screen, Screen::Board));
    assert!(app.board.find(&created).is_some(), "the board sees it");

    // An empty title never creates a file.
    app.submit_composer(false);
    assert_eq!(std::fs::read_dir(&todo).unwrap().count(), 1);
}

#[test]
fn clicking_away_adds_the_issue_and_closes_the_composer() {
    let dir = scratch_dir("compose-away");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    app.open_composer();
    app.composer.as_mut().unwrap().title = "Export board".to_string();
    app.submit_composer(true);

    let created = todo.join("export-board.md");
    assert!(created.exists(), "the new issue is written to Todo");
    assert!(app.composer.is_none(), "the composer closes");
    assert!(matches!(app.screen, Screen::Board), "the board stays");
    assert_eq!(app.selected.as_deref(), Some(created.as_path()));
}

/// The composer's keys, as typed: `⇧↩` and `⌘↩` are new lines, `↩` and
/// `⌘S` add, and a click on another card adds and closes it.
#[test]
fn composer_keys_and_clicking_away() {
    let dir = scratch_dir("compose-keys");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let other = todo.join("other.md");
    std::fs::write(&other, "# Other\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();

    let ctx = themed_ctx();
    let key = |key: Key, modifiers: egui::Modifiers| {
        vec![
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            },
        ]
    };
    let text = |text: &str| vec![egui::Event::Text(text.into())];
    let files = || std::fs::read_dir(&todo).unwrap().count();

    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, text("First line"));
    frame(&ctx, &mut app, key(Key::Enter, egui::Modifiers::SHIFT));
    frame(&ctx, &mut app, text("second"));
    frame(&ctx, &mut app, key(Key::Enter, egui::Modifiers::COMMAND));
    frame(&ctx, &mut app, text("third"));
    assert_eq!(
        app.composer.as_ref().unwrap().description,
        "First line\nsecond\nthird"
    );
    assert_eq!(files(), 1, "new lines never add");

    frame(&ctx, &mut app, key(Key::Enter, egui::Modifiers::NONE));
    assert_eq!(
        std::fs::read_to_string(todo.join("first-line.md")).unwrap(),
        "# First line\n\nFirst line\nsecond\nthird\n"
    );
    assert!(app.composer.is_some(), "↩ keeps it open for the next one");

    frame(
        &ctx,
        &mut app,
        vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
    );
    frame(&ctx, &mut app, text("Saved with command S"));
    frame(&ctx, &mut app, key(Key::S, egui::Modifiers::COMMAND));
    // Adding a ticket at the top renumbers Todo's files.
    assert!(todo.join("001-saved-with-command-s.md").exists(), "⌘S adds");
    assert!(app.composer.is_some());
    let other = todo.join("003-other.md");
    assert!(other.exists(), "the older card was renumbered too");

    frame(
        &ctx,
        &mut app,
        vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
    );
    frame(&ctx, &mut app, text("Clicked away"));
    let card = ctx
        .read_response(egui::Id::new(("issue_card", &other)))
        .unwrap()
        .rect
        .center();
    let button = |pressed| egui::Event::PointerButton {
        pos: card,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(card)]);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(card), button(true)],
    );
    frame(&ctx, &mut app, vec![button(false)]);
    assert!(
        todo.join("001-clicked-away.md").exists(),
        "the click adds it"
    );
    assert!(app.composer.is_none(), "and closes the composer");
    let opened = todo.join("004-other.md");
    let got = match &app.screen {
        Screen::Detail(path) => Some(path.clone()),
        _ => None,
    };
    assert_eq!(got, Some(opened), "the clicked card still opens");
}

/// A composer whose engine answers through `sender`, as the real ones do.
fn composer_with_engine(tag: &str) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    // A stand-in that answers at once: no test ever runs a real agent.
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        command: "true # {prompt}".to_string(),
        configured: true,
        ..AgentConfig::default()
    };
    app.open_composer();
    let composer = app.composer.as_mut().unwrap();
    composer.engine = Some(Engine::Agent);
    composer.description = "When I export the board I want a checklist.\nOne per column.".into();
    (dir, todo, app)
}

/// Stands in for the engine: the answer is already waiting.
fn answer_with(app: &mut IboardApp, answer: Result<String, String>) {
    let composer = app.composer.as_mut().unwrap();
    let run = composer.naming.as_mut().expect("naming started");
    run.answer = Job::done(answer);
}

#[test]
fn enter_names_the_issue_then_adds_it() {
    let (_dir, todo, mut app) = composer_with_engine("compose-name");
    let ctx = egui::Context::default();

    app.submit_composer(false);
    assert!(app.composer.as_ref().unwrap().stage == TitleStage::Naming);
    assert_eq!(
        std::fs::read_dir(&todo).unwrap().count(),
        0,
        "nothing written yet"
    );

    answer_with(&mut app, Ok("Export the board as checklists".into()));
    app.poll_naming(&ctx);
    let composer = app.composer.as_ref().unwrap();
    assert!(
        composer.stage == TitleStage::Inferred,
        "it stops on the suggestion"
    );
    assert_eq!(composer.title, "Export the board as checklists");
    assert!(composer.select_title, "selected, so typing replaces it");

    app.submit_composer(false);
    let created = todo.join("export-the-board-as-checklists.md");
    assert_eq!(
        std::fs::read_to_string(&created).unwrap(),
        "# Export the board as checklists\n\nWhen I export the board I want a checklist.\nOne per column.\n"
    );
    let composer = app.composer.as_ref().expect("the composer stays open");
    assert!(composer.title.is_empty() && composer.description.is_empty());
    assert!(composer.stage == TitleStage::Idle);
}

#[test]
fn a_failed_name_falls_back_to_the_first_line() {
    let (_dir, todo, mut app) = composer_with_engine("compose-fail");
    let ctx = egui::Context::default();

    app.submit_composer(false);
    answer_with(&mut app, Err("model unavailable".into()));
    app.poll_naming(&ctx);
    let composer = app.composer.as_ref().unwrap();
    assert!(composer.stage == TitleStage::Failed);
    assert!(
        composer.focus == Some(ComposerField::Title),
        "cursor in the title"
    );
    assert!(!composer.description.is_empty(), "nothing lost");

    app.submit_composer(false);
    assert!(
        todo.join("when-i-export-the-board-i-want-a-checklist.md")
            .exists(),
        "↩ uses the first line"
    );
}

#[test]
fn clicking_away_while_naming_writes_once_named() {
    let (_dir, todo, mut app) = composer_with_engine("compose-name-away");
    let ctx = egui::Context::default();

    app.submit_composer(false);
    app.submit_composer(true);
    answer_with(&mut app, Ok("Export checklists".into()));
    app.poll_naming(&ctx);
    let created = todo.join("export-checklists.md");
    assert!(
        created.exists(),
        "a click away never stops on the suggestion"
    );
    assert!(app.composer.is_none(), "and closes the composer");
}

#[test]
fn esc_while_naming_lets_you_type_the_title() {
    let (_dir, todo, mut app) = composer_with_engine("compose-skip");
    app.submit_composer(false);
    app.composer_escape();
    let composer = app
        .composer
        .as_ref()
        .expect("esc does not close while naming");
    assert!(composer.stage == TitleStage::Skipped);
    assert!(composer.naming.is_none());

    app.composer.as_mut().unwrap().title = "Board export".into();
    app.submit_composer(false);
    assert!(todo.join("board-export.md").exists());
    app.composer_escape();
    assert!(app.composer.is_none(), "esc closes it otherwise");
}

#[test]
fn without_an_engine_the_first_line_names_the_issue() {
    let dir = scratch_dir("compose-manual");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    assert!(
        app.composer.as_ref().unwrap().engine.is_none(),
        "Off in tests"
    );
    app.composer.as_mut().unwrap().description =
        "  Fix the login redirect\nIt loses the deep link.".into();
    app.submit_composer(false);
    assert_eq!(
        std::fs::read_to_string(todo.join("fix-the-login-redirect.md")).unwrap(),
        "# Fix the login redirect\n\n  Fix the login redirect\nIt loses the deep link.\n"
    );
}

#[test]
fn with_asking_off_delete_is_immediate_and_command_z_restores() {
    let dir = scratch_dir("delete-no-ask");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    let file = done.join("sidebar.md");
    std::fs::write(&file, "# Sidebar\n\nCollapse it.\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    assert!(app.confirm_delete, "asking is on by default");

    app.request_delete(file.clone());
    assert!(app.confirm.is_some(), "on: the dialog opens");
    assert!(file.exists());
    app.confirm = None;

    app.confirm_delete = false;
    app.request_delete(file.clone());
    assert!(app.confirm.is_none(), "off: no dialog");
    assert!(!file.exists(), "off: deleted at once");
    assert!(app.can_undo_delete());

    app.undo_delete();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Sidebar\n\nCollapse it.\n",
        "⌘Z puts it back as it was"
    );
    assert!(app.board.find(&file).is_some(), "and on the board");
    assert_eq!(app.selected.as_deref(), Some(file.as_path()));
    assert!(!app.can_undo_delete(), "once");
}

/// Bug: opening another issue dropped the composer and what was typed.
#[test]
fn opening_an_issue_keeps_an_unsaved_composer() {
    let dir = scratch_dir("compose-keep");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let other = todo.join("other.md");
    std::fs::write(&other, "# Other\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.open_composer();
    app.composer.as_mut().unwrap().description = "Half a thought".into();
    app.open_issue(other.clone());
    app.back_to_board();
    assert_eq!(
        app.composer.as_ref().map(|c| c.description.as_str()),
        Some("Half a thought"),
        "the draft waits on the board"
    );

    app.composer.as_mut().unwrap().description.clear();
    app.open_issue(other);
    assert!(app.composer.is_none(), "an empty composer still closes");
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
    app.auto = false;
    app.start_editing(&file);
    app.editor.as_mut().unwrap().text = "# Demo\n\nMore.\n".into();

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
        app.detail_mode,
        DetailMode::Preview,
        "like the Save ⌘S button, it shows the saved issue"
    );
}

#[test]
fn archive_all_clears_done() {
    let dir = scratch_dir("archive-all");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let a = done.join("a.md");
    let b = done.join("b.md");
    std::fs::write(&a, "# A\n").unwrap();
    std::fs::write(&b, "# B\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let before = app.board_generation;

    app.archive_all(&[a.clone(), b.clone()]);
    assert_eq!(app.board_generation, before + 1, "the board is read once");
    assert!(!a.exists() && !b.exists());
    assert!(dir.join("issues/.archive/a.md").exists());
    assert_eq!(
        app.notice.as_ref().map(|(text, _)| text.as_str()),
        Some("Archived 2 issues")
    );
}

#[test]
fn only_the_todo_column_offers_new_issues() {
    let dir = scratch_dir("new-columns");
    let todo = dir.join("issues/todo");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(&doing).unwrap();

    let located = discovery::locate(&dir).unwrap();
    let board = discovery::build(&located).unwrap();

    let find = |name: &str| {
        board
            .columns
            .iter()
            .find(|column| column.name == name)
            .unwrap()
    };
    assert!(find("Todo").accepts_new_issues());
    assert!(!find("Doing").accepts_new_issues());
}

#[test]
fn deleting_an_issue_asks_first_then_removes_the_file() {
    let dir = scratch_dir("delete");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
    app.selected = Some(file.clone());
    app.open_issue(file.clone());

    app.request_delete(file.clone());
    assert!(app.confirm.is_some(), "the delete waits for confirmation");
    assert!(file.exists(), "nothing is removed until confirmed");

    app.delete_issue(&file);
    assert!(!file.exists(), "the file is gone");
    assert!(app.confirm.is_none());
    assert!(app.selected.is_none());
    assert!(
        matches!(app.screen, Screen::Board),
        "deleting the open issue returns to the board"
    );
}

#[test]
fn archiving_a_done_issue_hides_it_in_the_dot_archive() {
    let dir = scratch_dir("archive");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    let file = done.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
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

    assert!(app.confirm.is_some(), "the icon asks to delete");
    assert!(
        matches!(app.screen, Screen::Board),
        "and does not open the issue"
    );
}

/// `⌘↩` in the settings dialog saves; a custom command without
/// `{prompt}` keeps it open, and `esc` there never cancels the review.
#[test]
fn the_settings_dialog_saves_with_command_enter() {
    let (_dir, _file, mut app) = open_issue_with_review("settings-save");
    let ctx = themed_ctx();
    let frame = |app: &mut IboardApp, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| app.draw(ui)).textures_delta.clear();
    };

    app.open_config();
    app.settings.as_mut().unwrap().form = AgentConfig {
        tool: agent::Tool::Custom,
        command: "llm".to_string(),
        ..AgentConfig::default()
    };
    frame(&mut app, vec![key(Key::Enter, egui::Modifiers::COMMAND)]);
    assert!(app.settings.is_some(), "an invalid command keeps it open");
    assert!(!app.agent.configured());

    app.settings.as_mut().unwrap().form.command = "llm < {prompt}".to_string();
    frame(&mut app, vec![key(Key::Enter, egui::Modifiers::COMMAND)]);
    assert!(app.settings.is_none(), "saved and closed");
    assert!(app.agent.configured());
    assert_eq!(app.agent.label(), "custom · default");

    app.open_config();
    frame(&mut app, vec![key(Key::Escape, egui::Modifiers::NONE)]);
    assert!(app.settings.is_none(), "esc closes the dialog");
    assert!(app.review.is_some(), "and leaves the review alone");
}

/// The same flow through egui_kittest: widgets found by their label, as a
/// screen reader sees them.
#[test]
fn settings_save_by_clicking_its_labelled_button() {
    let (_dir, _file, mut app) = open_issue_with_review("kittest-save");
    app.open_title_settings();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(window().size())
        .build_ui_state(
            |ui, (themed, app): &mut (bool, IboardApp)| {
                // Plex is registered on the first frame and usable from the next.
                if !*themed {
                    theme::install(ui.ctx());
                    *themed = true;
                    return;
                }
                app.frame_logic(ui.ctx());
                app.draw(ui);
            },
            (false, app),
        );
    harness.run_steps(2);
    assert!(harness.state().1.settings.is_some());

    egui_kittest::kittest::Queryable::get_by_label(&harness, "Save").click();
    harness.run_steps(2);

    assert!(harness.state().1.settings.is_none(), "Save closed Settings");
}

/// Save writes only the sections that changed, keeps the rest of the file
/// as written, and never writes over a file that does not parse.
#[test]
fn saving_settings_never_loses_the_users_file() {
    let (dir, _file, mut app) = open_issue_with_review("settings-file");
    let ctx = themed_ctx();
    let save = |app: &mut IboardApp| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events: vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| app.draw(ui)).textures_delta.clear();
    };
    let config = dir.join("iboard.toml");
    let mine = "# mine\n[delete]\nconfirm = true # keep asking\n";
    std::fs::write(&config, mine).unwrap();
    app.config_file = Some(config.clone());

    app.open_title_settings();
    save(&mut app);
    assert!(app.settings.is_none());
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        mine,
        "nothing changed"
    );

    app.open_title_settings();
    app.settings.as_mut().unwrap().engine = Engine::Agent;
    save(&mut app);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        format!("{mine}\n[titles]\nengine = \"agent\"\n")
    );

    let broken = "[titles\nengine = \"agent\"\n";
    std::fs::write(&config, broken).unwrap();
    app.open_title_settings();
    app.settings.as_mut().unwrap().engine = Engine::Manual;
    save(&mut app);
    let error = app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.save_error.clone());
    assert!(error.is_some_and(|error| error.contains("line 1")));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), broken);
}

/// The panel and the settings window go through a real egui pass, so a
/// panic or a layout mistake surfaces without a display.
#[test]
fn the_review_panel_and_config_window_render() {
    let (_dir, _file, mut app) = open_issue_with_review("review-render");
    let ctx = themed_ctx();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };

    for _ in 0..2 {
        ctx.run_ui(input.clone(), |ui| app.draw(ui))
            .textures_delta
            .clear();
    }

    app.open_config();
    for _ in 0..2 {
        ctx.run_ui(input.clone(), |ui| app.draw(ui))
            .textures_delta
            .clear();
    }
}

/// A project with a Todo column of `(title, label)` issues, and the app.
fn focus_app(tag: &str, cards: &[(&str, Option<&str>)]) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    for (index, (title, label)) in cards.iter().enumerate() {
        let body = match label {
            Some(label) => format!("# [{label}] {title}\n\n## Context\n\nWhy.\n"),
            None => format!("# {title}\n\n## Context\n\nWhy.\n\n- [ ] one\n- [x] two\n"),
        };
        std::fs::write(todo.join(format!("card-{index}.md")), body).unwrap();
    }
    let located = discovery::locate(&dir).unwrap();
    (dir, todo, IboardApp::from_located(located))
}

/// Renders `app` twice and clears the texture deltas, as the other tests do.
fn render(app: &mut IboardApp, ctx: &egui::Context, width: f32) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 800.0),
        )),
        ..Default::default()
    };
    for _ in 0..2 {
        ctx.run_ui(input.clone(), |ui| app.draw(ui))
            .textures_delta
            .clear();
    }
}

/// A focused Todo column lays the wide cards out four across at 1280pt,
/// with the fifth wrapping to a new row inside the gutters.
#[test]
fn column_focus_lays_wide_cards_in_a_grid() {
    let (_dir, todo, mut app) = focus_app(
        "focus-grid",
        &[
            ("One", None),
            ("Two", None),
            ("Three", None),
            ("Four", None),
            ("Five", None),
        ],
    );
    // Pin the file order so the grid is deterministic.
    app.column_sorts.insert(todo.clone(), Sort::Board);
    app.focus_column(todo.clone());
    assert_eq!(app.focused_column.as_deref(), Some(todo.as_path()));

    let ctx = themed_ctx();
    render(&mut app, &ctx, 1280.0);

    let paths: Vec<PathBuf> = app
        .board
        .columns
        .iter()
        .find(|column| column.dir == todo)
        .unwrap()
        .issues
        .iter()
        .map(|issue| issue.path.clone())
        .collect();
    let rects: Vec<Rect> = paths
        .iter()
        .map(|path| {
            ctx.read_response(egui::Id::new(("issue_card", path)))
                .expect("every card is drawn")
                .rect
        })
        .collect();

    assert!(
        rects[0].width() >= 295.0,
        "wide cards: {:?}",
        rects[0].width()
    );
    assert!(
        (rects[0].center().y - rects[3].center().y).abs() < 1.0,
        "four cards share the first row: {rects:?}"
    );
    assert!(
        rects[4].center().y > rects[0].center().y + 10.0,
        "the fifth wraps to a new row"
    );
    assert!(rects[1].min.x > rects[0].min.x, "cards run left to right");
    for rect in &rects {
        assert!(
            rect.min.x >= 16.0 - 0.5 && rect.max.x <= 1280.0 - 16.0 + 0.5,
            "{} at {rect:?} is outside the gutters",
            rect.center()
        );
    }
}

/// Opening an issue from a focused column and going back returns to that
/// column, not the whole board.
#[test]
fn opening_an_issue_from_focus_returns_to_the_focused_column() {
    let (_dir, todo, mut app) = focus_app("focus-open", &[("One", None), ("Two", None)]);
    app.focus_column(todo.clone());
    let file = app
        .board
        .columns
        .iter()
        .find(|column| column.dir == todo)
        .unwrap()
        .issues[0]
        .path
        .clone();
    app.open_issue(file.clone());
    assert!(matches!(app.screen, Screen::Detail(_)));
    app.back_to_board();
    assert!(matches!(app.screen, Screen::Board));
    assert_eq!(
        app.focused_column.as_deref(),
        Some(todo.as_path()),
        "the focused column is kept"
    );
    assert_eq!(app.selected.as_deref(), Some(file.as_path()));

    // The active column, and `esc`, leave ColumnFocus.
    app.unfocus_column();
    assert!(app.focused_column.is_none());
}

/// A focused column still accepts a card dropped on another tab, moving the
/// file between folders without leaving ColumnFocus.
#[test]
fn a_card_can_be_moved_to_another_column_from_the_tabs() {
    let dir = scratch_dir("focus-tab-drop");
    for name in ["todo", "doing"] {
        std::fs::create_dir_all(dir.join("issues").join(name)).unwrap();
    }
    let file = dir.join("issues/todo/card.md");
    std::fs::write(&file, "# Card\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let todo = dir.join("issues/todo");
    let doing = dir.join("issues/doing");
    app.focus_column(todo.clone());

    // Simulate the tab drop the focus bar reports.
    app.move_issue(&file, &doing);
    assert!(doing.join("card.md").exists(), "moved into doing/");
    assert!(!file.exists());
}

/// A tiny `design/` so the panel tests need no display and no repo data.
fn write_min_design(root: &Path) {
    let design = root.join("design");
    std::fs::create_dir_all(design.join("components/Button")).unwrap();
    std::fs::write(
        design.join("design-system.json"),
        r#"{ "v": 1, "title": "Fixture", "namespace": "fixture" }"#,
    )
    .unwrap();
    std::fs::write(
            design.join("tokens.json"),
            r##"{
                "name": "Fixture", "version": 1,
                "color": {
                    "themes": [ {"id":"light","name":"Light"}, {"id":"dark","name":"Dark"} ],
                    "tokens": [ {"name":"bg","value":{"light":"#ffffff","dark":"#000000"},"usage":"ground"} ]
                },
                "type": { "families": {"sans":"-apple-system"}, "groups": [] },
                "spacing": {"tokens":[]}, "radius": {"tokens":[]},
                "size": {"tokens":[]}, "shadow": {"tokens":[]}
            }"##,
        )
        .unwrap();
    std::fs::write(
        design.join("components/Button/README.md"),
        "# Button\n\nA button.\n",
    )
    .unwrap();
    std::fs::write(
        design.join("components/Button/preview.html"),
        "<!doctype html><html><head></head><body><button>Go</button></body></html>",
    )
    .unwrap();
}

fn panel_ctx() -> (egui::Context, egui::RawInput) {
    (
        themed_ctx(),
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0))),
            ..Default::default()
        },
    )
}

#[test]
fn design_panel_loads_and_creates_a_component() {
    let dir = scratch_dir("design-panel");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    write_min_design(&dir);
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.open_design();
    assert!(
        app.design_system.is_some(),
        "loaded: {:?}",
        app.design_error
    );
    assert!(app.design.is_some());

    let (ctx, raw) = panel_ctx();
    let mut output = ctx.run_ui(raw.clone(), |ui| app.draw(ui));
    output.textures_delta.clear();

    // The same path the Create button drives.
    let mut panel = app.design.take().expect("panel open");
    let keep = app.handle_design_action(
        &mut panel,
        design_ui::Action::Create {
            name: "Toast".to_string(),
            group: "Controls".to_string(),
            intent: "Transient confirmation.".to_string(),
            draft: false,
        },
    );
    assert!(keep);
    assert!(dir.join("design/components/Toast/README.md").is_file());
    assert!(
        app.design_system
            .as_ref()
            .unwrap()
            .components
            .iter()
            .any(|c| c.name == "Toast")
    );

    app.design = Some(panel);
    let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
    output.textures_delta.clear();
}

#[test]
fn design_panel_reports_a_missing_system() {
    let dir = scratch_dir("design-missing");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.open_design();
    assert!(app.design_system.is_none());
    assert!(app.design.is_some(), "the panel opens even with no system");

    let (ctx, raw) = panel_ctx();
    let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
    output.textures_delta.clear();
}

#[test]
fn preview_render_helper_composes_and_renders() {
    if !iboard::design::render::available() {
        return;
    }
    let design = Path::new(env!("CARGO_MANIFEST_DIR")).join("design");
    let system = DesignSystem::load(&design).expect("repo design");
    let render = design::render::component_preview(&system, "Button").expect("spawn render");
    match render.wait() {
        Some(Ok(path)) => assert!(path.is_file(), "PNG written"),
        Some(Err(err)) => panic!("render failed: {err}"),
        None => panic!("the render thread died"),
    }
}

#[test]
fn design_panel_drafts_a_component_with_the_agent() {
    let dir = scratch_dir("design-draft");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    write_min_design(&dir);
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_design();

    // A fake agent: prints a fenced HTML fragment, ignores the prompt.
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "printf '```html\\n<button class=\"ib-btn\">Hi</button>\\n```' # {prompt}"
            .to_string(),
        configured: true,
    };

    let mut panel = app.design.take().expect("panel open");
    let keep = app.handle_design_action(
        &mut panel,
        design_ui::Action::Create {
            name: "Toast".to_string(),
            group: "Controls".to_string(),
            intent: "Transient confirmation.".to_string(),
            draft: true,
        },
    );
    assert!(keep);
    app.design = Some(panel);
    assert!(app.design_draft.is_some(), "a draft was started");

    let (ctx, _raw) = panel_ctx();
    let preview = dir.join("design/components/Toast/preview.html");
    until(|| {
        app.poll_design_draft(&ctx);
        std::fs::read_to_string(&preview)
            .unwrap_or_default()
            .contains("ib-btn")
    });

    let written = std::fs::read_to_string(&preview).unwrap();
    assert!(
        written.contains("ib-btn"),
        "drafted fragment written: {written}"
    );
    assert!(!written.contains("```"), "code fences stripped: {written}");
    assert!(app.design_draft.is_none(), "the draft finished");
}

#[test]
fn enrichment_artifact_path_is_per_issue() {
    let path = Path::new("/p/issues/todo/01-demo.md");
    assert_eq!(
        enrichment_dir(path),
        Path::new("/p/issues/todo/assets/01-demo")
    );
}

#[test]
fn enrichment_drafts_an_artifact_for_an_issue() {
    let dir = scratch_dir("enrich");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\nMake the header taller.\n").unwrap();

    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    // A fake agent: prints a whole HTML page, ignores the prompt.
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "printf '<!doctype html><html><body><h1>Demo</h1></body></html>' # {prompt}"
            .to_string(),
        configured: true,
    };

    app.start_enrichment(&file);
    assert!(app.enrichment.is_some(), "an enrichment started");

    let (ctx, raw) = panel_ctx();
    let artifact = todo.join("assets/01-demo/ui.html");
    until(|| {
        app.poll_enrichment(&ctx);
        artifact.is_file()
    });

    let html = std::fs::read_to_string(&artifact).unwrap_or_else(|err| {
        let status = match app.enrichment.as_ref().map(|e| &e.status) {
            Some(EnrichStatus::Failed(why)) => format!("failed: {why}"),
            Some(EnrichStatus::Running(_)) => "still running".to_string(),
            Some(EnrichStatus::Rendering(_)) => "rendering".to_string(),
            Some(EnrichStatus::Done) => "done".to_string(),
            None => "gone".to_string(),
        };
        panic!("no artifact ({err}); the enrichment is {status}")
    });
    assert!(html.contains("<h1>Demo</h1>"), "mockup saved: {html}");

    // The window renders without a display, loader and all.
    let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
    output.textures_delta.clear();
}

#[test]
fn enrichment_without_an_agent_opens_the_settings_instead() {
    let dir = scratch_dir("enrich-noagent");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.start_enrichment(&file);
    assert!(app.enrichment.is_none(), "nothing started");
    assert!(app.settings.is_some(), "the settings dialog opened");
}

#[test]
fn image_links_resolve_against_the_issue_folder() {
    let issue = Path::new("/p/issues/todo/01-demo.md");
    assert_eq!(image_base_uri(issue), "file:///p/issues/todo/");
}

#[test]
fn a_ticket_can_show_an_image_from_its_asset_folder() {
    let dir = scratch_dir("image");
    let todo = dir.join("issues/todo");
    let assets = todo.join("assets/01-demo");
    std::fs::create_dir_all(&assets).unwrap();
    let png = assets.join("ui.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 40, 40, 255]))
        .save(&png)
        .unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\n![mockup](assets/01-demo/ui.png)\n").unwrap();

    // The reader renders the markdown's image link without a display.
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_issue(file.clone());
    let (ctx, raw) = panel_ctx();
    let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
    output.textures_delta.clear();

    // And the loader resolves and decodes it.
    let uri = format!("file://{}", png.display());
    let mut decoded = false;
    for _ in 0..100 {
        match ctx.try_load_image(&uri, egui::load::SizeHint::default()) {
            Ok(egui::load::ImagePoll::Ready { .. }) => {
                decoded = true;
                break;
            }
            Ok(egui::load::ImagePoll::Pending { .. }) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(err) => panic!("decode failed: {err:?}"),
        }
    }
    assert!(
        decoded,
        "the ticket's image decodes through the file loader"
    );
}

#[test]
fn auto_rescans_off_the_ui_thread_and_never_over_a_newer_board() {
    let dir = scratch_dir("rescan");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::write(todo.join("a.md"), "# A\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.auto = true;
    let ctx = egui::Context::default();

    std::fs::write(todo.join("b.md"), "# B\n").unwrap();
    app.last_refresh = Instant::now() - RESCAN_EVERY;
    app.background(&ctx);
    assert!(app.scan.is_some(), "the rescan runs in the background");
    assert!(until(|| {
        app.background(&ctx);
        app.scan.is_none()
    }));
    assert!(app.board.find(&todo.join("b.md")).is_some());

    app.last_refresh = Instant::now() - RESCAN_EVERY;
    app.background(&ctx);
    app.refresh();
    let newer = app.board_generation;
    assert!(until(|| {
        app.background(&ctx);
        app.scan.is_none()
    }));
    assert_eq!(app.board_generation, newer, "the older scan was dropped");
}
