//! The Reader and Editor header, driven by label: what each column offers,
//! the AI split button and its menu, the `…` menu, and the title joining the
//! breadcrumb.

use super::*;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type Head = Harness<'static, (bool, IboardApp)>;

/// A board with one long issue per column, named after it; the agent is a
/// fake that never starts.
fn board(tag: &str) -> (Scratch, IboardApp) {
    let dir = scratch_dir(tag);
    let body: String = (0..120).map(|n| format!("Line {n}.\n\n")).collect();
    for (column, title) in [
        ("todo", "Todo title"),
        ("ready", "Ready title"),
        ("doing", "Doing title"),
        ("done", "Done title"),
    ] {
        let folder = dir.join("issues").join(column);
        std::fs::create_dir_all(&folder).unwrap();
        let text = format!("# [ux] {title}\n\n{body}");
        std::fs::write(folder.join(format!("{column}.md")), text).unwrap();
    }
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.prefs.agent.configured = true;
    (dir, app)
}

fn issue(dir: &Path, column: &str) -> PathBuf {
    dir.join("issues").join(column).join(format!("{column}.md"))
}

fn opened(dir: &Path, app: IboardApp, column: &str) -> Head {
    let mut app = app;
    app.open_issue(issue(dir, column));
    let mut harness = kittest(app);
    harness.run_steps(2);
    harness
}

fn has(harness: &Head, label: &str) -> bool {
    harness.query_all_by_label(label).next().is_some()
}

fn click(harness: &mut Head, label: &str) {
    harness.get_by_label(label).click();
    harness.run_steps(2);
}

/// Nothing is drawn or clickable in the leading 80pt, on the board and on an
/// issue.
#[test]
fn nothing_sits_under_the_traffic_lights() {
    let (dir, app) = board("header-lead");
    let lead = if UNIFIED_TITLEBAR {
        theme::TITLEBAR_LEAD
    } else {
        theme::SPACE_5
    };
    let harness = kittest(app);
    assert!(harness.get_by_label("Switch project").rect().min.x >= lead);
    let app = harness.into_state().1;
    let harness = opened(&dir, app, "todo");
    assert!(harness.get_by_label("Board").rect().min.x >= lead);
}

/// Refine and Review are Todo's; Ready, Doing and Done have no AI split
/// button, and Done's `…` comes right after Commits.
#[test]
fn the_ai_split_button_follows_the_column() {
    let (dir, app) = board("header-ai");
    let mut harness = opened(&dir, app, "todo");
    harness.get_by_label("Refine");
    click(&mut harness, "AI actions");
    for row in [
        "Refine with a challenger",
        "Review this issue",
        "Deep review",
        "Ask…",
    ] {
        assert!(has(&harness, row), "Todo offers {row}");
    }
    assert!(!has(&harness, "Update from the code"));
    let mut app = harness.into_state().1;

    for column in ["ready", "doing"] {
        let harness = opened(&dir, app, column);
        assert!(!has(&harness, "AI actions"), "{column}");
        assert!(!has(&harness, "Refine"), "{column}");
        app = harness.into_state().1;
    }

    let harness = opened(&dir, app, "done");
    assert!(!has(&harness, "AI actions"));
    let commits = harness.get_by_label("Commits").rect();
    let more = harness.get_by_label("More actions").rect();
    assert!(commits.max.x <= more.min.x);
}

/// Every AI menu ends with the tool and the model, which opens Settings.
#[test]
fn the_ai_menu_ends_with_the_tool_and_opens_settings() {
    let (dir, mut app) = board("header-ai-settings");
    app.prefs.agent = fake_agent("sleep 30 # {prompt}");
    let label = app.prefs.agent.label();
    let mut harness = opened(&dir, app, "todo");
    click(&mut harness, "AI actions");
    click(&mut harness, &label);
    assert!(harness.state().1.settings.is_some(), "Settings open");
}

/// `Ask…` opens the dialog.
#[test]
fn ask_opens_from_the_ai_menu() {
    let (dir, app) = board("header-ask");
    let mut harness = opened(&dir, app, "todo");
    click(&mut harness, "AI actions");
    click(&mut harness, "Ask…");
    assert!(harness.state().1.session.ask_draft.is_some());
}

/// Edit in Todo and Ready; Add label too; Delete but on Done; the Editor keeps
/// Delete only and Save, with no `…`.
#[test]
fn the_dynamic_buttons_follow_the_column_and_the_mode() {
    let (dir, app) = board("header-dynamic");
    let cases = [
        ("todo", true, true),
        ("ready", true, true),
        ("doing", false, true),
        ("done", false, false),
    ];
    let mut app = app;
    for (column, edits, deletes) in cases {
        let harness = opened(&dir, app, column);
        assert_eq!(has(&harness, "Edit"), edits, "{column}");
        assert_eq!(has(&harness, "Add label"), edits, "{column}");
        assert_eq!(has(&harness, "Delete"), deletes, "{column}");
        assert_eq!(has(&harness, "Commits"), column == "done", "{column}");
        let edit = harness.query_by_label("Edit").map(|node| node.rect());
        if let Some(edit) = edit {
            assert_eq!(edit.width(), 24.0, "an icon at rest");
        }
        app = harness.into_state().1;
    }

    let todo = issue(&dir, "todo");
    app.open_issue(todo.clone());
    app.start_editing(&todo);
    let mut harness = kittest(app);
    harness.run_steps(2);
    harness.get_by_label("Save");
    harness.get_by_label("Delete");
    assert!(!has(&harness, "Edit"));
    assert!(!has(&harness, "Add label"));
    assert!(!has(&harness, "More actions"));
}

/// `esc` closes the menu and stays on the issue.
#[test]
fn esc_closes_a_menu_before_leaving_the_issue() {
    let (dir, app) = board("header-esc");
    let mut harness = opened(&dir, app, "done");
    click(&mut harness, "More actions");
    assert!(has(&harness, "Archive"));
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(!has(&harness, "Archive"));
    assert!(matches!(
        harness.state().1.session.screen,
        Screen::Detail(..)
    ));
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(matches!(harness.state().1.session.screen, Screen::Board));
}

/// On a Done issue the `…` menu is where it gets archived.
#[test]
fn the_more_menu_archives_a_done_issue() {
    let (dir, app) = board("header-archive");
    let mut harness = opened(&dir, app, "done");
    click(&mut harness, "More actions");
    click(&mut harness, "Archive");
    assert!(!issue(&dir, "done").exists());
}

/// The title joins the breadcrumb, without its label, once the page's own has
/// scrolled away; the Editor always shows it.
#[test]
fn the_title_joins_the_breadcrumb_once_the_page_has_scrolled() {
    let (dir, app) = board("header-title");
    let mut harness = opened(&dir, app, "todo");
    let count = |harness: &Head| harness.query_all_by_label("Todo title").count();
    assert_eq!(count(&harness), 1, "the page's own");
    let path = issue(&dir, "todo");
    assert_eq!(harness.state().1.reader.title_away, None);

    let centre = harness.ctx.content_rect().center();
    harness.event(egui::Event::PointerMoved(centre));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::Vec2::new(0.0, -600.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
        source: egui::MouseWheelSource::Wheel,
    });
    harness.run_steps(6);
    assert_eq!(harness.state().1.reader.title_away, Some(path.clone()));
    assert!(has(&harness, "Todo title"));

    let mut app = harness.into_state().1;
    app.start_editing(&path);
    let mut harness = kittest(app);
    harness.run_steps(2);
    let crumb = harness.get_by_label("Todo title").rect();
    assert!(crumb.max.y <= theme::BAR_H, "in the header: {crumb:?}");
}
