//! Refinement's tests: the conversation, stopping a turn, applying the spec.

use super::*;

/// An app reading `todo/01-demo.md`, whose agent is the custom `command`.
fn refining(tag: &str, command: &str) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\nContext.\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        command: command.to_string(),
        configured: true,
        ..AgentConfig::default()
    };
    app.open_issue(file.clone());
    (dir, file, app)
}

fn turns(app: &IboardApp) -> usize {
    app.session.refine.as_ref().map_or(0, |r| r.turns.len())
}

/// The challenger speaks first, reads the author's answer back with the
/// issue, and its refined issue lands in the editor, never in the file.
#[test]
fn a_conversation_ends_in_a_refined_issue() {
    let (_dir, file, mut app) = refining(
        "refine-talk",
        "if grep -q 'Author: Touch users' {prompt}; then \
         printf 'Settled.\\nPROPOSAL:\\n# Spec'; \
         else printf 'Who needs it?\\nPROPOSAL: NONE'; fi",
    );
    app.start_refinement(&file);
    assert!(until(|| {
        app.poll_refinement();
        turns(&app) == 1
    }));
    let refine = app.session.refine.as_mut().unwrap();
    assert_eq!(refine.turns[0].text, "Who needs it?");
    assert!(refine.proposal().is_none());
    refine.reply = "Touch users miss the buttons.".to_string();

    app.send_reply();
    assert!(until(|| {
        app.poll_refinement();
        turns(&app) == 3
    }));
    let (ctx, input) = panel_ctx();
    for _ in 0..2 {
        ctx.run_ui(input.clone(), |ui| app.draw(ui))
            .textures_delta
            .clear();
    }

    app.apply_refinement();
    assert_eq!(app.session.editor.as_ref().unwrap().text, "# Spec");
    assert_eq!(app.session.detail_mode, DetailMode::Edit);
    assert!(app.session.refine.is_some(), "the conversation stays open");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Demo\n\nContext.\n"
    );
}

/// Stopping a turn gives the author's answer back to edit; `esc` before
/// the challenger's first word closes the refinement.
#[test]
fn stopping_a_turn_gives_the_answer_back() {
    let (_dir, file, mut app) = refining(
        "refine-stop",
        "grep -q 'Author:' {prompt} && sleep 30; printf 'Why?'",
    );
    app.start_refinement(&file);
    assert!(until(|| {
        app.poll_refinement();
        turns(&app) == 1
    }));
    app.session.refine.as_mut().unwrap().reply = "Because.".to_string();
    app.send_reply();
    assert!(app.session.refine.as_ref().unwrap().is_running());

    app.stop_refinement();
    let refine = app.session.refine.as_ref().unwrap();
    assert!(!refine.is_running());
    assert_eq!((refine.turns.len(), refine.reply.as_str()), (1, "Because."));

    app.start_refinement(&file);
    app.stop_refinement();
    assert!(app.session.refine.is_none());
}

/// Refinement and review share their place: one replaces the other, and
/// without an agent the settings open instead.
#[test]
fn a_refinement_and_a_review_replace_each_other() {
    let (_dir, file, mut app) = refining("refine-review", "printf 'Why?' # {prompt}");
    app.start_refinement(&file);
    app.start_review(&file, false);
    assert!(app.session.refine.is_none() && app.session.review.is_some());
    app.start_refinement(&file);
    assert!(app.session.refine.is_some() && app.session.review.is_none());

    app.session.refine = None;
    app.agent.configured = false;
    app.start_refinement(&file);
    assert!(app.settings.is_some() && app.session.refine.is_none());
}
