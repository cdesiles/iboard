//! The panes beside an issue: how they share the window, in which order tab
//! goes through them, and what stays of their split after a restart.

use super::*;

fn gutter_of(ctx: &egui::Context) -> egui::Rect {
    ctx.read_response(theme::pane_split_id("review"))
        .expect("the divider is on screen")
        .rect
}

fn long_issue() -> String {
    format!("# Demo\n\n{}", "A paragraph.\n\n".repeat(200))
}

fn settle(ctx: &egui::Context, app: &mut IboardApp) {
    for _ in 0..4 {
        draw(ctx, app, vec![]);
    }
}

/// Scrolls the pane on the left of the window by a good way.
fn wheel(ctx: &egui::Context, app: &mut IboardApp) {
    theme::set_reduced_motion(ctx, true);
    let at = egui::pos2(400.0, 400.0);
    draw(ctx, app, vec![moved(at)]);
    let event = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -300.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    };
    draw(ctx, app, vec![event]);
    for _ in 0..40 {
        draw(ctx, app, vec![]);
    }
}

fn sized(events: Vec<egui::Event>, width: f32) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 800.0),
        )),
        events,
        ..Default::default()
    }
}

#[test]
fn a_window_too_narrow_for_both_panes_stacks_them_without_a_divider() {
    let (_dir, _file, mut app) = open_issue_with_review("panes-stack");
    let ctx = themed_ctx();
    let divider = theme::pane_split_id("review");
    let frames = |ctx: &egui::Context, app: &mut IboardApp, width: f32| {
        for _ in 0..3 {
            ctx.run_ui(sized(vec![], width), |ui| app.draw(ui))
                .textures_delta
                .clear();
        }
    };
    frames(&ctx, &mut app, 1280.0);
    assert!(ctx.read_response(divider).is_some(), "side by side");
    let ctx = themed_ctx();
    frames(&ctx, &mut app, 600.0);
    assert!(ctx.read_response(divider).is_none(), "stacked");
}

#[test]
fn tab_reaches_the_divider_after_the_issue_and_before_the_panel() {
    let (_dir, _file, mut app) = open_issue_with_review("panes-tab");
    let ctx = themed_ctx();
    for _ in 0..3 {
        draw(&ctx, &mut app, vec![]);
    }
    let divider = theme::pane_split_id("review");
    let gutter = gutter_of(&ctx);
    let mut order = Vec::new();
    for _ in 0..60 {
        draw(&ctx, &mut app, vec![key(Key::Tab, egui::Modifiers::NONE)]);
        let Some(focused) = ctx.memory(egui::Memory::focused) else {
            continue;
        };
        if order.contains(&focused) {
            break;
        }
        order.push(focused);
    }
    let at = order
        .iter()
        .position(|id| *id == divider)
        .expect("tab reaches the divider");
    let side = |id: &egui::Id| {
        let rect = ctx.read_response(*id).unwrap().rect;
        rect.center().x > gutter.center().x && gutter.y_range().contains(rect.center().y)
    };
    assert!(!order[..at].iter().any(side), "only the issue before it");
    assert!(order[at + 1..].iter().any(side), "the panel has a stop");
}

#[test]
fn the_split_survives_a_restart() {
    let (dir, _file, mut app) = open_issue_with_review("panes-restart");
    let file = dir.join("iboard.toml");
    app.config_file = Some(file.clone());
    let ctx = themed_ctx();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![]);
    }
    let divider = theme::pane_split_id("review");
    ctx.memory_mut(|m| m.request_focus(divider));
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![]);
    }
    frame(
        &ctx,
        &mut app,
        vec![key(Key::ArrowLeft, egui::Modifiers::NONE)],
    );
    frame(&ctx, &mut app, vec![]);
    assert!(
        ctx.memory(|m| m.has_focus(divider)),
        "← stays on the divider"
    );
    let width = reader::REVIEW_PANEL_WIDTH + 16.0;
    assert_eq!(theme::pane_split(&ctx, divider, 0.0), width);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!("\n[splits]\nreview = {width}\n")
            .trim_start()
            .to_string()
    );

    let mut config = UserConfig::load(Some(&file));
    let restarted = themed_ctx();
    app.prefs = Preferences::load(&mut config);
    app.restore_splits(&restarted);
    assert_eq!(theme::pane_split(&restarted, divider, 0.0), width);
}

