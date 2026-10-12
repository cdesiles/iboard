//! The AI review's tests: running, keeping and applying it.

use super::*;

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
    assert!(app.review.is_none(), "a Ready issue is not reviewed");
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
    app.cancel_review();
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
    assert!(matches!(&restarted.screen, Screen::Detail(path) if *path == file));
    let review = restarted.review.as_ref().expect("the panel is back");
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
    app.review = Some(ReviewState {
        path: file.clone(),
        draft: false,
        agent: "opencode · vendor/model".to_string(),
        status: ReviewStatus::Done {
            raw: raw.to_string(),
            report: agent::parse(raw),
        },
        hash: String::new(),
        follow_up: String::new(),
    });
    (dir, file, app)
}

#[test]
fn applying_a_review_opens_the_editor_with_an_unsaved_draft() {
    let (_dir, file, mut app) = open_issue_with_review("apply");

    app.apply_review_proposal();

    assert_eq!(app.detail_mode, DetailMode::Edit);
    assert_eq!(app.editor.as_ref().unwrap().path, file);
    assert_eq!(app.editor.as_ref().unwrap().text, "# Demo\n\nBetter.");
    assert!(
        app.editor.as_ref().unwrap().is_dirty(),
        "the draft is unsaved"
    );
    assert!(app.review.is_none(), "the panel closes once applied");
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
    assert!(app.review.is_none(), "no review was started");
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

    match &app.review.as_ref().unwrap().status {
        ReviewStatus::Done { raw, report } => {
            assert!(raw.contains("You review software issues"), "got: {raw:?}");
            assert!(report.is_none(), "prose that is not a report stays raw");
        }
        _ => panic!("the review never finished"),
    }
}
