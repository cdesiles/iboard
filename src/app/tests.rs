//! The board window's tests, driven through real egui frames.

use super::nav::step_selection;
use super::*;

mod build;
mod compose;
mod design_panel;
mod diff;
mod drag;
mod editor;
mod enrich;
mod focus;
mod images;
mod refine;
mod review;
mod select;
mod settings;
mod stats;
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
    agent::run_tools_from(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fakes"
    )));
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

    app.session.screen = Screen::Board;
    app.open_composer();
    app.session.composer.as_mut().unwrap().title = "Export board".to_string();
    draw(&ctx, &mut app, vec![]); // board with the quick-add composer open

    app.session.composer = None;
    app.move_selection(Key::ArrowDown);
    draw(&ctx, &mut app, vec![]); // board with a selected card

    app.auto = false;
    app.start_editing(&file);
    app.session.editor.as_mut().unwrap().text.push_str("more\n");
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
    assert_eq!(app.session.detail_mode, DetailMode::Edit);
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
    app.session.undo = Some(Undo::Deleted(vec![Deleted {
        path: a.clone(),
        trashed: None,
        content: Vec::new(),
        images: None,
        at: Instant::now(),
    }]));
    app.session.notice = Some(("Archived a.md".to_string(), Instant::now()));
    app.session.pending_front = Some((alpha.join("issues/todo"), a.clone()));
    app.session.design_error = Some("alpha has no design".to_string());
    app.column_focus.column = Some(alpha.join("issues/todo"));

    app.open_project(beta.to_path_buf());

    assert!(
        app.session.undo.is_none(),
        "⌘Z cannot restore into the old project"
    );
    assert!(app.session.notice.is_none() && app.session.pending_front.is_none());
    assert!(app.session.design_error.is_none() && app.session.design_system.is_none());
    assert!(app.column_focus.column.is_none());
    assert!(app.error.is_none());
    assert_eq!(
        recents::canonical(&app.board.root),
        recents::canonical(&beta.join("issues")),
        "the board now belongs to the target project"
    );
    assert!(matches!(app.session.screen, Screen::Board));
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

