//! Update with AI: checking an issue past Todo against its project, and
//! applying what it proposes.

use egui_kittest::kittest::Queryable;

use super::*;

/// Update with AI: an issue past Todo is checked in its project, and Apply
/// opens the editor with the update even where the board keeps it read-only.
#[test]
fn an_update_rewrites_a_doing_issue_through_the_editor() {
    let (dir, mut app) = project(
        "update",
        &[
            ("todo/", ""),
            ("doing/migrate.md", "# Migrate\n\nNext: check the dumps.\n"),
            ("done/old.md", "# Old\n"),
        ],
    );
    let file = dir.join("issues/doing/migrate.md");
    app.prefs.agent = fake_agent(
        "printf 'RELEVANCE: Block\\ndone in %s\\nSIZE: Pass\\nnothing left\\nCLARITY: Challenge\\nstale\\nPROPOSAL:\\n# Migrate\\n\\nDone.' \"$(pwd -P)\" # {prompt}",
    );
    assert!(app.updatable(&file));
    assert!(!app.updatable(&dir.join("issues/done/old.md")));
    app.start_update(&file);
    landed(&mut app, &file);
    app.show_past_review(file.clone());
    app.apply_review_proposal();
    let editor = app
        .session
        .editor
        .as_ref()
        .expect("the editor opens on a Doing issue");
    assert_eq!(editor.text.trim(), "# Migrate\n\nDone.");
    let kept = std::fs::read_to_string(dir.join("issues/.reviews/migrate.toml")).unwrap();
    assert!(
        kept.contains(&dir.display().to_string()),
        "run from the project: {kept}"
    );
}

/// An update on a Todo issue is called one, running and ready, not a review.
#[test]
fn an_update_on_todo_says_updating_and_update_ready() {
    let (_dir, file, mut harness) = reviewing("update-words", "sleep 30 # {prompt}");
    harness.state_mut().1.start_update(&file);
    harness.run_steps(2);
    harness.get_by_label("Updating…");
    harness.state_mut().1.open_issue(file.clone());
    harness.run_steps(2);
    harness.get_by_label_contains("Updating…");
    assert!(harness.query_by_label_contains("Reviewing…").is_none());
    harness.state_mut().1.cancel_review_run(&file);
    harness.state_mut().1.back_to_board();

    harness.state_mut().1.prefs.agent = fake_agent(PASSING);
    harness.state_mut().1.start_update(&file);
    land(&mut harness, &file);
    harness.get_by_label("Update ready");
    assert!(harness.query_by_label("Review ready").is_none());
}

/// An issue that reached Done has no split button, but the runs already going
/// on it keep what stops or shows them.
#[test]
fn a_run_on_an_issue_that_reached_done_keeps_its_controls() {
    let (dir, file, mut harness) = reviewing("update-done", "sleep 30 # {prompt}");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    harness.state_mut().1.refresh();
    let app = &mut harness.state_mut().1;
    app.prefs.build_agent = fake_agent("sleep 30 # {prompt}");
    app.start_update(&file);
    app.toggle_ask(&file);
    app.session.ask_draft.as_mut().unwrap().request = "Go on".to_string();
    app.send_ask();
    let moved = app.move_issue(&file, &done).expect("it moves");
    app.open_issue(moved.clone());
    harness.run_steps(2);
    assert!(harness.query_by_label("AI actions").is_none());
    harness.get_by_label_contains("Asking…").click();
    harness.run_steps(2);
    assert!(!harness.state().1.asking(&moved));
    harness.get_by_label_contains("Updating…").click();
    harness.run_steps(2);
    assert!(!harness.state().1.review_is_running(&moved));
}

/// `⌘U` starts Update with AI on the open issue, and a second one cancels it.
#[test]
fn cmd_u_starts_and_cancels_an_update() {
    let (dir, mut app) = project("update-key", &[("doing/migrate.md", "# Migrate\n")]);
    let file = dir.join("issues/doing/migrate.md");
    app.prefs.agent = fake_agent("sleep 30 # {prompt}");
    app.open_issue(file.clone());
    let ctx = themed_ctx();
    frame(&ctx, &mut app, cmd(Key::U));
    assert!(app.review_is_running(&file));
    frame(&ctx, &mut app, cmd(Key::U));
    assert!(!app.review_is_running(&file));
}

/// A finished update opens its panel on the issue being read, and when the
/// issue is opened later; `⌘↩` applies it.
#[test]
fn a_finished_update_shows_by_itself_and_applies_with_cmd_enter() {
    let (dir, mut app) = project(
        "update-shows",
        &[("doing/one.md", "# One\n"), ("doing/two.md", "# Two\n")],
    );
    let (one, two) = (
        dir.join("issues/doing/one.md"),
        dir.join("issues/doing/two.md"),
    );
    app.prefs.agent = fake_agent(
        "printf 'RELEVANCE: Challenge\\nstale\\nSIZE: Pass\\nok\\nCLARITY: Pass\\nok\\nPROPOSAL:\\n# Updated' # {prompt}",
    );
    app.start_update(&two);
    app.open_issue(one.clone());
    app.start_update(&one);
    landed(&mut app, &one);
    landed(&mut app, &two);
    assert!(
        app.session.review.as_ref().is_some_and(|r| r.path == one),
        "shown at once"
    );
    app.open_issue(two.clone());
    assert!(
        app.session.review.as_ref().is_some_and(|r| r.path == two),
        "shown on opening"
    );

    frame(&themed_ctx(), &mut app, cmd(Key::Enter));
    let editor = app
        .session
        .editor
        .as_ref()
        .expect("⌘↩ applies the proposal");
    assert_eq!(editor.text.trim(), "# Updated");
}
