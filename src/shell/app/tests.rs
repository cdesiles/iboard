//! The board window's tests, driven through real egui frames.

use super::*;

mod agent_log;
mod ask;
mod bars;
mod board_look;
mod build;
mod comments;
mod compose;
mod design_panel;
mod diff;
mod drag;
mod editor;
mod enrich;
mod filter_look;
mod groups;
mod handover;
mod header;
mod images;
mod keys;
mod kit;
mod labelling;
mod labels;
mod modals;
mod moves;
mod naming;
mod panes;
mod plan;
mod project_menu;
mod question;
mod quitting;
mod reading;
mod refine;
mod release;
mod review;
mod select;
mod settings;
mod splitting;
mod stats;
mod unsaved;
mod welcome;
mod workspace;
use crate::shell::app::filter_bar::filter_id;
use kit::{Kit, PASSING, fake_agent, kittest, kittest_with, landed, press, press_nth, project};
use review::open_issue_with_review;

/// A context with the design system installed, past the first frame so
/// the bundled font families are loaded.
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
#[must_use]
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
/// fails. It reads as the folder's canonical `Path`.
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
    let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let dir = tempfile::Builder::new()
        .prefix(&format!("iboard-{tag}-"))
        .tempdir_in(temp)
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

/// One frame of `app` that only draws, and the shapes it painted.
fn painted(
    ctx: &egui::Context,
    app: &mut IboardApp,
    events: Vec<egui::Event>,
) -> (&'static Palette, Vec<egui::Shape>) {
    let input = egui::RawInput {
        screen_rect: Some(window()),
        events,
        ..Default::default()
    };
    let mut palette = None;
    let mut output = ctx.run_ui(input, |ui| {
        palette = Some(theme::palette(ui));
        app.draw(ui);
    });
    output.textures_delta.clear();
    let shapes = output.shapes.into_iter().map(|clipped| clipped.shape);
    (palette.unwrap(), shapes.collect())
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

/// `⌘` held down and `key` pressed with it.
fn cmd(key: Key) -> Vec<egui::Event> {
    let command = egui::Modifiers::COMMAND;
    vec![
        egui::Event::ModifiersChanged(command),
        self::key(key, command),
    ]
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

fn panel_ctx() -> (egui::Context, egui::RawInput) {
    (
        themed_ctx(),
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0))),
            ..Default::default()
        },
    )
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

    app.prefs.auto = false;
    app.start_editing(&file);
    app.session.editor.as_mut().unwrap().text.push_str("more\n");
    app.show_preview();
    draw(&ctx, &mut app, vec![]); // editor asking before leaving
}

/// A long error keeps its Dismiss button in the window.
#[test]
fn a_long_error_can_still_be_dismissed() {
    use egui_kittest::kittest::Queryable;
    let dir = scratch_dir("long-error");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.error = Some("direnv: error /a/very/long/path/.envrc is blocked. ".repeat(12));
    let mut harness = kittest(app);
    let dismiss = harness.get_by_label("Dismiss").rect();
    assert!(dismiss.max.x <= window().width(), "{dismiss:?}");
    harness.get_by_label("Dismiss").click();
    harness.run_steps(2);
    assert!(harness.state().1.error.is_none());
}

/// Opening a folder with no issues directory shows a one-line banner at the
/// top, and its button creates the directory and opens the board.
#[test]
fn a_missing_issues_directory_is_a_banner_that_creates_it() {
    use egui_kittest::kittest::Queryable;
    let dir = scratch_dir("banner");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let empty = scratch_dir("banner-empty");
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_project(empty.to_path_buf());
    let mut harness = kittest(app);
    let create = harness.get_by_label("Create issues/").rect();
    assert!(create.max.y < theme::BAR_H + 60.0, "{create:?}");
    harness.get_by_label("Create issues/").click();
    harness.run_steps(2);
    let app = &harness.state().1;
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(empty.join("issues/todo/.gitkeep").is_file());
    assert_eq!(
        recents::canonical(&app.board.root),
        recents::canonical(&empty.join("issues"))
    );
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
        .read_response(egui::Id::unique(("issue_card", &file)))
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
        .read_response(egui::Id::unique(("card_delete", &file)))
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
    for font in [theme::meta_mono(), theme::ui_button()] {
        for glyph in ['⌘', '↩'] {
            assert!(
                ctx.fonts_mut(|f| f.has_glyph(&font, glyph)),
                "{glyph} is missing from {font:?}"
            );
        }
    }
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
            .read_response(egui::Id::unique(("issue_card", card)))
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
fn auto_rescans_off_the_ui_thread_and_never_over_a_newer_board() {
    let dir = scratch_dir("rescan");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::write(todo.join("a.md"), "# A\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.prefs.auto = true;
    let ctx = egui::Context::default();

    std::fs::write(todo.join("b.md"), "# B\n").unwrap();
    app.last_refresh = Instant::now() - RESCAN_EVERY;
    // A fast scan may land within the call that started it, so only its
    // outcome is checked.
    app.rescan(&ctx);
    assert!(until(|| {
        app.rescan(&ctx);
        app.session.scan.is_none()
    }));
    assert!(app.board.find(&todo.join("b.md")).is_some());

    app.last_refresh = Instant::now() - RESCAN_EVERY;
    app.rescan(&ctx);
    app.refresh();
    let newer = app.board_generation;
    assert!(until(|| {
        app.rescan(&ctx);
        app.session.scan.is_none()
    }));
    assert_eq!(app.board_generation, newer, "the older scan was dropped");
}

/// Two fingers swiping right go back one level, as in a browser: the reader
/// to the board. Scrolling down never goes back.
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
        source: egui::MouseWheelSource::Wheel,
    };

    app.open_issue(file.clone());
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![wheel(0.0, -60.0)]);
    }
    assert!(
        matches!(app.session.screen, Screen::Detail(..)),
        "scrolling stays"
    );

    for _ in 0..3 {
        frame(&ctx, &mut app, vec![wheel(50.0, 2.0)]);
    }
    assert!(
        matches!(app.session.screen, Screen::Board),
        "back to the board"
    );
}
