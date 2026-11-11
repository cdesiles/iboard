//! The Modal spec across the app's dialogs: their role, focus, what they ask
//! before dropping typed text, and what a scrim click does.

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use super::*;

fn asking(tag: &str) -> (Scratch, PathBuf, IboardApp) {
    let (dir, file, mut app) = open_issue_with_review(tag);
    app.session.review = None;
    app.toggle_ask(&file);
    (dir, file, app)
}

#[test]
fn modals_report_their_role_and_title() {
    let (_dir, file, mut app) = open_issue_with_review("modal-roles");
    app.open_settings(settings_ui::Tab::General);
    let mut harness = kittest(app);
    harness.get_by_role_and_label(Role::Dialog, "Settings");
    harness.get_by_label("Close (esc)");

    harness.state_mut().1.settings = None;
    harness.state_mut().1.session.confirm = Some(Confirm::Delete(vec![file]));
    harness.run_steps(2);
    harness.get_by_role_and_label(Role::AlertDialog, "Delete this issue?");
}

/// Ask AI has a head and a foot, starts in its field, and asks before
/// dropping what was typed; an empty one closes at once.
#[test]
fn ask_ai_starts_in_its_field_and_asks_before_dropping_typed_text() {
    let (_dir, _file, app) = asking("modal-ask");
    let mut harness = kittest(app);
    harness.run_steps(2);
    assert!(
        harness.get_by_role(Role::MultilineTextInput).is_focused(),
        "the first field has focus"
    );
    harness.get_by_role_and_label(Role::Dialog, "Ask the agent");
    harness.get_by_label("Close (esc)");

    let draft = |harness: &mut Kit| harness.state_mut().1.session.ask_draft.is_some();
    harness
        .state_mut()
        .1
        .session
        .ask_draft
        .as_mut()
        .unwrap()
        .request = "Take care of point 1".to_string();
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(draft(&mut harness), "esc asks first");
    press(&mut harness, "Keep editing");
    assert!(harness.query_by_label("Keep editing").is_none());
    assert!(draft(&mut harness));

    press(&mut harness, "Cancel");
    assert!(draft(&mut harness), "Cancel asks too");
    press(&mut harness, "Discard");
    assert!(!draft(&mut harness), "Discard drops it");

    let (_dir, file, _) = asking("modal-ask-empty");
    harness.state_mut().1.toggle_ask(&file);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(!draft(&mut harness), "nothing typed, nothing to ask");
}

/// Settings fits each tab, and asks before dropping an edit. An untouched
/// one closes at once, every tab shown.
#[test]
fn settings_fits_its_tab_and_asks_before_dropping_edits() {
    let (_dir, _file, mut app) = open_issue_with_review("modal-settings");
    app.open_settings(settings_ui::Tab::General);
    let mut harness = kittest(app);
    harness.run_steps(2);
    let height = |harness: &Kit| {
        let id = egui::Id::unique("review-settings");
        harness.ctx.memory(|m| m.area_rect(id)).unwrap().height()
    };
    let general = height(&harness);
    assert!(general < 560.0, "General fits its content: {general}");
    press(&mut harness, "Labels");
    harness.run_steps(2);
    assert_ne!(height(&harness), general, "each tab has its own height");
    press(&mut harness, "AI review");
    press(&mut harness, "General");
    harness.run_steps(12);
    press(&mut harness, "Cancel");
    assert!(harness.state().1.settings.is_none(), "nothing to lose");

    harness
        .state_mut()
        .1
        .open_settings(settings_ui::Tab::General);
    harness.run_steps(2);
    press(&mut harness, "Dark");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(harness.state().1.settings.is_some(), "esc asks first");
    harness.get_by_label("Keep editing");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(harness.query_by_label("Keep editing").is_none());
    press(&mut harness, "Close (esc)");
    assert!(harness.state().1.settings.is_some(), "the × asks too");
    press(&mut harness, "Discard");
    assert!(harness.state().1.settings.is_none());
}

/// The scrim closes a Modal S, where nothing can be lost, and no other.
#[test]
fn a_scrim_click_closes_only_a_small_modal() {
    let (_dir, file, mut app) = open_issue_with_review("modal-scrim");
    app.open_commits(&file);
    let ctx = themed_ctx();
    let corner = egui::pos2(4.0, 4.0);
    let click = |ctx: &egui::Context, app: &mut IboardApp| {
        frame(ctx, app, vec![moved(corner)]);
        frame(ctx, app, vec![button(corner, true)]);
        frame(ctx, app, vec![button(corner, false)]);
    };
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    let width = ctx
        .memory(|m| m.area_rect(egui::Id::unique("commits")))
        .unwrap()
        .width();
    assert_eq!(width, theme::MODAL_M + 2.0, "a token's width and its edge");
    click(&ctx, &mut app);
    assert!(app.session.commits.is_some(), "Commits ignores the scrim");

    app.session.commits = None;
    app.session.confirm = Some(Confirm::Delete(vec![file]));
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    click(&ctx, &mut app);
    assert!(app.session.confirm.is_none(), "the S closes");
}

/// The danger button starts focused; a Cancel tabbed to takes `↩` itself, and
/// nothing is deleted. Focus goes back to what held it when the modal opened.
#[test]
fn enter_on_a_focused_cancel_cancels_and_focus_goes_back() {
    let (_dir, file, mut app) = open_issue_with_review("modal-focus");
    app.session.review = None;
    let ctx = themed_ctx();
    frame(&ctx, &mut app, vec![]);
    let opener = egui::Id::unique("the-opener");
    ctx.memory_mut(|m| m.request_focus(opener));
    app.session.confirm = Some(Confirm::Delete(vec![file.clone()]));
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![]);
    }
    let danger = ctx.memory(|m| m.focused()).expect("a button has focus");
    assert_ne!(danger, opener);

    let tab = key(Key::Tab, egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![tab]);
    frame(&ctx, &mut app, vec![]);
    assert_ne!(ctx.memory(|m| m.focused()), Some(danger), "on to Cancel");

    frame(&ctx, &mut app, vec![key(Key::Enter, egui::Modifiers::NONE)]);
    frame(&ctx, &mut app, vec![]);
    assert!(app.session.confirm.is_none(), "Cancel took it");
    assert!(file.exists(), "nothing is deleted");
    assert_eq!(ctx.memory(|m| m.focused()), Some(opener));
}
