//! ColumnFocus's tests: the grid, leaving it, and drops on its tabs.

use super::*;

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
    app.column_focus.sorts.insert(todo.clone(), Sort::Board);
    app.focus_column(todo.clone());
    assert_eq!(app.column_focus.column.as_deref(), Some(todo.as_path()));

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
    assert!(matches!(app.session.screen, Screen::Detail(_)));
    app.back_to_board();
    assert!(matches!(app.session.screen, Screen::Board));
    assert_eq!(
        app.column_focus.column.as_deref(),
        Some(todo.as_path()),
        "the focused column is kept"
    );
    assert_eq!(app.session.selected.as_deref(), Some(file.as_path()));

    // The active column, and `esc`, leave ColumnFocus.
    app.unfocus_column();
    assert!(app.column_focus.column.is_none());
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
