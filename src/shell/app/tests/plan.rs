//! Tools › Plan…: the conversation, its kind, and the tickets it files.

use super::*;
use crate::plan::Kind;

/// A fake agent that answers every turn with `answer`.
fn answering(answer: &str) -> AgentConfig {
    fake_agent(&format!("printf '{answer}' # {{prompt}}"))
}

#[test]
fn the_plan_menu_pick_opens_the_panel() {
    let (_dir, mut app) = project("plan-open", &[("todo/", "")]);
    app.prefs.agent = answering("Who uses it?\\n");
    app.menu_picks(false, false, true, false);
    assert!(app.session.plan.is_some());
    assert!(!app.session.plan.as_ref().unwrap().is_resuming());

    let (ctx, raw) = panel_ctx();
    let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
    output.textures_delta.clear();
}

#[test]
fn a_plan_conversation_files_its_tickets_in_todo() {
    let (dir, mut app) = project("plan-tickets", &[("todo/", "")]);
    app.prefs.agent = answering("Who uses it, and what would they plan?\\n");
    app.open_plan();
    app.send_plan_text("A calendar view for the board");
    let panel = app.session.plan.as_ref().unwrap();
    assert_eq!(panel.plan.kind(), Kind::Epic);
    assert_eq!(panel.plan.topic(), Some("A calendar view for the board"));

    assert!(until(|| {
        app.poll_plan();
        app.session
            .plan
            .as_ref()
            .is_some_and(|panel| panel.plan.turns.len() >= 2)
    }));

    app.prefs.agent = answering(
        "I would cut it in two.\\n\
         === TICKET ===\\n\
         # [feat] Show a week\\n\
         \\n\
         Reasoning: planning needs a week at a glance.\\n\
         === TICKET ===\\n\
         # [feat] Show a release\\n\
         \\n\
         Reasoning: a release is the other unit people plan.\\n",
    );
    app.ask_plan_tickets();
    assert!(until(|| {
        app.poll_plan();
        dir.join("issues/todo/show-a-week.md").is_file()
            && dir.join("issues/todo/show-a-release.md").is_file()
    }));

    let week = std::fs::read_to_string(dir.join("issues/todo/show-a-week.md")).unwrap();
    assert!(week.starts_with("# [feat] Show a week"));
    assert!(week.contains("Reasoning: planning needs a week at a glance."));
    let panel = app.session.plan.as_ref().unwrap();
    assert_eq!(panel.status.as_deref(), Some("Filed 2 tickets in Todo."));
    assert_eq!(panel.plan.turns.len(), 3, "the agent's answer is kept");
}

#[test]
fn a_plan_reopens_only_as_a_choice() {
    let (_dir, mut app) = project("plan-reopen", &[("todo/", "")]);
    let cache = scratch_dir("plan-cache");
    app.plan_dir = Some(cache.to_path_buf());
    app.prefs.agent = answering("What users, and what would they plan?\\n");
    app.open_plan();
    app.send_plan_text("A calendar view for the board");
    assert!(until(|| {
        app.poll_plan();
        app.session
            .plan
            .as_ref()
            .is_some_and(|panel| panel.plan.turns.len() >= 2)
    }));

    app.session.plan = None;
    app.open_plan();
    assert!(app.session.plan.as_ref().unwrap().is_resuming());

    let mut harness = kittest(app);
    press(&mut harness, "Start fresh");
    let app = &harness.state().1;
    let panel = app.session.plan.as_ref().unwrap();
    assert!(!panel.is_resuming());
    assert!(panel.plan.turns.is_empty());
    let saved = std::fs::read_dir(&cache)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
        .expect("the fresh plan is saved");
    assert!(crate::plan::load(&saved).unwrap().turns.is_empty());
}

#[test]
fn the_kind_is_inferred_from_the_first_topic() {
    let (_dir, mut app) = project("plan-kind", &[("todo/", "")]);
    app.prefs.agent = answering("What breaks?\\n");
    app.open_plan();
    app.send_plan_text("The board crashes when a column is empty");
    let panel = app.session.plan.as_ref().unwrap();
    assert_eq!(panel.plan.kind(), Kind::Bug);
    assert_eq!(panel.plan.kind().name(), "Bug review");

    if let Some(panel) = app.session.plan.as_mut() {
        panel.plan.set_kind(Kind::Debt);
    }
    assert_eq!(app.session.plan.as_ref().unwrap().plan.kind(), Kind::Debt);
}