/// The board's header names the project, then its issues path.
#[test]
fn the_board_header_names_the_project_before_the_path() {
    use egui_kittest::kittest::Queryable;
    let dir = scratch_dir("project-name");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let mut harness = egui_kittest::Harness::builder()
        .with_size(window().size())
        .build_ui_state(
            |ui, (themed, app): &mut (bool, IboardApp)| {
                if !*themed {
                    theme::install(ui.ctx());
                    *themed = true;
                    return;
                }
                app.draw(ui);
            },
            (false, app),
        );
    harness.run_steps(2);
    let name = harness.state().1.board.name();
    let path = display_path(&harness.state().1.board.root);
    let name = harness.get_by_label(&name).rect();
    let path = harness.get_by_label(&path).rect();
    assert!(name.max.x <= path.min.x, "{name:?} then {path:?}");
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
    app.filter_bar.focus = true;
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    assert!(focused(&ctx), "the filter takes focus");

    frame(
        &ctx,
        &mut app,
        vec![key(Key::ArrowDown, egui::Modifiers::NONE)],
    );
    assert_eq!(
        app.filter_bar.chip_cursor,
        Some(0),
        "↓ reaches the first chip"
    );
    frame(&ctx, &mut app, vec![key(Key::Enter, egui::Modifiers::NONE)]);
    assert_eq!(app.filter_bar.filter.tokens, vec!["bug"]);
    assert!(focused(&ctx), "picking keeps focus in the field");
    assert_eq!(app.visible_grid().concat().len(), 1, "only the bug matches");

    frame(
        &ctx,
        &mut app,
        vec![key(Key::Escape, egui::Modifiers::NONE)],
    );
    assert_eq!(
        app.filter_bar.chip_cursor, None,
        "esc goes back from the chips to the text"
    );
    assert!(focused(&ctx), "leaving the chips keeps focus");
    frame(
        &ctx,
        &mut app,
        vec![key(Key::Backspace, egui::Modifiers::NONE)],
    );
    assert!(
        app.filter_bar.filter.tokens.is_empty(),
        "⌫ in an empty field drops the token"
    );

    app.filter_bar.filter.pick("feature");
    frame(
        &ctx,
        &mut app,
        vec![key(Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.filter_bar.filter.is_empty(), "the first esc clears");
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
    app.session.selected = Some(login.clone());

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
    let add = egui::Id::new(("card_add_label", &wip));
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

/// A press and release without movement is still a click, not a drag.
#[test]
fn a_click_selects_a_card_and_expand_opens_it() {
    let dir = scratch_dir("click");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let ctx = themed_ctx();
    let click = |app: &mut IboardApp, at: egui::Pos2| {
        draw(&ctx, app, vec![moved(at)]);
        draw(&ctx, app, vec![moved(at), button(at, true)]);
        draw(&ctx, app, vec![button(at, false)]);
    };

    draw(&ctx, &mut app, vec![]);
    let card = ctx
        .read_response(egui::Id::new(("issue_card", &file)))
        .unwrap()
        .rect;
    click(&mut app, card.center());
    assert!(matches!(app.session.screen, Screen::Board), "nothing opens");
    assert_eq!(app.session.selected.as_deref(), Some(file.as_path()));

    let expand = ctx
        .read_response(egui::Id::new(("card_expand", &file)))
        .expect("the hovered card offers Expand")
        .rect;
    assert!(
        expand.right() <= card.right() && (expand.top() - card.top()).abs() < 10.0,
        "Expand sits in the top right corner"
    );
    click(&mut app, expand.center());
    assert!(
        matches!(app.session.screen, Screen::Detail(path) if path == file),
        "Expand opens the issue"
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
    assert_eq!(app.session.selected, Some(todo.join("01-alpha.md")));
    app.move_selection(Key::ArrowDown);
    app.move_selection(Key::ArrowRight);
    assert_eq!(app.session.selected, Some(doing.join("01-gamma.md")));

    // Filtered out cards are skipped.
    app.filter_bar.filter.text = "beta".to_string();
    app.session.selected = None;
    app.move_selection(Key::ArrowDown);
    assert_eq!(app.session.selected, Some(todo.join("02-beta.md")));

    // Back from the reader, the card is still selected.
    app.open_issue(todo.join("02-beta.md"));
    app.back_to_board();
    assert_eq!(app.session.selected, Some(todo.join("02-beta.md")));
}

#[test]
fn with_asking_off_delete_is_immediate_and_command_z_restores() {
    let dir = scratch_dir("delete-no-ask");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&doing).unwrap();
    let file = doing.join("sidebar.md");
    std::fs::write(&file, "# Sidebar\n\nCollapse it.\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    assert!(app.confirm_delete, "asking is on by default");

    app.request_delete(file.clone());
    assert!(app.session.confirm.is_some(), "on: the dialog opens");
    assert!(file.exists());
    app.session.confirm = None;

    app.confirm_delete = false;
    app.request_delete(file.clone());
    assert!(app.session.confirm.is_none(), "off: no dialog");
    assert!(!file.exists(), "off: deleted at once");
    assert!(app.can_undo());

    app.undo_last();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Sidebar\n\nCollapse it.\n",
        "⌘Z puts it back as it was"
    );
    assert!(app.board.find(&file).is_some(), "and on the board");
    assert_eq!(app.session.selected.as_deref(), Some(file.as_path()));
    assert!(!app.can_undo(), "once");
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
        app.session.notice.as_ref().map(|(text, _)| text.as_str()),
        Some("Archived 2 issues")
    );
}

/// Done's count gives way to "Archive all" while its header is hovered.
#[test]
fn hovering_the_done_header_offers_archive_all() {
    let dir = scratch_dir("archive-all-head");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let a = done.join("a.md");
    std::fs::write(&a, "# A\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let index = app
        .board
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("done"))
        .unwrap();
    let archive_all = egui::Id::new(("archive_all", index));

    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    assert!(ctx.read_response(archive_all).is_none(), "hidden at rest");
    let column = ctx
        .read_response(egui::Id::new(("column_drop", index)))
        .unwrap()
        .rect;
    let head = egui::pos2(column.center().x, column.min.y + theme::CONTROL_H / 2.0);
    draw(&ctx, &mut app, vec![moved(head)]);
    let at = ctx
        .read_response(archive_all)
        .expect("shown on hover")
        .rect
        .center();
    let expand = ctx
        .read_response(egui::Id::new(("expand_column", index)))
        .expect("expand shown on hover")
        .rect;
    assert!(expand.min.x >= at.x, "expand sits right of archive");
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(!a.exists() && dir.join("issues/.archive/a.md").exists());
}

/// Todo's `+` is a column action: shown on hover, left of expand.
#[test]
fn todo_offers_new_issue_on_hover_left_of_expand() {
    let dir = scratch_dir("plus-head");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let index = app
        .board
        .columns
        .iter()
        .position(|c| c.accepts_new_issues())
        .unwrap();
    let plus = egui::Id::new(("new_issue", index));

    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    assert!(ctx.read_response(plus).is_none(), "hidden at rest");
    let column = ctx
        .read_response(egui::Id::new(("column_drop", index)))
        .unwrap()
        .rect;
    let head = egui::pos2(column.center().x, column.min.y + theme::CONTROL_H / 2.0);
    draw(&ctx, &mut app, vec![moved(head)]);
    let at = ctx.read_response(plus).expect("shown on hover").rect;
    let expand = ctx
        .read_response(egui::Id::new(("expand_column", index)))
        .unwrap()
        .rect;
    assert!(expand.min.x >= at.max.x, "+ sits left of expand");
    let at = at.center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(app.session.composer.is_some(), "+ opens the composer");
}

/// A hovered header offers expand at its right, which opens ColumnFocus.
#[test]
fn hovering_a_header_offers_expand() {
    let dir = scratch_dir("expand-head");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&doing).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let index = app
        .board
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("doing"))
        .unwrap();
    let expand = egui::Id::new(("expand_column", index));

    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    assert!(ctx.read_response(expand).is_none(), "hidden at rest");
    let column = ctx
        .read_response(egui::Id::new(("column_drop", index)))
        .unwrap()
        .rect;
    let head = egui::pos2(column.center().x, column.min.y + theme::CONTROL_H / 2.0);
    draw(&ctx, &mut app, vec![moved(head)]);
    let at = ctx.read_response(expand).expect("shown on hover").rect;
    assert!(at.max.x > column.center().x, "on the right of the header");
    let at = at.center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert_eq!(app.column_focus.column.as_deref(), Some(doing.as_path()));
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
    // A fast scan may land within the call that started it, so only its
    // outcome is checked.
    app.background(&ctx);
    assert!(until(|| {
        app.background(&ctx);
        app.session.scan.is_none()
    }));
    assert!(app.board.find(&todo.join("b.md")).is_some());

    app.last_refresh = Instant::now() - RESCAN_EVERY;
    app.background(&ctx);
    app.refresh();
    let newer = app.board_generation;
    assert!(until(|| {
        app.background(&ctx);
        app.session.scan.is_none()
    }));
    assert_eq!(app.board_generation, newer, "the older scan was dropped");
}

