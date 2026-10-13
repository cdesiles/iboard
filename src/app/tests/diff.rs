//! The Diff screen's tests: from the commits popup to a find in the diff.

use egui_kittest::kittest::Queryable;

use super::*;

/// A repository whose last commit closes `done/fixed.md`, changing
/// `src/lib.rs`; its board, the issue, and that commit's short hash.
fn closed_issue(tag: &str) -> (Scratch, PathBuf, String) {
    let dir = scratch_dir(tag);
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&*dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let issue = done.join("fixed.md");
    std::fs::write(&issue, "# Fixed\n").unwrap();
    std::fs::write(dir.join("src/lib.rs"), "fn old() {}\n").unwrap();
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "Start"]);
    std::fs::write(
        dir.join("src/lib.rs"),
        "fn old() {}\nfn needle() {}\nfn needle_again() {}\n",
    )
    .unwrap();
    git(&["commit", "-q", "-am", "Fix it\n\ncloses fixed.md"]);
    let hash = git(&["rev-parse", "--short", "HEAD"]);
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
    assert!(matches!(&harness.state().1.session.screen, Screen::Detail(path) if *path == issue));
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
        .read_response(egui::Id::new(("card_diff", &issue)))
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
