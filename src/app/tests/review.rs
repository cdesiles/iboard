//! The AI review's tests: running, keeping and applying it.

use egui_kittest::kittest::Queryable;

use super::*;
use crate::app::reader::REVIEW_PANEL_WIDTH;
use crate::app::review::{PLAN_NOTE, card_pill, past_pill};

/// Reviewing and "Not written yet" are for issues still in Todo.
#[test]
fn only_todo_issues_are_reviewed() {
    let (dir, mut app) = project(
        "review-todo-only",
        &[("todo/", ""), ("ready/001-queued.md", "# Queued\n")],
    );
    let file = dir.join("issues/ready/001-queued.md");
    app.prefs.agent.configured = true;
    assert!(!app.is_todo_issue(&file));
    app.start_review(&file, false);
    assert!(
        app.session.review.is_none(),
        "a Ready issue is not reviewed"
    );
}

/// The agent reads the issue's project, wherever iboard was started from.
#[test]
fn a_review_runs_from_the_issue_project() {
    let (dir, mut app) = project("review-cwd", &[("todo/demo.md", "# Demo\n\nContext.\n")]);
    let file = dir.join("issues/todo/demo.md");
    app.prefs.agent = fake_agent(
        "printf 'RELEVANCE: Pass\\n%s\\nSIZE: Pass\\nok\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE' \"$(pwd -P)\" # {prompt}",
    );
    app.start_review(&file, false);
    landed(&mut app, &file);
    let kept = std::fs::read_to_string(dir.join("issues/.reviews/demo.toml")).unwrap();
    assert!(kept.contains(&dir.display().to_string()), "{kept}");
}

/// A finished review is kept beside the issue: it stays behind the card's
/// verdict pill, survives a restart, and turns stale once the issue changes.
#[test]
fn a_review_is_kept_beside_the_issue() {
    let (dir, mut app) = project("past-review", &[("todo/demo.md", "# Demo\n\nContext.\n")]);
    let file = dir.join("issues/todo/demo.md");
    app.prefs.agent = fake_agent(
        "printf 'RELEVANCE: Pass\\nok\\nSIZE: Block\\nsplit\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE' # {prompt}",
    );
    app.open_issue(file.clone());
    app.start_review(&file, false);
    landed(&mut app, &file);
    app.close_review();
    let p = &theme::LIGHT;
    assert_eq!(past_pill(&app.past_reviews[&file], p).0, "Block");
    let hint = card_pill(&app.past_reviews[&file], p).2;
    assert!(hint.contains("Block") && hint.contains("clarity"), "{hint}");
    assert!(dir.join("issues/.reviews/demo.toml").exists());

    let mut restarted = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let past = restarted
        .past_reviews
        .get(&file)
        .expect("kept across restarts");
    assert!(!past.stale);
    restarted.show_past_review(file.clone());
    assert!(matches!(&restarted.session.screen, Screen::Detail(path, _) if *path == file));
    let review = restarted
        .session
        .review
        .as_ref()
        .expect("the panel is back");
    assert!(review.split.is_none());

    std::fs::write(&file, "# Demo\n\nContext, edited.\n").unwrap();
    restarted.refresh();
    assert!(
        restarted.past_reviews[&file].stale,
        "the issue changed since"
    );
}

/// An app reading `todo/01-demo.md`, with a finished review of it.
pub(super) fn open_issue_with_review(tag: &str) -> (Scratch, PathBuf, IboardApp) {
    let (dir, mut app) = project(tag, &[("todo/01-demo.md", "# Demo\n\nContext.\n")]);
    let file = dir.join("issues/todo/01-demo.md");
    app.open_issue(file.clone());
    let raw = "RELEVANCE: Pass\nStill needed.\nSIZE: Challenge\nSplit it.\n\
                   CLARITY: Block\nWhich tool runs it?\nPROPOSAL:\n# Demo\n\nBetter.";
    app.session.review = Some(ReviewState {
        path: file.clone(),
        review: PastReview {
            draft: false,
            mode: ReviewMode::Quick,
            plan: false,
            agent: "opencode · vendor/model".to_string(),
            raw: raw.to_string(),
            report: agent::parse(raw),
            stale: false,
        },
        split: None,
        follow_up: None,
    });
    (dir, file, app)
}

