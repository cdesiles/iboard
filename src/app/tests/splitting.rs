//! Splitting a reviewed issue: what the panel offers when the split fails.

use super::review::open_issue_with_review;
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
    use egui_kittest::kittest::{NodeT, Queryable};
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
