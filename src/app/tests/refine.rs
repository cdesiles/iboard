//! Refinement's tests: the conversation, stopping a turn, applying the spec.

use super::*;
use egui_kittest::kittest::Queryable;

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
    refine.note = "Touch users miss the buttons.".to_string();

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
    assert!(
        !app.refinement_shown(),
        "the popin tucks away for the draft"
    );
    app.start_refinement(&file);
    assert!(app.refinement_shown());
    assert_eq!(turns(&app), 3, "the menu brings the same conversation back");
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
    app.session.refine.as_mut().unwrap().note = "Because.".to_string();
    app.send_reply();
    assert!(app.session.refine.as_ref().unwrap().is_running());

    app.stop_refinement();
    let refine = app.session.refine.as_ref().unwrap();
    assert!(!refine.is_running());
    assert_eq!((refine.turns.len(), refine.note.as_str()), (1, "Because."));

    app.session.refine = None;
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

/// The panel sits over the page: Hide tucks it away and keeps the
/// conversation, the pill at the foot brings it back, End refining ends it.
#[test]
fn the_popin_hides_and_comes_back() {
    let (_dir, file, mut app) = refining("refine-popin", "printf 'Why?' # {prompt}");
    app.start_refinement(&file);
    assert!(until(|| {
        app.poll_refinement();
        turns(&app) == 1
    }));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(window().size())
        .build_ui_state(
            |ui, (themed, app): &mut (bool, IboardApp)| {
                if !*themed {
                    theme::install(ui.ctx());
                    *themed = true;
                    return;
                }
                app.frame_logic(ui.ctx());
                app.draw(ui);
            },
            (false, app),
        );
    harness.run_steps(2);
    harness.get_by_label("Hide").click();
    harness.run_steps(4);
    assert!(!harness.state().1.refinement_shown());
    assert_eq!(turns(&harness.state().1), 1);

    harness.get_by_label("Back to the refinement").click();
    harness.run_steps(2);
    assert!(harness.state().1.refinement_shown());
    harness.get_by_label("End refining").click();
    harness.run_steps(2);
    assert!(harness.state().1.session.refine.is_none());
}

/// The app in a kittest harness of `size`, drawn the way the window is.
fn harness(app: IboardApp, size: Vec2) -> egui_kittest::Harness<'static, (bool, IboardApp)> {
    let mut harness = egui_kittest::Harness::builder()
        .with_size(size)
        .build_ui_state(
            |ui, (themed, app): &mut (bool, IboardApp)| {
                if !*themed {
                    theme::install(ui.ctx());
                    *themed = true;
                    return;
                }
                app.frame_logic(ui.ctx());
                app.draw(ui);
            },
            (false, app),
        );
    harness.run_steps(2);
    harness
}

const CHOICES: &str = "printf 'TLDR: Keep it small.\\n\\nWho needs it?\\nOPTIONS:\\n\
    - Ask them first: talk to touch users.\\n- Drop it\\nPROPOSAL: NONE' # {prompt}";

/// A number key picks a choice and the same key unpicks it; Send names what
/// goes out and sends the choice's full text, then the note. The round
/// folds into one line above the next TL;DR.
#[test]
fn a_choice_is_picked_then_sent_with_its_note() {
    let (_dir, file, mut app) = refining("refine-options", CHOICES);
    app.start_refinement(&file);
    assert!(until(|| {
        app.poll_refinement();
        turns(&app) == 1
    }));
    let turn = &app.session.refine.as_ref().unwrap().turns[0];
    assert_eq!(
        (turn.tldr.as_deref(), turn.text.as_str()),
        (Some("Keep it small."), "Who needs it?")
    );
    let mut harness = harness(app, window().size());
    let choice = |harness: &egui_kittest::Harness<'_, (bool, IboardApp)>| {
        harness.state().1.session.refine.as_ref().unwrap().choice
    };
    harness.key_press(egui::Key::Num2);
    harness.run_steps(2);
    assert_eq!(choice(&harness), Some(1));
    assert!(harness.query_by_label("Send choice 2").is_some());
    harness.key_press(egui::Key::Num2);
    harness.run_steps(2);
    assert_eq!(choice(&harness), None, "the same key unpicks it");
    assert!(
        harness
            .query_by_label("Or answer in your own words")
            .is_some()
    );

    harness.get_by_label("Ask them first").click();
    harness.run_steps(2);
    harness.state_mut().1.session.refine.as_mut().unwrap().note = "This week.".into();
    harness.run_steps(2);
    harness.get_by_label("Send choice 1 with note").click();
    harness.run_steps(2);
    let refine = harness.state().1.session.refine.as_ref().unwrap();
    assert_eq!(
        refine.turns[1].text,
        "Ask them first: talk to touch users.\n\nThis week."
    );
    let app = &mut harness.state_mut().1;
    assert!(until(|| {
        app.poll_refinement();
        turns(app) == 3
    }));
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label("Round 1: you chose Ask them first")
            .is_some()
    );
}

/// Nothing to send keeps Send disabled; `tab` goes to the note, `⌘↩` sends
/// it, and `esc` with no field focused hides the panel.
#[test]
fn the_note_alone_is_an_answer_and_esc_hides() {
    let (_dir, file, mut app) = refining("refine-note", CHOICES);
    app.start_refinement(&file);
    assert!(until(|| {
        app.poll_refinement();
        turns(&app) == 1
    }));
    let mut harness = harness(app, window().size());
    harness.get_by_label("Send answer").click();
    harness.run_steps(2);
    assert_eq!(turns(&harness.state().1), 1, "nothing to send");

    harness.key_press(egui::Key::Tab);
    harness.run_steps(2);
    assert!(harness.ctx.memory(|m| m.focused().is_some()));
    harness.state_mut().1.session.refine.as_mut().unwrap().note = "Nobody.".into();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
    harness.run_steps(2);
    let refine = harness.state().1.session.refine.as_ref().unwrap();
    assert_eq!(refine.turns[1].text, "Nobody.");

    let app = &mut harness.state_mut().1;
    assert!(until(|| {
        app.poll_refinement();
        turns(app) == 3
    }));
    harness
        .ctx
        .memory_mut(|m| m.surrender_focus(m.focused().unwrap_or(egui::Id::NULL)));
    harness.run_steps(2);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    let app = &harness.state().1;
    assert!(!app.refinement_shown() && app.session.refine.is_some());
}

/// On a narrow window the panes stack and the reasoning folds under the
/// TL;DR until asked for.
#[test]
fn a_narrow_panel_folds_the_reasoning() {
    let (_dir, file, mut app) = refining("refine-narrow", CHOICES);
    app.start_refinement(&file);
    assert!(until(|| {
        app.poll_refinement();
        turns(&app) == 1
    }));
    let mut harness = harness(app, Vec2::new(760.0, 900.0));
    assert!(harness.query_by_label("Who needs it?").is_none());
    harness
        .get_by_label("Show the reasoning (1 paragraph)")
        .click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Who needs it?").is_some());
    assert!(harness.query_by_label("Drop it").is_some());
}
