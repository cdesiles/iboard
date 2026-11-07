//! Splitting a reviewed issue: the tickets that take its place, and what the
//! panel offers when the split fails.

use egui_kittest::kittest::{NodeT, Queryable};

use super::*;
use crate::app::reader::REVIEW_PANEL_WIDTH;
use crate::app::split::Split;

fn split_of(app: &IboardApp) -> Option<&Split> {
    app.session.review.as_ref().unwrap().split.as_ref()
}

/// A failed split keeps the review it came from: Retry splits again, Back to
/// review drops the failure, Apply takes the kept proposal, and Ask again
/// quotes the kept review.
#[test]
fn a_failed_split_keeps_its_review() {
    let (_dir, file, mut app) = open_issue_with_review("split-failed");
    app.prefs.agent = fake_agent("exit 3 # {prompt}");
    app.start_split();
    assert!(until(|| {
        app.poll_split();
        matches!(split_of(&app), Some(Split::Failed(_)))
    }));
    assert!(app.proposal_shown(), "the review's proposal still applies");

    let mut harness = kittest(app);
    harness.run_steps(1);
    let panel_left = window().width() - REVIEW_PANEL_WIDTH;
    for label in [
        "Apply to draft",
        "Retry",
        "Back to review",
        "Ask again",
        "Dismiss",
    ] {
        let rect = harness.get_by_label(label).rect();
        assert!(rect.min.x >= panel_left, "{label} {rect:?} fits the panel");
    }
    assert!(
        !harness
            .get_by_label("Apply to draft")
            .accesskit_node()
            .is_disabled()
    );

    harness.state_mut().1.prefs.agent = fake_agent("sleep 30 # {prompt}");
    harness.get_by_label("Retry").click();
    harness.run_steps(1);
    assert!(matches!(
        split_of(&harness.state().1),
        Some(Split::Cutting { .. })
    ));

    harness.state_mut().1.session.review.as_mut().unwrap().split =
        Some(Split::Failed("no tickets".to_string()));
    harness.run_steps(1);
    harness.get_by_label("Back to review").click();
    harness.run_steps(2);
    assert!(split_of(&harness.state().1).is_none());
    harness.get_by_label("RELEVANCE");

    let app = &mut harness.state_mut().1;
    app.session.review.as_mut().unwrap().split = Some(Split::Failed("no tickets".to_string()));
    app.session.review.as_mut().unwrap().follow_up = Some("Smaller".to_string());
    app.prefs.agent = fake_agent(
        "grep -q 'previous review said' {prompt} && printf 'RELEVANCE: Pass\\nquoted\\nSIZE: Pass\\nok\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE' # {prompt}",
    );
    app.ask_again();
    assert!(until(|| {
        app.poll_review();
        !app.review_is_running(&file)
    }));
    assert!(
        app.past_reviews[&file].raw.contains("quoted"),
        "{:?}",
        app.error
    );
}

/// A Size finding that blocks offers a split: the agent's tickets take the
/// original's place in Todo, the original is archived naming them, and one
/// `⌘Z` takes it all back.
#[test]
fn a_size_block_splits_the_issue_in_place() {
    let (dir, mut app) = project(
        "split",
        &[
            ("todo/001-first.md", "# First\n"),
            ("todo/002-big.md", "# Big\n"),
            ("todo/003-last.md", "# Last\n"),
        ],
    );
    let todo = dir.join("issues/todo");
    let big = todo.join("002-big.md");
    app.prefs.agent = fake_agent(
        "if grep -q '=== TICKET ===' {prompt}; then \
         printf '=== TICKET ===\\n# [refactor] Part A\\nDo a.\\n=== TICKET ===\\n# [refactor] Part B\\nDo b.\\n'; \
         else printf 'RELEVANCE: Pass\\nok\\nSIZE: Block\\ntoo big\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE'; fi",
    );
    app.open_issue(big.clone());
    app.start_review(&big, false);
    landed(&mut app, &big);
    app.show_past_review(big.clone());
    app.start_split();
    assert!(until(|| {
        app.poll_split();
        !matches!(split_of(&app), Some(Split::Cutting { .. }))
    }));
    let Some(Split::Ready(drafts)) = split_of(&app) else {
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
