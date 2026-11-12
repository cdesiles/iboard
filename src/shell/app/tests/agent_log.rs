//! The agent log's timeline: runs, folded activity and a run's changes.

use egui_kittest::kittest::Queryable;

use super::*;
use crate::build_log;
use crate::scratch_git::git;
use crate::shell::app::intent::Intent;

const LOG: &str = "\
── 2026-11-07 14:02 · opencode
Let me look around.
→ Reading src/a.rs…
→ Reading src/b.rs…
→ Searching todo…
→ Using edit src/a.rs…
That is the fix.
→ Reading src/c.rs…
✓ Finished in 03:12
";

/// A service build's log as it was once written: no arrows, and each call
/// said twice, bare then with its target.
const UNMARKED: &str = "\
── 2026-11-07 14:02 · opencode
Running…
Running git status…
Using edit…
Using edit src/a.rs…
Running…
Running cargo test…
";

/// A project with one Doing issue, a dirty `src/a.rs`, and the log above.
fn logged(tag: &str) -> (Scratch, IboardApp, PathBuf) {
    let dir = scratch_dir(tag);
    for column in ["todo", "ready", "doing", "done"] {
        std::fs::create_dir_all(dir.join("issues").join(column)).unwrap();
    }
    let issue = dir.join("issues/doing/fix.md");
    std::fs::write(&issue, "# Fix\n").unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/a.rs"), "one\ntwo\n").unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "Start"]);
    std::fs::write(dir.join("src/a.rs"), "one\nthree\n").unwrap();

    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let log = build_log::path(&app.board.root, &issue);
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, LOG).unwrap();
    (dir, app, issue)
}

/// Steps logged without their arrow fold into one row around the edit, which
/// lands in the run's changes: no step reads as the agent's prose.
#[test]
fn an_unmarked_log_folds_like_a_marked_one() {
    let (_dir, mut app, issue) = logged("agent-log-unmarked");
    std::fs::write(build_log::path(&app.board.root, &issue), UNMARKED).unwrap();
    app.apply(vec![Intent::AgentLog(issue)]);
    let mut harness = kittest(app);
    harness.run_steps(2);

    harness.get_by_label("Ran 2 commands, collapsed");
    assert!(harness.query_by_label_contains("Running").is_none());
    assert!(until(|| {
        harness.run_steps(1);
        harness
            .query_by_label("1 file changed, +1 −1, collapsed")
            .is_some()
    }));
}

/// A run reads as a head, one folded activity row, a lone step past the
/// message, its changes, and its outcome; the changes open the Diff screen.
#[test]
fn a_run_reads_as_a_timeline_with_its_changes() {
    let (_dir, mut app, issue) = logged("agent-log-timeline");
    app.apply(vec![Intent::AgentLog(issue.clone())]);
    let mut harness = kittest(app);
    harness.run_steps(2);

    assert!(harness.query_by_label("Run 1").is_some());
    assert!(
        harness.query_by_label("src/c.rs").is_some(),
        "a lone step stays a row, not a disclosure"
    );
    harness
        .get_by_label("Read 2 files, searched 1, collapsed")
        .click();
    harness.run_steps(2);
    harness.get_by_label("Read 2 files, searched 1, expanded");
    harness.get_by_label("src/b.rs");

    assert!(until(|| {
        harness.run_steps(1);
        harness
            .query_by_label("1 file changed, +1 −1, collapsed")
            .is_some()
    }));
    harness
        .get_by_label("1 file changed, +1 −1, collapsed")
        .click();
    harness.run_steps(2);
    harness
        .get_by_label("Open the diff of src/a.rs, +1 −1")
        .click();
    harness.run_steps(2);
    assert!(matches!(harness.state().1.session.screen, Screen::Diff));
    assert!(until(|| {
        harness.run_steps(1);
        harness
            .state()
            .1
            .session
            .diff
            .as_ref()
            .is_some_and(|view| view.summary() == "working tree · 1 file · +1 −1")
    }));
}