#[test]
fn applying_a_review_opens_the_editor_with_an_unsaved_draft() {
    let (_dir, file, mut app) = open_issue_with_review("apply");

    app.apply_review_proposal();

    assert_eq!(app.session.editing(), Some(file.as_path()));
    assert_eq!(app.session.editor.as_ref().unwrap().path, file);
    assert_eq!(
        app.session.editor.as_ref().unwrap().text,
        "# Demo\n\nBetter."
    );
    assert!(
        app.session.editor.as_ref().unwrap().is_dirty(),
        "the draft is unsaved"
    );
    assert!(
        app.session.review.is_none(),
        "the panel closes once applied"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Demo\n\nContext.\n",
        "the agent never writes the file"
    );
}

#[test]
fn reviewing_without_a_configured_agent_opens_the_configuration() {
    let (dir, mut app) = project("review-config", &[("todo/01-demo.md", "# Demo\n")]);
    let file = dir.join("issues/todo/01-demo.md");
    app.open_issue(file.clone());

    app.start_review(&file, false);

    assert!(app.settings.is_some(), "the settings dialog opens instead");
    assert!(app.session.review.is_none(), "no review was started");
}

/// A configured tool (here `echo`) is spawned, polled and adopted, so the
/// running → answered hand-off is exercised for real.
#[test]
fn polling_a_running_review_adopts_the_answer() {
    let (dir, mut app) = project(
        "review-poll",
        &[("todo/01-demo.md", "# Demo\n\nContext.\n")],
    );
    let file = dir.join("issues/todo/01-demo.md");
    app.prefs.agent = fake_agent("cat {prompt}");

    app.start_review(&file, false);
    assert!(app.review_is_running(&file));
    landed(&mut app, &file);

    let past = &app.past_reviews[&file];
    assert!(
        past.raw.contains("You review software issues"),
        "got: {:?}",
        past.raw
    );
    assert!(
        past.report.is_none(),
        "prose that is not a report stays raw"
    );
}

/// A Deep review reaches the agent as such, and is kept with its mode.
#[test]
fn a_deep_review_says_so_and_is_kept_as_deep() {
    let (dir, mut app) = project("deep-review", &[("todo/demo.md", "# Demo\n")]);
    let file = dir.join("issues/todo/demo.md");
    app.prefs.agent = fake_agent("grep -q 'read the repository' {prompt} && printf %s {mode}");
    app.start_review_in(&file, false, ReviewMode::Deep);
    landed(&mut app, &file);
    assert_eq!(app.past_reviews[&file].raw, "deep");
    assert_eq!(app.past_reviews[&file].mode, ReviewMode::Deep);
}

/// The board with `todo/01-demo.md` selected, drawn through egui_kittest,
/// its reviews run by `command`.
pub(super) fn reviewing(tag: &str, command: &str) -> (Scratch, PathBuf, Kit) {
    let (dir, mut app) = project(
        tag,
        &[
            ("todo/01-demo.md", "# Demo\n\nContext.\n"),
            ("todo/02-other.md", "# Other\n"),
        ],
    );
    let file = dir.join("issues/todo/01-demo.md");
    app.prefs.agent = fake_agent(command);
    app.session.selected = Some(file.clone());
    (dir, file, kittest(app))
}

/// Frames until the review of `file` is no longer running.
pub(super) fn land(harness: &mut Kit, file: &Path) {
    assert!(until(|| {
        harness.step();
        !harness.state().1.review_is_running(file)
    }));
    harness.run_steps(2);
}