#[test]
fn a_drag_is_saved_only_once_the_mouse_lets_go() {
    let (dir, _file, mut app) = open_issue_with_review("panes-drag");
    let file = dir.join("iboard.toml");
    app.config_file = Some(file.clone());
    let ctx = themed_ctx();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![]);
    }
    let start = gutter_of(&ctx).center();
    let end = start - egui::vec2(100.0, 0.0);
    frame(&ctx, &mut app, vec![moved(start)]);
    frame(&ctx, &mut app, vec![button(start, true)]);
    frame(&ctx, &mut app, vec![moved(end)]);
    frame(&ctx, &mut app, vec![moved(end)]);
    assert!(!file.exists(), "not yet");
    frame(&ctx, &mut app, vec![button(end, false)]);
    frame(&ctx, &mut app, vec![]);
    assert!(
        std::fs::read_to_string(&file).unwrap().contains("[splits]"),
        "saved"
    );
    let width = theme::pane_split(&ctx, theme::pane_split_id("review"), 0.0);
    assert!(
        width > reader::REVIEW_PANEL_WIDTH + 50.0,
        "dragged: {width}"
    );
}

#[test]
fn opening_the_side_panel_keeps_the_issues_scroll_offset() {
    let (_dir, file, mut app) = open_issue_with_review("panes-offset");
    std::fs::write(&file, long_issue()).unwrap();
    app.refresh();
    let ctx = themed_ctx();
    let review = app.session.review.take();
    settle(&ctx, &mut app);
    let top = app.reader.preview_rect.top();
    wheel(&ctx, &mut app);
    let scrolled = app.reader.preview_rect.top();
    assert!(scrolled < top, "the wheel scrolled it");
    app.session.review = review;
    settle(&ctx, &mut app);
    assert_eq!(app.reader.preview_rect.top(), scrolled);
}

#[test]
fn the_editor_keeps_a_scroll_offset_per_issue() {
    let dir = scratch_dir("panes-editor");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let (first, second) = (todo.join("01-first.md"), todo.join("02-second.md"));
    std::fs::write(&first, long_issue()).unwrap();
    std::fs::write(&second, long_issue()).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let ctx = themed_ctx();
    let text_top = |ctx: &egui::Context| {
        ctx.read_response(egui::Id::new(crate::app::editor::EDITOR_ID))
            .expect("the editor is on screen")
            .rect
            .top()
    };

    app.start_editing(&first);
    settle(&ctx, &mut app);
    let top = text_top(&ctx);
    wheel(&ctx, &mut app);
    let scrolled = text_top(&ctx);
    assert!(scrolled < top, "the wheel scrolled it");

    app.start_editing(&second);
    settle(&ctx, &mut app);
    assert_eq!(text_top(&ctx), top, "the other issue starts at its top");

    app.start_editing(&first);
    settle(&ctx, &mut app);
    assert_eq!(text_top(&ctx), scrolled, "back where it was");
}

/// The divider sits between the panes for tab, but the right pane is laid
/// out after it, so it must not take the gutter's right half.
pub(super) fn drags_from_right_half(ctx: &egui::Context, app: &mut IboardApp, name: &str) {
    let id = theme::pane_split_id(name);
    for _ in 0..4 {
        draw(ctx, app, vec![]);
    }
    let gutter = ctx.read_response(id).expect("a divider").rect;
    let before = theme::pane_split(ctx, id, 0.0);
    let start = gutter.center() + egui::vec2(gutter.width() / 2.0 - 1.0, 0.0);
    let end = start + egui::vec2(60.0, 0.0);
    draw(ctx, app, vec![moved(start)]);
    draw(ctx, app, vec![button(start, true)]);
    draw(ctx, app, vec![moved(end)]);
    draw(ctx, app, vec![moved(end)]);
    draw(ctx, app, vec![button(end, false)]);
    let after = theme::pane_split(ctx, id, 0.0);
    assert_ne!(after, before, "{name}: the drag moved it");
}

#[test]
fn the_new_issue_divider_takes_the_whole_gutter() {
    let dir = scratch_dir("panes-compose");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    drags_from_right_half(&themed_ctx(), &mut app, "new-issue");
}
