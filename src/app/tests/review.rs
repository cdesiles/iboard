//! The AI review's tests: running, keeping and applying it.

use super::*;
use crate::app::review::PLAN_NOTE;

/// Reviewing and "Not written yet" are for issues still in Todo.
#[test]
fn only_todo_issues_are_reviewed() {
    let dir = scratch_dir("review-todo-only");
    let ready = dir.join("issues/ready");
    std::fs::create_dir_all(&ready).unwrap();
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let file = ready.join("001-queued.md");
    std::fs::write(&file, "# Queued\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.agent.configured = true;
    assert!(!app.is_todo_issue(&file));
    app.start_review(&file, false);
    assert!(
        app.session.review.is_none(),
        "a Ready issue is not reviewed"
    );
}

/// A finished review is kept beside the issue: it stays behind the card's
/// verdict pill, survives a restart, and turns stale once the issue changes.
#[test]
fn a_review_is_kept_beside_the_issue() {
    let dir = scratch_dir("past-review");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("demo.md");
    std::fs::write(&file, "# Demo\n\nContext.\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "printf 'RELEVANCE: Pass\\nok\\nSIZE: Block\\nsplit\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE' # {prompt}"
            .to_string(),
        configured: true,
    };
    app.open_issue(file.clone());
    app.start_review(&file, false);
    assert!(until(|| {
        app.poll_review();
        app.past_reviews.contains_key(&file)
    }));
    app.close_review();
    let p = &theme::LIGHT;
    assert_eq!(app.past_reviews[&file].pill(p).0, "Block");
    assert!(dir.join("issues/.reviews/demo.toml").exists());

    let mut restarted = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let past = restarted
        .past_reviews
        .get(&file)
        .expect("kept across restarts");
    assert!(!past.stale);
    restarted.show_past_review(file.clone());
    assert!(matches!(&restarted.session.screen, Screen::Detail(path) if *path == file));
    let review = restarted
        .session
        .review
        .as_ref()
        .expect("the panel is back");
    assert!(matches!(review.status, ReviewStatus::Done { .. }));

    std::fs::write(&file, "# Demo\n\nContext, edited.\n").unwrap();
    restarted.refresh();
    assert!(
        restarted.past_reviews[&file].stale,
        "the issue changed since"
    );
}

/// An app reading `todo/01-demo.md`, with a finished review of it.
pub(super) fn open_issue_with_review(tag: &str) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\nContext.\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
    app.open_issue(file.clone());
    let raw = "RELEVANCE: Pass\nStill needed.\nSIZE: Challenge\nSplit it.\n\
                   CLARITY: Block\nWhich tool runs it?\nPROPOSAL:\n# Demo\n\nBetter.";
    app.session.review = Some(ReviewState {
        path: file.clone(),
        draft: false,
        mode: ReviewMode::Quick,
        plan: false,
        agent: "opencode · vendor/model".to_string(),
        status: ReviewStatus::Done {
            raw: raw.to_string(),
            report: agent::parse(raw),
        },
        follow_up: String::new(),
    });
    (dir, file, app)
}

#[test]
fn applying_a_review_opens_the_editor_with_an_unsaved_draft() {
    let (_dir, file, mut app) = open_issue_with_review("apply");

    app.apply_review_proposal();

    assert_eq!(app.session.detail_mode, DetailMode::Edit);
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
    let dir = scratch_dir("review-config");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
    app.open_issue(file);

    app.start_review(&todo.join("01-demo.md"), false);

    assert!(app.settings.is_some(), "the settings dialog opens instead");
    assert!(app.session.review.is_none(), "no review was started");
}

/// A configured tool (here `echo`) is spawned, polled and adopted, so the
/// running → answered hand-off is exercised for real.
#[test]
fn polling_a_running_review_adopts_the_answer() {
    let dir = scratch_dir("review-poll");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\nContext.\n").unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        command: "cat {prompt}".to_string(),
        configured: true,
        ..AgentConfig::default()
    };

    app.start_review(&file, false);
    assert!(app.review_is_running(&file));

    until(|| {
        app.poll_review();
        !app.review_is_running(&file)
    });

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
    let dir = scratch_dir("deep-review");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "grep -q 'read the repository' {prompt} && printf %s {mode}".to_string(),
        configured: true,
    };
    app.start_review_in(&file, false, ReviewMode::Deep);
    assert!(until(|| {
        app.poll_review();
        app.past_reviews.contains_key(&file)
    }));
    assert_eq!(app.past_reviews[&file].raw, "deep");
    assert_eq!(app.past_reviews[&file].mode, ReviewMode::Deep);
}

