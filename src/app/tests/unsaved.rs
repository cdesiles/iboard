//! Applying an agent's proposal over text the Editor has not saved.

use egui_kittest::kittest::Queryable;

use super::kit::press;
use super::reading::kit;
use super::review::open_issue_with_review;
use super::*;

fn editing_unsaved(tag: &str, mine: &str) -> (Scratch, PathBuf, IboardApp) {
    let (dir, file, mut app) = open_issue_with_review(tag);
    app.prefs.auto = false;
    app.open_editor(&file);
    app.session.editor.as_mut().unwrap().text = mine.to_string();
    (dir, file, app)
}

/// Unsaved text in the Editor is never replaced unasked, however the Editor
/// was reached: the proposal waits on the row's answer, the panel beside it.
#[test]
fn applying_over_unsaved_text_asks_first() {
    let mine = "# Demo\n\nMine.";
    let (_dir, file, mut app) = editing_unsaved("apply-over-unsaved", mine);
    let saved = std::fs::read_to_string(&file).unwrap();
    app.session.stop_editing();

    app.apply_review_proposal();

    assert_eq!(app.session.editing(), Some(file.as_path()));
    assert_eq!(app.session.editor.as_ref().unwrap().text, mine);
    assert!(app.session.review.is_some(), "the proposal stays in view");
    let mut harness = kit(app);
    harness.run_steps(2);
    press(&mut harness, "Keep mine");
    assert_eq!(
        harness.state().1.session.editor.as_ref().unwrap().text,
        mine
    );
    assert!(harness.query_by_label("Replace mine").is_none(), "answered");

    harness.state_mut().1.apply_review_proposal();
    harness.run_steps(2);
    press(&mut harness, "Replace mine");
    let app = &harness.state().1;
    let text = &app.session.editor.as_ref().unwrap().text;
    assert_eq!(text, "# Demo\n\nBetter.");
    assert!(app.session.review.is_none(), "applied, the panel closes");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
}

/// A name edited but no text: nothing the proposal would lose, so it applies.
#[test]
fn applying_over_an_unsaved_name_alone_applies() {
    let (_dir, file, mut app) = open_issue_with_review("apply-over-name");
    app.prefs.auto = false;
    app.open_editor(&file);
    app.session.editor.as_mut().unwrap().name = "02-other.md".to_string();
    app.apply_review_proposal();
    let editor = app.session.editor.as_ref().unwrap();
    assert_eq!(editor.text, "# Demo\n\nBetter.");
    assert_eq!(editor.name, "02-other.md");
}

/// With Auto on, an applied proposal stays unsaved until the author saves it.
#[test]
fn an_applied_proposal_waits_for_the_author_with_auto_on() {
    let (_dir, file, mut app) = open_issue_with_review("apply-auto");
    let saved = std::fs::read_to_string(&file).unwrap();
    app.prefs.auto = true;
    app.apply_review_proposal();
    let mut harness = kit(app);
    std::thread::sleep(crate::app::editor::AUTOSAVE_AFTER);
    harness.run_steps(2);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
    assert!(harness.state().1.has_unsaved_edits());

    harness.state_mut().1.save_and_preview();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n\nBetter.");
}

/// Discarding the draft drops the question with it.
#[test]
fn discarding_the_edits_ends_the_question() {
    let (_dir, file, mut app) = editing_unsaved("apply-then-discard", "# Demo\n\nMine.");
    app.apply_review_proposal();
    app.discard_edits();
    app.open_editor(&file);
    let mut harness = kit(app);
    harness.run_steps(2);
    assert!(harness.query_by_label("Replace mine").is_none());
}