/// While a run is live its last step is the Now strip, not the fold; once it
/// ends the step returns to the timeline.
#[test]
fn the_live_step_moves_from_now_to_the_timeline() {
    let dir = scratch_dir("agent-log-now");
    for column in ["todo", "ready", "doing", "done"] {
        std::fs::create_dir_all(dir.join("issues").join(column)).unwrap();
    }
    let issue = dir.join("issues/ready/fix.md");
    std::fs::write(&issue, "# Fix\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let step =
        r#"{"type":"tool_use","part":{"tool":"read","state":{"input":{"path":"src/a.rs"}}}}"#;
    app.prefs.build_agent = fake_agent(&format!("printf '%s\\n' '{step}'; sleep 30 # {{prompt}}"));

    app.apply(vec![Intent::Build(vec![issue])]);
    let taken = app.building_now()[0].0.clone();
    let log = build_log::path(&app.board.root, &taken);
    app.apply(vec![Intent::AgentLog(taken)]);
    let mut harness = kittest(app);
    assert!(until(|| {
        harness.run_steps(1);
        harness.query_by_label("Reading src/a.rs").is_some()
    }));
    assert!(
        harness.query_by_label("src/a.rs").is_none(),
        "the Now strip owns the live step"
    );

    harness.state_mut().1.stop_build();
    assert!(until(|| {
        std::fs::read_to_string(&log).ok().is_some_and(|text| {
            build_log::parse(&text)
                .iter()
                .any(|entry| matches!(entry, build_log::Entry::Outcome { .. }))
        })
    }));
    harness.run_steps(2);
    assert!(harness.query_by_label("Reading src/a.rs").is_none());
    assert!(
        harness.query_by_label("src/a.rs").is_some(),
        "the step returns to the timeline"
    );
}

/// A run's thoughts fold with the steps around them and open onto their text.
#[test]
fn thoughts_fold_with_the_steps_and_open_onto_their_text() {
    let (_dir, mut app, issue) = logged("agent-log-thoughts");
    let thoughts = "\
── 2026-11-07 14:02 · opencode
∴ 4s · Weighing the fix.
→ Reading src/a.rs…
∴ 7s · The tag check lives here.
→ Reading src/b.rs…
✓ Finished in 00:10
";
    std::fs::write(build_log::path(&app.board.root, &issue), thoughts).unwrap();
    app.apply(vec![Intent::AgentLog(issue)]);
    let mut harness = kittest(app);
    harness.run_steps(2);

    harness
        .get_by_label("Thought 2 times, read 2 files, collapsed")
        .click();
    harness.run_steps(2);
    harness.get_by_label("Thought 2 times, read 2 files, expanded");
    assert!(
        harness.query_by_label("Weighing the fix.").is_none(),
        "closed"
    );
    harness
        .get_by_label("Thinking, 4 seconds, collapsed")
        .click();
    harness.run_steps(2);
    harness.get_by_label("Thinking, 4 seconds, expanded");
    harness.get_by_label("Weighing the fix.");
}

/// While the run is live its newest thought owns the Now strip, with no
/// object; once it ends the thought returns to the fold.
#[test]
fn the_now_strip_says_thinking_for_a_live_thought() {
    let dir = scratch_dir("agent-log-thinking");
    for column in ["todo", "ready", "doing", "done"] {
        std::fs::create_dir_all(dir.join("issues").join(column)).unwrap();
    }
    let issue = dir.join("issues/ready/fix.md");
    std::fs::write(&issue, "# Fix\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let thought = r#"{"type":"reasoning","part":{"type":"reasoning","text":"Weighing the fix.","time":{"created":1000,"completed":4000}}}"#;
    app.prefs.build_agent = fake_agent(&format!(
        "printf '%s\\n' '{thought}'; sleep 30 # {{prompt}}"
    ));

    app.apply(vec![Intent::Build(vec![issue])]);
    let taken = app.building_now()[0].0.clone();
    let log = build_log::path(&app.board.root, &taken);
    app.apply(vec![Intent::AgentLog(taken)]);
    let mut harness = kittest(app);
    assert!(until(|| {
        harness.run_steps(1);
        harness.query_by_label("Thinking").is_some()
    }));
    assert!(
        harness.query_by_label("Weighing the fix.").is_none(),
        "the Now strip owns the live thought"
    );

    harness.state_mut().1.stop_build();
    assert!(until(|| {
        std::fs::read_to_string(&log).ok().is_some_and(|text| {
            build_log::parse(&text)
                .iter()
                .any(|entry| matches!(entry, build_log::Entry::Outcome { .. }))
        })
    }));
    harness.run_steps(2);
    assert!(harness.query_by_label("Thinking").is_none());
    harness.get_by_label("Thinking, 3 seconds, collapsed");
}

/// The switch and the foot keep the panel's `space-4` sides, the reply is as
/// wide as the foot, and Send stays inside the window.
#[test]
fn the_switch_and_the_composer_sit_inside_the_panel() {
    let (_dir, mut app, issue) = logged("agent-log-chrome");
    app.apply(vec![Intent::AgentLog(issue)]);
    let mut harness = kittest(app);
    harness.run_steps(3);

    let title = harness.get_by_label("Agent log of fix.md").rect();
    let switch = harness.get_by_label("Timeline").rect();
    let reply = harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .rect();
    let hint = harness.get_by_label("⌘↩ sends").rect();
    let send = harness.get_by_label("Send").rect();
    let screen = harness.ctx.content_rect();
    let near = |left: f32, right: f32| (left - right).abs() < 0.5;

    assert!(near(switch.left() - 2.0, title.left()), "{switch:?}");
    assert!(near(reply.left(), title.left()), "{reply:?}");
    assert!(near(hint.left(), title.left()), "{hint:?}");
    assert!(near(reply.right(), send.right()), "{reply:?} {send:?}");
    assert!(reply.height() >= 56.0, "{reply:?}");
    assert!(send.bottom() < screen.bottom(), "{send:?}");
}