/// A comment written in the reader's margin is kept beside the issue and
/// reaches the next review, which marks it seen.
#[test]
fn a_margin_comment_reaches_the_review() {
    use egui_kittest::kittest::Queryable;
    let dir = scratch_dir("margin-comment");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("demo.md");
    std::fs::write(&file, "# Demo\n\nThree files over 1,100 lines.\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "grep -q 'Only design.rs matters' {prompt} && printf noted".to_string(),
        configured: true,
    };
    app.open_issue(file.clone());
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

    let buttons: Vec<_> = harness.get_all_by_label("Add a comment").collect();
    assert_eq!(
        buttons.len(),
        2,
        "one per block: the title and the paragraph"
    );
    buttons[1].click();
    harness.run_steps(2);
    harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .type_text("Only design.rs matters now.");
    harness.run_steps(1);
    harness.get_by_label("Comment").click();
    harness.run_steps(2);

    let sidecar = reviews::load(&dir.join("issues"), &file);
    assert_eq!(sidecar.comments.len(), 1);
    assert_eq!(sidecar.comments[0].anchor, "Three files over 1,100 lines.");
    assert!(!sidecar.comments[0].sent);

    let app = &mut harness.state_mut().1;
    app.start_review(&file, false);
    assert!(until(|| {
        app.poll_review();
        app.past_reviews.contains_key(&file)
    }));
    assert_eq!(
        app.past_reviews[&file].raw, "noted",
        "the prompt quoted the note"
    );
    assert!(reviews::load(&dir.join("issues"), &file).comments[0].sent);
}

/// A Size finding that blocks offers a split: the agent's tickets take the
/// original's place in Todo, the original is archived naming them, and one
/// `⌘Z` takes it all back.
#[test]
fn a_size_block_splits_the_issue_in_place() {
    let dir = scratch_dir("split");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    for (name, title) in [
        ("001-first.md", "First"),
        ("002-big.md", "Big"),
        ("003-last.md", "Last"),
    ] {
        std::fs::write(todo.join(name), format!("# {title}\n")).unwrap();
    }
    let big = todo.join("002-big.md");
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "if grep -q '=== TICKET ===' {prompt}; then \
                  printf '=== TICKET ===\\n# [refactor] Part A\\nDo a.\\n=== TICKET ===\\n# [refactor] Part B\\nDo b.\\n'; \
                  else printf 'RELEVANCE: Pass\\nok\\nSIZE: Block\\ntoo big\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE'; fi"
            .to_string(),
        configured: true,
    };
    app.open_issue(big.clone());
    app.start_review(&big, false);
    assert!(until(|| {
        app.poll_review();
        app.past_reviews.contains_key(&big)
    }));
    app.show_past_review(big.clone());
    app.start_split();
    assert!(until(|| {
        app.poll_split();
        !matches!(
            app.session.review.as_ref().unwrap().status,
            ReviewStatus::Splitting { .. }
        )
    }));
    let ReviewStatus::SplitReady { drafts, .. } = &app.session.review.as_ref().unwrap().status
    else {
        panic!("the tickets are proposed");
    };
    assert_eq!(drafts.len(), 2);
    let ctx = themed_ctx();
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    app.create_split();

    let names = |dir: &Path| {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| !name.starts_with('.'))
            .collect();
        names.sort();
        names
    };
    assert_eq!(
        names(&todo),
        [
            "001-first.md",
            "002-part-a.md",
            "003-part-b.md",
            "004-last.md"
        ]
    );
    let part = std::fs::read_to_string(todo.join("002-part-a.md")).unwrap();
    assert_eq!(
        part,
        "# [refactor] Part A\n\nDo a.\n\nSplit from `002-big.md`.\n"
    );
    let archived = std::fs::read_to_string(dir.join("issues/.archive/002-big.md")).unwrap();
    assert_eq!(
        archived,
        "# Big\n\nSplit into `002-part-a.md`, `003-part-b.md`.\n"
    );
    assert!(matches!(app.session.screen, Screen::Board));

    app.undo_last();
    assert_eq!(names(&todo), ["001-first.md", "002-big.md", "004-last.md"]);
    assert_eq!(std::fs::read_to_string(&big).unwrap(), "# Big\n");
}

type Drawn = egui_kittest::Harness<'static, (bool, IboardApp)>;

/// The board with `todo/01-demo.md` selected, drawn through egui_kittest,
/// its reviews run by `command`.
fn reviewing(tag: &str, command: &str) -> (Scratch, PathBuf, Drawn) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\nContext.\n").unwrap();
    std::fs::write(todo.join("02-other.md"), "# Other\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: command.to_string(),
        configured: true,
    };
    app.session.selected = Some(file.clone());
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
    (dir, file, harness)
}

