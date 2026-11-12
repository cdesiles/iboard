//! The Reader's notice when the issue it shows has moved or is gone.

use egui_kittest::kittest::Queryable;

use super::*;

fn gone(tag: &str, from: &str, to: Option<&str>, log: bool) -> (Scratch, PathBuf, PathBuf, Kit) {
    let (dir, mut app) = project(
        tag,
        &[
            (from, "# Demo\n"),
            ("done/", ""),
            ("ready/", ""),
            ("archive/", ""),
        ],
    );
    let old = dir.join("issues").join(from);
    app.open_issue(old.clone());
    if log {
        app.toggle_agent_log(&old);
    }
    let new = match to {
        Some(to) => dir.join("issues").join(to),
        None => {
            std::fs::remove_file(&old).unwrap();
            old.clone()
        }
    };
    if to.is_some() {
        std::fs::rename(&old, &new).unwrap();
    }
    app.refresh();
    (dir, old, new, kittest(app))
}

#[test]
fn an_issue_moved_to_done_says_so_and_opens_there_with_its_log() {
    let (_dir, _old, new, mut harness) = gone(
        "gone-done",
        "doing/01-demo.md",
        Some("done/01-demo.md"),
        true,
    );
    harness.run_steps(2);
    harness.get_by_role_and_label(egui::Role::Section, "Moved to Done");
    harness.get_by_label("Its agent log is still open beside it.");
    assert_eq!(
        harness.query_all_by_role(egui::Role::Image).count(),
        1,
        "the Done picture"
    );
    assert!(
        harness.get_by_label("Open in Done").is_focused(),
        "the primary takes focus when the notice opens"
    );

    press(&mut harness, "Open in Done");

    let app = &harness.state().1;
    assert!(
        matches!(&app.session.screen, Screen::Detail(path, _) if path == &new),
        "the Reader follows it"
    );
    assert!(app.agent_log_open(&new), "the log comes along");
}

#[test]
fn a_deleted_issue_offers_only_the_way_back() {
    let (_dir, _old, _new, mut harness) = gone("gone-deleted", "todo/01-demo.md", None, false);
    harness.run_steps(2);
    harness.get_by_role_and_label(egui::Role::Section, "Issue not found");
    assert!(
        harness.query_by_label("Open in Done").is_none(),
        "nothing to open"
    );
    let back = harness.get_by_label("Back to board");
    assert!(back.is_focused(), "the only button takes focus");

    press(&mut harness, "Back to board");

    assert!(matches!(harness.state().1.session.screen, Screen::Board));
}

#[test]
fn a_move_to_ready_has_no_picture() {
    let (_dir, _old, _new, mut harness) = gone(
        "gone-ready",
        "todo/01-demo.md",
        Some("ready/01-demo.md"),
        false,
    );
    harness.run_steps(2);
    harness.get_by_label("Moved to Ready");
    assert_eq!(
        harness.query_all_by_role(egui::Role::Image).count(),
        0,
        "only Done shows a picture"
    );
}

#[test]
fn the_notice_leaves_when_the_file_comes_back() {
    let (dir, old, _new, mut harness) = gone(
        "gone-back",
        "todo/01-demo.md",
        Some("done/01-demo.md"),
        false,
    );
    harness.run_steps(2);
    harness.get_by_label("Moved to Done");

    std::fs::rename(dir.join("issues/done/01-demo.md"), &old).unwrap();
    harness.state_mut().1.refresh();
    harness.run_steps(2);

    assert!(harness.query_by_label("Moved to Done").is_none());
    harness.get_by_label("Demo");
}