/// Asking for a review returns at once: the card says it runs, the board
/// and the reader stay usable, and the reader says it runs too.
#[test]
fn a_review_runs_in_the_background() {
    let (_dir, file, mut harness) = reviewing("review-running", "sleep 30 # {prompt}");
    harness.get_by_label("Review").click();
    harness.run_steps(2);
    let app = &harness.state().1;
    assert!(app.review_is_running(&file));
    assert!(
        matches!(app.session.screen, Screen::Board),
        "the board stays"
    );
    assert!(app.session.review.is_none(), "no panel while it runs");
    harness.get_by_label("Reviewing…");

    harness.state_mut().1.move_selection(Key::ArrowDown);
    harness.run_steps(1);
    harness.state_mut().1.open_issue(file.clone());
    harness.run_steps(2);
    assert!(harness.state().1.review_is_running(&file));
    harness.get_by_label("Reviewing…").click();
    harness.run_steps(1);
    assert!(
        !harness.state().1.review_is_running(&file),
        "the reader's button cancels it"
    );
}

/// While a review runs on the issue being read, the panel beside it shows the
/// time spent and the agent's step, and Cancel stops the run.
#[test]
fn the_panel_shows_a_running_review() {
    let (_dir, file, mut harness) = reviewing("review-progress", "sleep 30 # {prompt}");
    harness.state_mut().1.open_issue(file.clone());
    harness.state_mut().1.start_review(&file, false);
    harness.run_steps(2);
    harness.get_by_label("Review of 01-demo.md");
    harness.get_by_label("The agent is reading the issue…");
    assert!(
        ["0s", "1s"]
            .iter()
            .any(|clock| harness.query_by_label(clock).is_some()),
        "the clock counts whole seconds"
    );

    let runs = &harness.state().1.session.review_runs;
    *runs.running[&file].progress.lock().unwrap() = Some("Reading src/app.rs…".to_string());
    harness.run_steps(1);
    harness.get_by_label("Reading src/app.rs…");

    harness.get_by_label("Cancel").click();
    harness.run_steps(2);
    assert!(!harness.state().1.review_is_running(&file));
    assert!(harness.query_by_label("Review of 01-demo.md").is_none());
}

/// A title alone gets the sections it misses: no findings, and they apply to
/// the draft.
#[test]
fn a_title_alone_gets_proposed_sections() {
    let (dir, _file, mut harness) = reviewing(
        "review-title",
        "printf 'PROPOSAL:\\n# Other\\n\\n## Context\\n\\nWhy print.' # {prompt}",
    );
    let file = dir.join("issues/todo/02-other.md");
    harness.state_mut().1.open_issue(file.clone());
    harness.state_mut().1.start_review(&file, false);
    land(&mut harness, &file);
    harness.run_steps(2);
    let app = &harness.state().1;
    assert!(app.proposal_shown());
    let review = app.session.review.as_ref().unwrap();
    assert!(review.review.report.as_ref().unwrap().findings.is_empty());
    harness.get_by_label_contains("only a title");
    harness.get_by_label_contains("Why print.");
    assert!(harness.query_by_label("RELEVANCE").is_none());
}

/// Ask again is a button: a first click opens the field, a second sends what
/// it holds. The findings read as rows under an uppercase label.
#[test]
fn ask_again_opens_its_field_then_sends() {
    let (_dir, file, app) = open_issue_with_review("review-ask");
    let mut harness = kittest(app);
    harness.run_steps(1);
    harness.get_by_label("RELEVANCE");
    harness.get_by_label("Ask again").click();
    harness.run_steps(2);
    let app = &mut harness.state_mut().1;
    assert_eq!(
        app.session.review.as_ref().unwrap().follow_up.as_deref(),
        Some("")
    );
    app.prefs.agent = fake_agent("sleep 30 # {prompt}");
    app.session.review.as_mut().unwrap().follow_up = Some("Make it smaller".to_string());
    harness.run_steps(1);
    harness.get_by_label("Ask again").click();
    harness.run_steps(2);
    assert!(harness.state().1.review_is_running(&file));
}