/// Two fingers swiping right go back one level, as in a browser: the reader
/// to the board, then ColumnFocus to the whole board. Its momentum never
/// goes back twice, and scrolling down never goes back.
#[test]
fn a_two_finger_swipe_right_goes_back() {
    let dir = scratch_dir("swipe");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("a.md");
    std::fs::write(&file, "# A\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let ctx = themed_ctx();
    let wheel = |x: f32, y: f32| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(x, y),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    };

    app.focus_column(todo.clone());
    app.open_issue(file.clone());
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![wheel(0.0, -60.0)]);
    }
    assert!(
        matches!(app.session.screen, Screen::Detail(_)),
        "scrolling stays"
    );

    for _ in 0..3 {
        frame(&ctx, &mut app, vec![wheel(50.0, 2.0)]);
    }
    assert!(
        matches!(app.session.screen, Screen::Board),
        "back to the board"
    );
    assert!(app.column_focus.column.is_some(), "in its focused column");
    for _ in 0..5 {
        frame(&ctx, &mut app, vec![wheel(50.0, 0.0)]);
    }
    assert!(
        app.column_focus.column.is_some(),
        "the momentum goes back once"
    );

    frame(&ctx, &mut app, vec![]);
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![wheel(50.0, 0.0)]);
    }
    assert!(
        app.column_focus.column.is_none(),
        "a new swipe leaves ColumnFocus"
    );
}