/// Frames until the review of `file` is no longer running.
fn land(harness: &mut Drawn, file: &Path) {
    assert!(until(|| {
        harness.step();
        !harness.state().1.review_is_running(file)
    }));
    harness.run_steps(2);
}

const PASSING: &str = "printf 'RELEVANCE: Pass\\nok\\nSIZE: Pass\\nok\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE' # {prompt}";

/// Asking for a review returns at once: the card says it runs, the board
/// and the reader stay usable, and the reader says it runs too.
#[test]
fn a_review_runs_in_the_background() {
    use egui_kittest::kittest::Queryable;
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

/// When the answer lands the card shows it is ready; a click opens the
/// issue with the review panel.
#[test]
fn a_ready_review_opens_from_its_card() {
    use egui_kittest::kittest::Queryable;
    let (_dir, file, mut harness) = reviewing("review-ready", PASSING);
    harness.get_by_label("Review").click();
    harness.run_steps(1);
    land(&mut harness, &file);
    assert!(harness.state().1.past_reviews.contains_key(&file));
    assert!(matches!(harness.state().1.session.screen, Screen::Board));

    harness.get_by_label("Review ready").click();
    harness.run_steps(2);
    let app = &harness.state().1;
    assert!(matches!(&app.session.screen, Screen::Detail(path) if *path == file));
    let review = app.session.review.as_ref().expect("the panel opens");
    assert!(matches!(review.status, ReviewStatus::Done { .. }));
    assert!(!app.session.review_runs.is_ready(&file), "seen now");
    assert!(harness.query_by_label("Review ready").is_none());
}

/// A review asked for from the reader lands there as a ready button.
#[test]
fn a_ready_review_opens_from_the_reader() {
    use egui_kittest::kittest::Queryable;
    let (_dir, file, mut harness) = reviewing("review-ready-reader", PASSING);
    harness.state_mut().1.open_issue(file.clone());
    harness.run_steps(1);
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::R);
    harness.run_steps(1);
    assert!(harness.state().1.session.review.is_none());
    land(&mut harness, &file);

    harness.get_by_label("Review ready").click();
    harness.run_steps(2);
    let app = &harness.state().1;
    assert!(app.session.review.is_some(), "the panel opens");
    assert!(!app.session.review_runs.is_ready(&file));
}

/// A failed review says so in the status strip, and the issue keeps its
/// previous review.
#[test]
fn a_failed_review_keeps_the_last_one() {
    use egui_kittest::kittest::Queryable;
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

/// The board runs `tool`'s review, which answers with its arguments.
fn plan_review(tag: &str, tool: agent::Tool) -> (Scratch, PathBuf, Drawn) {
    let (dir, file, mut harness) = reviewing(tag, PASSING);
    harness.state_mut().1.agent.tool = tool;
    (dir, file, harness)
}

/// A review asked for from the card runs Claude in plan mode, and its panel
/// says so.
#[test]
fn a_background_review_runs_claude_in_plan_mode() {
    use egui_kittest::kittest::Queryable;
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
    assert!(harness.state().1.session.review.as_ref().unwrap().plan);
    harness.get_by_label(PLAN_NOTE);
}

/// A review asked for from the reader runs OpenCode in plan mode.
#[test]
fn a_reader_review_runs_opencode_in_plan_mode() {
    use egui_kittest::kittest::Queryable;
    let (dir, file, mut harness) = plan_review("review-plan-opencode", agent::Tool::OpenCode);
    harness.state_mut().1.open_issue(file.clone());
    harness.run_steps(1);
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::R);
    harness.run_steps(1);
    land(&mut harness, &file);
    let stored = reviews::load(&dir.join("issues"), &file).review.unwrap();
    assert!(stored.plan);
    assert!(stored.raw.contains("--agent\nplan\n"), "{}", stored.raw);
    harness.get_by_label("Review ready").click();
    harness.run_steps(2);
    harness.get_by_label(PLAN_NOTE);
}

/// A custom command has no plan mode: it runs as before, without the note.
#[test]
fn a_custom_review_has_no_plan_note() {
    use egui_kittest::kittest::Queryable;
    let (_dir, file, mut harness) = reviewing("review-plan-custom", PASSING);
    harness.get_by_label("Review").click();
    harness.run_steps(1);
    land(&mut harness, &file);
    harness.get_by_label("Review ready").click();
    harness.run_steps(2);
    assert!(!harness.state().1.session.review.as_ref().unwrap().plan);
    assert!(harness.query_by_label(PLAN_NOTE).is_none());
}