/// When the answer lands the card shows it is ready; a click opens the
/// issue with the review panel.
#[test]
fn a_ready_review_opens_from_its_card() {
    let (_dir, file, mut harness) = reviewing("review-ready", PASSING);
    harness.get_by_label("Review").click();
    harness.run_steps(1);
    land(&mut harness, &file);
    assert!(harness.state().1.past_reviews.contains_key(&file));
    assert!(matches!(harness.state().1.session.screen, Screen::Board));

    harness.get_by_label("Review ready").click();
    harness.run_steps(2);
    let app = &harness.state().1;
    assert!(matches!(&app.session.screen, Screen::Detail(path, _) if *path == file));
    let review = app.session.review.as_ref().expect("the panel opens");
    assert!(review.split.is_none());
    assert!(!app.session.review_runs.is_ready(&file), "seen now");
    assert!(harness.query_by_label("Review ready").is_none());
}

/// A review asked for from the reader opens its panel there as it lands.
#[test]
fn a_ready_review_opens_in_the_reader() {
    let (dir, file, mut harness) = reviewing("review-ready-reader", PASSING);
    let gate = dir.join("gate");
    harness.state_mut().1.prefs.agent.command = format!(
        "while [ ! -e '{}' ]; do sleep 0.01; done; {PASSING}",
        gate.display()
    );
    harness.state_mut().1.open_issue(file.clone());
    harness.run_steps(1);
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::R);
    harness.run_steps(1);
    assert!(harness.state().1.session.review.is_none());
    std::fs::write(&gate, "").unwrap();
    land(&mut harness, &file);

    harness.run_steps(2);
    let app = &harness.state().1;
    assert!(app.session.review.is_some(), "the panel opens");
    assert!(!app.session.review_runs.is_ready(&file));
    assert!(harness.query_by_label("Review ready").is_none());
}

/// A failed review says so in the status strip, and the issue keeps its
/// previous review.
#[test]
fn a_failed_review_keeps_the_last_one() {
    let (dir, file, mut harness) = reviewing("review-failed", "exit 3 # {prompt}");
    let previous = reviews::StoredReview {
        hash: reviews::content_hash("# Demo\n\nContext.\n"),
        agent: "custom · default".to_string(),
        mode: "quick".to_string(),
        draft: false,
        plan: false,
        at: chrono::Local::now().to_rfc3339(),
        raw: "RELEVANCE: Pass\nok\nSIZE: Block\nsplit\nCLARITY: Pass\nok\nPROPOSAL: NONE"
            .to_string(),
    };
    reviews::save_review(&dir.join("issues"), &file, previous).unwrap();
    harness.state_mut().1.refresh();
    harness.run_steps(1);
    harness.get_by_label("Block");

    harness.get_by_label("Review").click();
    harness.run_steps(1);
    land(&mut harness, &file);
    let error = harness.state().1.error.clone().expect("the strip names it");
    assert!(error.starts_with("Review of 01-demo.md failed"), "{error}");
    harness.get_by_label(&error);
    harness.get_by_label("Block");
    assert!(harness.query_by_label("Review ready").is_none());
    let sidecar = reviews::load(&dir.join("issues"), &file);
    assert!(sidecar.review.unwrap().raw.contains("SIZE: Block"));
}

/// The board runs `tool`'s review, which answers with its arguments, on a
/// model of its own rather than the developer's default.
fn plan_review(tag: &str, tool: agent::Tool) -> (Scratch, PathBuf, Kit) {
    let (dir, file, mut harness) = reviewing(tag, PASSING);
    let agent = &mut harness.state_mut().1.prefs.agent;
    agent.tool = tool;
    agent.model = "m".to_string();
    (dir, file, harness)
}

/// A review asked for from the card runs Claude in plan mode, and its panel
/// says so.
#[test]
fn a_background_review_runs_claude_in_plan_mode() {
    let (dir, file, mut harness) = plan_review("review-plan-claude", agent::Tool::Claude);
    harness.get_by_label("Review").click();
    harness.run_steps(1);
    land(&mut harness, &file);
    let stored = reviews::load(&dir.join("issues"), &file).review.unwrap();
    assert!(stored.plan);
    assert!(
        stored.raw.contains("--permission-mode\nplan\n"),
        "{}",
        stored.raw
    );

    harness.get_by_label("Review ready").click();
    harness.run_steps(2);
    let review = &harness.state().1.session.review.as_ref().unwrap().review;
    assert!(review.plan);
    harness.get_by_label(PLAN_NOTE);
}

