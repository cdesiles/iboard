//! Keys and guards agree with what the labels promise: `↩` and the scrolling
//! keys on the Reader's page, `⌘⇧R` on a Todo card, `⌘U` and `⌘K` in the Editor,
//! `⌘R` by Auto, and the double-click that only edits from the sheet itself.

use egui::Modifiers;
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use super::kit::{Kit, fake_agent, press};
use super::reading::{double_click, kit};
use super::*;
use crate::app::review::RunKind;

const SLOW: &str = "sleep 30 # {prompt}";

/// An issue long enough to scroll, open in `column`.
fn opened_in(tag: &str, column: &str) -> (Scratch, PathBuf, Kit) {
    let dir = scratch_dir(tag);
    for name in ["todo", "ready", "doing", "done"] {
        std::fs::create_dir_all(dir.join("issues").join(name)).unwrap();
    }
    let file = dir.join("issues").join(column).join("01-demo.md");
    let body: String = (0..200).map(|n| format!("Line {n}.\n\n")).collect();
    std::fs::write(&file, format!("# Demo\n\n{body}")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.prefs.agent = fake_agent(SLOW);
    app.open_issue(file.clone());
    let mut harness = kit(app);
    harness.run_steps(2);
    (dir, file, harness)
}

fn hit(harness: &mut Kit, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    harness.run_steps(2);
}

fn page_top(harness: &Kit) -> f32 {
    harness.state().1.reader.preview_rect.top()
}

fn editing(harness: &Kit) -> bool {
    harness.state().1.session.editing().is_some()
}

#[test]
fn return_edits_in_the_reader_where_e_does() {
    for (column, edits) in [
        ("todo", true),
        ("ready", true),
        ("doing", false),
        ("done", false),
    ] {
        let (_dir, _file, mut harness) = opened_in("reader-return", column);
        hit(&mut harness, Modifiers::NONE, Key::Enter);
        assert_eq!(editing(&harness), edits, "{column}");
    }
}

#[test]
fn return_waits_while_a_menu_is_open() {
    let (_dir, _file, mut harness) = opened_in("reader-return-menu", "todo");
    press(&mut harness, "AI actions");
    hit(&mut harness, Modifiers::NONE, Key::Enter);
    assert!(!editing(&harness));
}

#[test]
fn the_keys_scroll_the_readers_page() {
    let (_dir, _file, mut harness) = opened_in("reader-scroll", "doing");
    let top = page_top(&harness);
    hit(&mut harness, Modifiers::NONE, Key::ArrowDown);
    let line = top - page_top(&harness);
    assert!(line > 0.0, "↓ scrolls");
    hit(&mut harness, Modifiers::NONE, Key::ArrowUp);
    assert_eq!(page_top(&harness), top, "↑ comes back");

    hit(&mut harness, Modifiers::NONE, Key::Space);
    let page = top - page_top(&harness);
    assert!(page > 5.0 * line, "space turns a page: {page}");
    hit(&mut harness, Modifiers::SHIFT, Key::Space);
    assert_eq!(page_top(&harness), top, "⇧space turns it back");

    hit(&mut harness, Modifiers::NONE, Key::PageDown);
    assert_eq!(top - page_top(&harness), page);
    hit(&mut harness, Modifiers::NONE, Key::PageUp);
    assert_eq!(page_top(&harness), top);
}

#[test]
fn the_keys_leave_the_page_to_a_menu_or_a_dialog() {
    let (_dir, file, mut harness) = opened_in("reader-scroll-guard", "doing");
    let top = page_top(&harness);
    press(&mut harness, "More actions");
    hit(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert_eq!(page_top(&harness), top, "the menu has the arrows");
    hit(&mut harness, Modifiers::NONE, Key::Escape);

    harness.state_mut().1.session.confirm = Some(Confirm::Delete(vec![file]));
    harness.run_steps(2);
    hit(&mut harness, Modifiers::NONE, Key::Space);
    assert_eq!(page_top(&harness), top, "the dialog is on top");
}

#[test]
fn the_editors_text_keeps_the_arrows() {
    let (_dir, file, mut harness) = opened_in("editor-arrows", "todo");
    harness.state_mut().1.start_editing(&file);
    harness.run_steps(2);
    hit(&mut harness, Modifiers::NONE, Key::ArrowDown);
    assert!(editing(&harness));
    assert!(harness.state().1.reader.scroll.is_none());
}

fn board_with(tag: &str, column: &str) -> (Scratch, PathBuf, Kit) {
    let dir = scratch_dir(tag);
    for name in ["todo", "ready", "doing", "done"] {
        std::fs::create_dir_all(dir.join("issues").join(name)).unwrap();
    }
    let file = dir.join("issues").join(column).join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.prefs.agent = fake_agent(SLOW);
    app.session.selected = Some(file.clone());
    (dir, file, kit(app))
}

#[test]
fn command_shift_r_reviews_the_selected_todo_card() {
    let (_dir, file, mut harness) = board_with("card-review-key", "todo");
    let keys = Modifiers::COMMAND | Modifiers::SHIFT;
    hit(&mut harness, keys, Key::R);
    assert!(harness.state().1.review_is_running(&file));
    assert!(matches!(harness.state().1.session.screen, Screen::Board));
    hit(&mut harness, keys, Key::R);
    assert!(
        !harness.state().1.review_is_running(&file),
        "and cancels it"
    );
}

#[test]
fn command_shift_r_leaves_other_cards_and_a_dialog_alone() {
    let keys = Modifiers::COMMAND | Modifiers::SHIFT;
    let (_dir, file, mut harness) = board_with("ready-review-key", "ready");
    hit(&mut harness, keys, Key::R);
    assert!(!harness.state().1.review_is_running(&file));

    let (_dir, file, mut harness) = board_with("confirm-review-key", "todo");
    harness.state_mut().1.session.confirm = Some(Confirm::Delete(vec![file.clone()]));
    harness.run_steps(2);
    hit(&mut harness, keys, Key::R);
    assert!(!harness.state().1.review_is_running(&file));
}

#[test]
fn command_u_and_k_work_in_the_editor_as_its_menu_does() {
    let (_dir, file, mut harness) = opened_in("editor-ai-keys", "ready");
    harness.state_mut().1.edit_at_line(&file, None);
    harness.run_steps(3);
    assert!(
        harness.ctx.memory(|m| m.focused().is_some()),
        "the text has the keyboard"
    );
    hit(&mut harness, Modifiers::COMMAND, Key::U);
    assert!(harness.state().1.review_is_running(&file));
    hit(&mut harness, Modifiers::COMMAND, Key::U);
    assert!(!harness.state().1.review_is_running(&file));

    hit(&mut harness, Modifiers::COMMAND, Key::K);
    assert!(harness.state().1.session.ask_draft.is_some());
    assert!(editing(&harness), "the draft stays behind the dialog");
}

/// `⌘R` reloads only with Auto off: with Auto on the board reads itself.
#[test]
fn command_r_refreshes_only_with_auto_off() {
    for column in ["ready", "todo"] {
        let (_dir, file, mut harness) = board_with("refresh-key", column);
        let late = file.with_file_name("02-late.md");
        std::fs::write(&late, "# Late\n").unwrap();
        harness.state_mut().1.prefs.auto = true;
        harness.state_mut().1.last_refresh = Instant::now();
        hit(&mut harness, Modifiers::COMMAND, Key::R);
        assert!(harness.state().1.board.find(&late).is_none(), "Auto on");

        harness.state_mut().1.prefs.auto = false;
        hit(&mut harness, Modifiers::COMMAND, Key::R);
        assert!(harness.state().1.board.find(&late).is_some(), "Auto off");
    }
}

#[test]
fn command_r_still_refines_a_todo_issue_with_auto_on() {
    let (_dir, file, mut harness) = opened_in("refine-key", "todo");
    harness.state_mut().1.prefs.auto = true;
    hit(&mut harness, Modifiers::COMMAND, Key::R);
    let refine = harness.state().1.session.refine.as_ref();
    assert_eq!(refine.map(|refine| refine.path.clone()), Some(file));
}

#[test]
fn a_double_click_on_a_dialog_does_not_edit_behind_it() {
    let (_dir, file, mut harness) = opened_in("double-confirm", "todo");
    harness.state_mut().1.session.confirm = Some(Confirm::Delete(vec![file]));
    harness.run_steps(2);
    harness.get_by_role_and_label(Role::AlertDialog, "Delete this issue?");
    let dialog = harness.state().1.reader.sheet_rect.center();
    let over = harness.ctx.layer_id_at(dialog).expect("a layer is there");
    assert_eq!(over.order, egui::Order::Foreground);
    double_click(&mut harness, dialog);
    assert!(!editing(&harness));
    assert!(harness.state().1.session.confirm.is_some());
}

#[test]
fn a_double_click_on_the_label_picker_does_not_edit_behind_it() {
    let (_dir, file, mut harness) = opened_in("double-picker", "todo");
    harness.state_mut().1.toggle_label_picker(file);
    harness.run_steps(3);
    let area = egui::Id::unique("label_picker");
    let picker = harness.ctx.memory(|m| m.area_rect(area)).unwrap();
    let sheet = harness.state().1.reader.sheet_rect;
    let over = picker.intersect(sheet);
    assert!(over.is_positive(), "{picker:?} over {sheet:?}");
    double_click(&mut harness, over.center());
    assert!(!editing(&harness));
}

/// The first click closes the menu, so the second is alone: a double-click
/// only counts when both are on the uncovered sheet.
#[test]
fn the_click_that_closes_a_menu_is_not_half_a_double_click() {
    let (_dir, _file, mut harness) = opened_in("double-menu", "todo");
    press(&mut harness, "AI actions");
    let at = harness.state().1.reader.sheet_rect.center();
    double_click(&mut harness, at);
    assert!(!editing(&harness));

    harness.run_steps(40);
    double_click(&mut harness, at);
    assert!(editing(&harness), "once the sheet is bare, it edits");
}

/// A long page runs on under the bars, and only its shown part edits.
#[test]
fn a_double_click_on_the_bars_over_a_long_page_does_not_edit() {
    let (_dir, _file, mut harness) = opened_in("double-bars", "todo");
    hit(&mut harness, Modifiers::NONE, Key::Space);
    let page = harness.state().1.reader.preview_rect;
    let x = page.center().x;
    let bars = [
        egui::Pos2::new(x, theme::BAR_H / 2.0),
        egui::Pos2::new(x, window().max.y - theme::STATUSBAR_H / 2.0),
    ];
    for bar in bars {
        assert!(page.contains(bar), "{bar:?} in {page:?}");
        double_click(&mut harness, bar);
        assert!(!editing(&harness), "{bar:?}");
        harness.run_steps(40);
    }
}

/// The outline fades in over `motion-hover` and goes at once.
#[test]
fn the_edit_outline_fades_in() {
    let ctx = themed_ctx();
    let dir = scratch_dir("outline-fade");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\nText.\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_issue(file);
    draw(&ctx, &mut app, vec![]);
    let outline = app.reader.preview_rect;

    let edge = |shapes: &[egui::Shape]| {
        shapes.iter().find_map(|shape| match shape {
            egui::Shape::Rect(rect) if rect.rect == outline && rect.stroke.width == 1.0 => {
                Some(rect.stroke.color)
            }
            _ => None,
        })
    };
    let (_, rest) = painted(&ctx, &mut app, vec![]);
    assert_eq!(edge(&rest), None, "no outline at rest");

    let (_, first) = painted(&ctx, &mut app, vec![moved(outline.center())]);
    let (p, second) = painted(&ctx, &mut app, vec![]);
    let rising = edge(&first).or(edge(&second)).expect("already fading in");
    assert_ne!(rising, p.line_strong, "still fading in");
    let mut last = Vec::new();
    for _ in 0..20 {
        (_, last) = painted(&ctx, &mut app, vec![]);
    }
    assert_eq!(edge(&last), Some(p.line_strong));

    let away = egui::Pos2::new(1.0, 1.0);
    painted(&ctx, &mut app, vec![moved(away)]);
    let (_, gone) = painted(&ctx, &mut app, vec![]);
    assert_eq!(edge(&gone), None, "out at once");
}

type Modal = fn(&mut IboardApp, &Path);

fn delete_dialog(app: &mut IboardApp, file: &Path) {
    app.session.confirm = Some(Confirm::Delete(vec![file.to_path_buf()]));
}

fn quit_dialog(app: &mut IboardApp, _: &Path) {
    app.session.confirm = Some(Confirm::Quit(1));
}

/// Only the keys of the modal up may act: nothing starts, opens or leaves
/// behind any of them.
#[test]
fn the_command_keys_wait_behind_a_modal() {
    let modals: [(&str, Modal); 7] = [
        ("delete", delete_dialog),
        ("quit", quit_dialog),
        ("commits", |app, file| app.open_commits(file)),
        ("settings", |app, _| {
            app.open_settings(settings_ui::Tab::Review);
        }),
        ("stats", |app, _| app.open_stats()),
        ("ask", |app, file| app.toggle_ask(file)),
        ("menu", |app, _| {
            app.project_menu = Some(crate::project_menu::Menu::new(&app.menu_entries()));
        }),
    ];
    let command = Modifiers::COMMAND;
    for (name, up) in modals {
        let (_dir, file, mut harness) = opened_in("modal-keys", "todo");
        up(&mut harness.state_mut().1, &file);
        harness.run_steps(2);
        for (modifiers, key) in [
            (command, Key::U),
            (command | Modifiers::SHIFT, Key::R),
            (command, Key::R),
            (command, Key::OpenBracket),
        ] {
            hit(&mut harness, modifiers, key);
        }
        let app = &harness.state().1;
        assert!(!app.review_is_running(&file), "{name}: a run started");
        assert!(app.session.refine.is_none(), "{name}: a refinement started");
        assert_eq!(app.session.reading(), Some(file.as_path()), "{name}: left");
    }
    for (name, up) in [("delete", delete_dialog as Modal), ("quit", quit_dialog)] {
        let (_dir, file, mut harness) = opened_in("dialog-keys", "todo");
        up(&mut harness.state_mut().1, &file);
        harness.run_steps(2);
        for key in [Key::N, Key::K, Key::Comma] {
            hit(&mut harness, command, key);
        }
        let app = &harness.state().1;
        assert!(!app.composer_open(), "{name}: ⌘N");
        assert!(app.session.ask_draft.is_none(), "{name}: ⌘K");
        assert!(app.settings.is_none(), "{name}: ⌘,");
        assert!(app.session.confirm.is_some(), "{name}: still asking");
    }
}

#[test]
fn the_menu_bar_picks_wait_behind_a_modal() {
    let modals: [(&str, Modal); 6] = [
        ("delete", delete_dialog),
        ("quit", quit_dialog),
        ("commits", |app, file| app.open_commits(file)),
        ("settings", |app, _| {
            app.open_settings(settings_ui::Tab::Review);
        }),
        ("ask", |app, file| app.toggle_ask(file)),
        ("new issue", |app, _| app.open_composer()),
    ];
    for (name, up) in modals {
        let (_dir, file, mut harness) = opened_in("menu-picks", "todo");
        let app = &mut harness.state_mut().1;
        up(app, &file);
        let had_settings = app.settings.is_some();
        app.menu_picks(true, true, false);
        assert!(app.session.stats.is_none(), "{name}: stats opened");
        assert_eq!(app.settings.is_some(), had_settings, "{name}: settings");
    }
    let (_dir, _file, mut harness) = opened_in("menu-picks-free", "todo");
    let app = &mut harness.state_mut().1;
    app.menu_picks(false, true, false);
    assert!(app.session.stats.is_some(), "stats");
    app.menu_picks(true, false, false);
    assert!(app.settings.is_some(), "settings over the stats screen");
}

#[test]
fn the_board_keys_wait_behind_a_dialog() {
    let (_dir, file, mut harness) = board_with("board-dialog-keys", "todo");
    delete_dialog(&mut harness.state_mut().1, &file);
    harness.run_steps(2);
    for key in [Key::I, Key::U, Key::N, Key::Comma] {
        hit(&mut harness, Modifiers::COMMAND, key);
    }
    let app = &harness.state().1;
    assert!(app.session.stats.is_none(), "⌘I");
    assert!(!app.review_is_running(&file), "⌘U");
    assert!(!app.composer_open(), "⌘N");
    assert!(app.settings.is_none(), "⌘,");
}

/// The quit dialog comes up over whatever is open, and `esc` is its own.
#[test]
fn the_quit_dialog_keeps_the_keys_from_the_modal_under_it() {
    let (_dir, _file, mut harness) = opened_in("quit-over-settings", "todo");
    let app = &mut harness.state_mut().1;
    app.open_settings(settings_ui::Tab::Review);
    quit_dialog(app, Path::new(""));
    harness.run_steps(2);
    hit(&mut harness, Modifiers::COMMAND, Key::Comma);
    assert!(harness.state().1.settings.is_some(), "⌘,");
    hit(&mut harness, Modifiers::NONE, Key::Escape);
    let app = &harness.state().1;
    assert!(app.session.confirm.is_none(), "esc answers the dialog");
    assert!(app.settings.is_some(), "and only the dialog");

    let (_dir, _file, mut harness) = opened_in("quit-over-stats", "todo");
    let app = &mut harness.state_mut().1;
    app.open_stats();
    quit_dialog(app, Path::new(""));
    harness.run_steps(2);
    hit(&mut harness, Modifiers::NONE, Key::Escape);
    let app = &harness.state().1;
    assert!(app.session.confirm.is_none() && app.session.stats.is_some());
}

/// `esc` closes the Commits dialog, not the screen behind it too.
#[test]
fn esc_closes_the_commits_dialog_and_nothing_else() {
    let (_dir, file, mut harness) = opened_in("commits-esc", "todo");
    harness.state_mut().1.open_commits(&file);
    harness.run_steps(2);
    hit(&mut harness, Modifiers::NONE, Key::Escape);
    let app = &harness.state().1;
    assert!(app.session.commits.is_none());
    assert_eq!(app.session.reading(), Some(file.as_path()));
}

/// The Refine modal keeps `esc` and `⌘↩`: nothing else reaches the page, and
/// a review, which would end the conversation, does not start behind it.
#[test]
fn the_command_keys_wait_behind_the_refine_modal() {
    let (_dir, file, mut harness) = opened_in("refine-modal-keys", "todo");
    harness.state_mut().1.start_refinement(&file);
    harness.run_steps(2);
    let command = Modifiers::COMMAND;
    for (modifiers, key) in [
        (command | Modifiers::SHIFT, Key::R),
        (command, Key::U),
        (command, Key::N),
        (command, Key::OpenBracket),
    ] {
        hit(&mut harness, modifiers, key);
    }
    let app = &harness.state().1;
    assert!(app.refinement_shown(), "the conversation stays");
    assert!(!app.review_is_running(&file));
    assert!(!app.composer_open());
    hit(&mut harness, Modifiers::NONE, Key::Escape);
    assert!(app_refine_stopped_or_hidden(&harness), "esc is its own");
}

/// `esc` stops the turn on its way first, then hides the panel.
fn app_refine_stopped_or_hidden(harness: &Kit) -> bool {
    let app = &harness.state().1;
    !app.refinement_shown() || app.session.refine.as_ref().is_some_and(|r| !r.is_running())
}

fn with_run(column: &str, kind: RunKind) -> (Scratch, PathBuf, Kit) {
    let (dir, file, mut harness) = opened_in("run-keys", column);
    let app = &mut harness.state_mut().1;
    app.start_update(&file);
    app.session.review_runs.running.get_mut(&file).unwrap().kind = kind;
    harness.run_steps(2);
    (dir, file, harness)
}

/// A key acts on its own job: `⌘U` on Update, `⌘⇧R` on Review.
#[test]
fn command_u_and_command_shift_r_leave_each_others_run_alone() {
    let (command, review) = (Modifiers::COMMAND, Modifiers::COMMAND | Modifiers::SHIFT);
    for (modifiers, key, other, own) in [
        (command, Key::U, RunKind::Review, RunKind::Update),
        (review, Key::R, RunKind::Update, RunKind::Review),
    ] {
        let (_dir, file, mut harness) = with_run("todo", other);
        hit(&mut harness, modifiers, key);
        let running = &harness.state().1.session.review_runs.running;
        assert_eq!(running.get(&file).map(|run| run.kind), Some(other));

        let (_dir, file, mut harness) = with_run("todo", own);
        hit(&mut harness, modifiers, key);
        assert!(!harness.state().1.review_is_running(&file), "{own:?}");
    }
}

/// The menu's Update row does what the key does while a Review runs: nothing.
#[test]
fn the_update_row_leaves_a_running_review_alone() {
    let (_dir, file, mut harness) = with_run("todo", RunKind::Review);
    press(&mut harness, "AI actions");
    press(&mut harness, "Update from the code");
    assert!(harness.state().1.review_is_running(&file));
}

/// A Review that went on to Ready shows on the Update face, which says it
/// cancels it, and does: the key stays Update's.
#[test]
fn the_face_of_a_running_review_still_cancels_it_on_ready() {
    let (_dir, file, mut harness) = with_run("ready", RunKind::Review);
    hit(&mut harness, Modifiers::COMMAND, Key::U);
    assert!(harness.state().1.review_is_running(&file));
    press(&mut harness, "Reviewing…");
    assert!(!harness.state().1.review_is_running(&file));
}

/// Two clicks a frame apart, as a hand makes them.
fn two_clicks(harness: &mut Kit, at: egui::Pos2) {
    harness.event(moved(at));
    harness.step();
    for _ in 0..2 {
        harness.event(button(at, true));
        harness.event(button(at, false));
        harness.step();
    }
    harness.run_steps(2);
}

const REST_HINT: &str = "Double-click below the text to keep writing";

/// The rest of the sheet, below the text: the first click closes the menu, so
/// the pair is no double-click; on the bare page it edits, at the end.
#[test]
fn the_click_that_closes_a_menu_does_not_edit_from_the_rest_area() {
    let (_dir, file, mut harness) = board_with("double-rest", "todo");
    harness.state_mut().1.open_issue(file);
    harness.run_steps(2);
    let at = harness.get_by_label(REST_HINT).rect().center();

    press(&mut harness, "AI actions");
    harness.run_steps(40);
    two_clicks(&mut harness, at);
    assert!(!editing(&harness), "the first click closed the menu");

    harness.run_steps(40);
    two_clicks(&mut harness, at);
    assert!(editing(&harness), "once the page is bare, it edits");
}

/// `⌘↩` applies a proposal only while its panel shows beside the issue: on
/// the board, where the panel is hidden, it applies nothing.
#[test]
fn cmd_enter_applies_only_a_review_on_screen() {
    let (_dir, file, mut app) = super::review::open_issue_with_review("apply-on-screen");
    assert!(app.proposal_shown());
    app.back_to_board();
    assert!(app.session.review.is_some());
    assert!(!app.proposal_shown(), "the panel is hidden on the board");

    let ctx = themed_ctx();
    let events = vec![
        egui::Event::ModifiersChanged(Modifiers::COMMAND),
        key(Key::Enter, Modifiers::COMMAND),
    ];
    frame(&ctx, &mut app, events);
    assert!(app.session.editor.is_none(), "nothing opens the editor");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Demo\n\nContext.\n"
    );
}
