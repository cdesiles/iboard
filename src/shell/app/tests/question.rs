//! The build's question card: its fields, errors and stopped note.

use egui_kittest::kittest::Queryable;

use super::build::{board, form_question, permission_question, ready, waiting_question};
use super::*;
use crate::agent::service::QuestionKind;

#[test]
fn a_claude_question_says_how_to_answer_it_elsewhere() {
    let (_dir, mut app) = board("build-claude-waiting", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path.clone(), waiting_question());
    let mut harness = kittest(app);
    harness
        .get_by_label("Waiting: approve Write: /code/app/proof.txt")
        .click();
    harness.run_steps(2);
    assert!(
        harness.state().1.agent_log_open(&path),
        "the ticket opens beside its log"
    );
    harness.get_by_label("claude attach 0d643c37");
    harness.get_by_label("This build cannot take a reply.");
    assert!(
        harness.query_by_label("Send answer").is_none(),
        "there is nothing to send from the board"
    );
    assert!(
        harness
            .query_by_label("Answer the question above to continue.")
            .is_none(),
        "the question is answered in Claude Code, not above"
    );
    assert!(
        harness.state().1.session.answer.is_none(),
        "no draft waits for an answer here"
    );
}

#[test]
fn a_permission_outside_the_project_offers_no_always_allow() {
    let (_dir, mut app) = board("build-permission-outside", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    let mut question = permission_question();
    if let QuestionKind::Permission(permission) = &mut question.kind {
        permission.resources = vec!["rm -rf ~/workspace/perso/old-project/build".into()];
    }
    app.seed_question(path, question);
    let mut harness = kittest(app);
    harness.get_by_label("Answer: Allow bash?").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Outside ");
    assert!(
        harness
            .query_by_label_contains("Always allow in ")
            .is_none(),
        "an always-grant outside the project would undo its scope"
    );
    assert!(harness.get_by_label("Allow once").rect().width() > 0.0);
    assert!(harness.get_by_label("Reject").rect().width() > 0.0);
}

#[test]
fn choosing_other_reveals_its_field_and_keeps_the_typed_answer() {
    let (_dir, mut app) = board("build-other", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path, form_question());
    let mut harness = kittest(app);
    harness.get_by_label("Answer: Which approach?").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Other…").is_some());
    assert!(
        harness
            .query_by_role(egui::accesskit::Role::TextInput)
            .is_none(),
        "the Other field appears only once the row is picked"
    );
    harness.get_by_label("Other…").click();
    harness.run_steps(2);
    harness
        .get_by_role(egui::accesskit::Role::TextInput)
        .type_text("A third way");
    harness.run_steps(1);
    let answer = harness
        .state()
        .1
        .form_answer_for_test()
        .expect("the typed answer validates");
    assert_eq!(
        answer.get("approach").and_then(|value| value.as_str()),
        Some("A third way")
    );
}

#[test]
fn an_unanswered_required_field_says_why_and_takes_focus() {
    let (_dir, mut app) = board("build-required", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path, form_question());
    let mut harness = kittest(app);
    harness.get_by_label("Answer: Which approach?").click();
    harness.run_steps(2);
    harness.get_by_label("Send answer").click();
    harness.run_steps(2);
    harness.get_by_label("Answer Approach first.");
    assert!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::RadioButton, "Fast")
            .is_focused(),
        "the missing field takes focus"
    );
}

#[test]
fn a_failed_send_keeps_the_card_and_offers_retry() {
    let (_dir, mut app) = board("build-send-failure", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path.clone(), form_question());
    app.open_answer(path);
    app.fail_answer_for_test("Couldn't send: the build is no longer running.");
    let harness = kittest(app);
    harness.get_by_label("Couldn't send: the build is no longer running.");
    harness.get_by_label("Retry");
    assert!(harness.query_by_label("Send answer").is_none());
    harness.get_by_label("Fast");
}

#[test]
fn a_stopped_build_keeps_its_question_as_a_quiet_note() {
    let (_dir, mut app) = board("build-stopped", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path.clone(), form_question());
    app.open_answer(path);
    app.stop_build();
    let mut harness = kittest(app);
    harness.get_by_label("This build stopped before it was answered.");
    harness.get_by_label("Which approach?");
    harness.get_by_label("Dismiss").click();
    harness.run_steps(2);
    assert!(harness.state().1.session.answer.is_none());
    assert!(
        harness
            .query_by_label("This build stopped before it was answered.")
            .is_none()
    );
}

#[test]
fn a_waiting_question_does_not_keep_the_log_open() {
    let (_dir, mut app) = board("build-log-close", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path.clone(), form_question());
    app.open_answer(path.clone());
    assert!(app.agent_log_open(&path));
    app.toggle_agent_log(&path);
    assert!(!app.agent_log_open(&path), "the log closes on a question");
}
