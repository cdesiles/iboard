//! The Diff screen's tests: from the commits popup to a find in the diff.

use egui_kittest::kittest::Queryable;

use super::*;
use crate::scratch_git::git;

/// A repository whose last commit closes `done/fixed.md`, changing
/// `src/lib.rs`; its board, the issue, and that commit's short hash.
fn closed_issue(tag: &str) -> (Scratch, PathBuf, String) {
    let dir = scratch_dir(tag);
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let issue = done.join("fixed.md");
    std::fs::write(&issue, "# Fixed\n").unwrap();
    std::fs::write(dir.join("src/lib.rs"), "fn old() {}\n").unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "Start"]);
    std::fs::write(
        dir.join("src/lib.rs"),
        "fn old() {}\nfn needle() {}\nfn needle_again() {}\n",
    )
    .unwrap();
    git(&dir, &["commit", "-q", "-am", "Fix it\n\ncloses fixed.md"]);
    let hash = git(&dir, &["rev-parse", "--short", "HEAD"]);
    (dir, issue, hash)
}

/// The commits popup opens a commit's diff; `⌘F` finds in it, `esc` closes
/// the find bar, then goes back to the issue.
#[test]
fn a_commit_opens_as_a_diff_and_finds_a_term() {
    let (_dir, issue, hash) = closed_issue("diff");
    let mut app = IboardApp::from_located(discovery::locate(&issue).unwrap());
    app.open_issue(issue.clone());
    app.open_commits(&issue);
    let mut harness = kittest(app);
    harness.get_by_label(&format!("Diff of {hash}")).click();
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
            .is_some_and(|view| view.summary().contains("1 file"))
    }));
    let summary = harness.state().1.session.diff.as_ref().unwrap().summary();
    assert_eq!(summary, format!("{hash} · 1 file · +2 −0"));

    harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::F);
    harness.run_steps(2);
    harness
        .get_by_role(egui::accesskit::Role::TextInput)
        .type_text("NEEDLE");
    harness.run_steps(2);
    let count = |harness: &egui_kittest::Harness<(bool, IboardApp)>| {
        harness
            .state()
            .1
            .session
            .diff
            .as_ref()
            .unwrap()
            .match_count()
    };
    assert_eq!(
        count(&harness).as_deref(),
        Some("1/2"),
        "found, ignoring case"
    );
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(
        count(&harness).as_deref(),
        Some("2/2"),
        "↩ steps to the next"
    );

    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(count(&harness), None, "esc closes the find bar");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(matches!(&harness.state().1.session.screen, Screen::Detail(path, _) if *path == issue));
}

/// VoiceOver reads a card as a group named by its title, and the Diff
/// screen's file headers; a large file's collapsed run is a button that
/// shows it.
#[test]
fn voiceover_reads_the_card_and_the_diff() {
    use egui::accesskit::Role;

    let (dir, issue, _) = closed_issue("diff-voiceover");
    let big: String = (0..1600).map(|n| format!("line {n}\n")).collect();
    std::fs::write(dir.join("big.txt"), big).unwrap();
    git(&dir, &["add", "big.txt"]);
    git(&dir, &["commit", "-q", "-m", "Big\n\ncloses fixed.md"]);
    let hash = git(&dir, &["rev-parse", "--short", "HEAD"]);
    let mut harness = kittest(IboardApp::from_located(discovery::locate(&issue).unwrap()));
    harness.run_steps(2);
    harness.get_by_role_and_label(Role::Group, "Fixed");

    harness.state_mut().1.open_issue(issue.clone());
    harness.state_mut().1.open_diff(&issue, vec![hash]);
    assert!(until(|| {
        harness.run_steps(1);
        harness.query_by_label("big.txt  +1600 −0  added").is_some()
    }));
    harness
        .get_by_role_and_label(Role::Button, "1601 diff lines, collapsed: click to show")
        .click();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label("1601 diff lines, collapsed: click to show")
            .is_none()
    );
    harness.get_by_role_and_label(Role::Label, "added: line 0");
}

/// A Done card's diff action opens every commit that closed it.
#[test]
fn a_done_card_opens_its_diff() {
    let (_dir, issue, hash) = closed_issue("card-diff");
    let mut app = IboardApp::from_located(discovery::locate(&issue).unwrap());
    app.session.selected = Some(issue.clone());
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let at = ctx
        .read_response(egui::Id::unique(("card_diff", &issue)))
        .expect("Done cards offer their diff")
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(matches!(app.session.screen, Screen::Diff));
    assert!(until(|| {
        app.session.diff.as_mut().unwrap().poll();
        app.session.diff.as_ref().unwrap().summary() == format!("{hash} · 1 file · +2 −0")
    }));
}

/// Tickets kept in another repository, `issues/` a symlink to it: the
/// commits that close one are still found in the code's repository.
#[cfg(unix)]
#[test]
fn an_issue_kept_elsewhere_finds_the_commits_of_its_project() {
    let scratch = scratch_dir("diff-symlink");
    let (code, tickets) = (scratch.join("code"), scratch.join("tickets"));
    std::fs::create_dir_all(tickets.join("done")).unwrap();
    std::fs::write(tickets.join("done/fixed.md"), "# Fixed\n").unwrap();
    git(&tickets, &["init", "-q"]);
    git(&tickets, &["add", "."]);
    git(&tickets, &["commit", "-q", "-m", "Tickets"]);
    std::fs::create_dir_all(code.join("src")).unwrap();
    std::fs::write(code.join("src/lib.rs"), "fn old() {}\n").unwrap();
    git(&code, &["init", "-q"]);
    git(&code, &["add", "."]);
    git(&code, &["commit", "-q", "-m", "Start"]);
    std::fs::write(code.join("src/lib.rs"), "fn new() {}\n").unwrap();
    git(&code, &["commit", "-q", "-am", "Fix it\n\ncloses fixed.md"]);
    let hash = git(&code, &["rev-parse", "--short", "HEAD"]);
    std::os::unix::fs::symlink(&tickets, code.join("issues")).unwrap();

    let mut app = IboardApp::from_located(discovery::locate(&code).unwrap());
    let issue = code.join("issues/done/fixed.md");
    assert!(app.board.find(&issue).is_some());
    app.open_commits(&issue);
    let list = &app.session.commits.as_ref().unwrap().list;
    assert_eq!(list.len(), 1, "{list:?}");
    assert!(
        list[0].hash.starts_with(&hash),
        "{} vs {hash}",
        list[0].hash
    );
}