/// A review asked for from the reader runs OpenCode in plan mode.
#[test]
fn a_reader_review_runs_opencode_in_plan_mode() {
    let (dir, file, mut harness) = plan_review("review-plan-opencode", agent::Tool::OpenCode);
    harness.state_mut().1.open_issue(file.clone());
    harness.run_steps(1);
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::R);
    harness.run_steps(1);
    land(&mut harness, &file);
    let stored = reviews::load(&dir.join("issues"), &file).review.unwrap();
    assert!(stored.plan);
    assert!(stored.raw.contains("--agent\nplan\n"), "{}", stored.raw);
    harness.run_steps(2);
    harness.get_by_label(PLAN_NOTE);
}

/// A custom command has no plan mode: it runs as before, without the note.
#[test]
fn a_custom_review_has_no_plan_note() {
    let (_dir, file, mut harness) = reviewing("review-plan-custom", PASSING);
    harness.get_by_label("Review").click();
    harness.run_steps(1);
    land(&mut harness, &file);
    harness.get_by_label("Review ready").click();
    harness.run_steps(2);
    let review = &harness.state().1.session.review.as_ref().unwrap().review;
    assert!(!review.plan);
    assert!(harness.query_by_label(PLAN_NOTE).is_none());
}

/// The scope Settings › AI review chose is what the agent is told, and what
/// the review panel names.
#[test]
fn the_review_runs_in_the_scope_settings_chose() {
    for (scope, told) in [
        (ReviewMode::Quick, "Read only the issue"),
        (ReviewMode::Deep, "its design system"),
    ] {
        let command = format!("grep -q '{told}' {{prompt}} && {PASSING}");
        let (_dir, file, mut harness) = reviewing("review-scope", &command);
        harness.state_mut().1.prefs.agent.scope = scope;
        harness.get_by_label("Review").click();
        harness.run_steps(1);
        land(&mut harness, &file);
        assert_eq!(harness.state().1.past_reviews[&file].mode, scope);
        harness.get_by_label("Review ready").click();
        harness.run_steps(2);
        let detail = format!("custom · default · {} · file", scope.scope_name());
        harness.get_by_label(&detail);
    }
}

/// A long agent name wraps in the review head instead of widening the panel
/// under the issue.
#[test]
fn a_long_agent_name_keeps_the_review_panel_in_its_place() {
    let (dir, mut app) = project("review-head", &[("todo/migrate.md", "# Migrate\n")]);
    let file = dir.join("issues/todo/migrate.md");
    app.prefs.agent = AgentConfig {
        model: "a-model-whose-name-goes-on-and-on-well-past-the-panel".to_string(),
        ..fake_agent(PASSING)
    };
    app.start_review(&file, false);
    landed(&mut app, &file);
    app.show_past_review(file);
    let mut harness = kittest(app);
    harness.run_steps(1);
    let title = harness.get_by_label("Review of migrate.md").rect();
    let panel_left = window().width() - REVIEW_PANEL_WIDTH;
    assert!(
        title.min.x >= panel_left,
        "{title:?} starts left of {panel_left}"
    );
    let detail = harness.get_by_label_contains("on-and-on").rect();
    assert!(
        detail.height() > 2.0 * theme::meta().size,
        "{detail:?} wraps"
    );
    assert!(detail.max.x <= window().width(), "{detail:?} stays inside");
}

/// The panel and the settings window go through a real egui pass, so a
/// panic or a layout mistake surfaces without a display.
#[test]
fn the_review_panel_and_config_window_render() {
    let (_dir, _file, mut app) = open_issue_with_review("review-render");
    let ctx = themed_ctx();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };

    for _ in 0..2 {
        ctx.run_ui(input.clone(), |ui| app.draw(ui))
            .textures_delta
            .clear();
    }

    app.open_config();
    for _ in 0..2 {
        ctx.run_ui(input.clone(), |ui| app.draw(ui))
            .textures_delta
            .clear();
    }
}
