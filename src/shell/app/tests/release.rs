//! Done's Release flag: its place in the header, and the note, commit, tag
//! and push the one release panel makes.

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use super::super::release::ReleaseChoice;
use super::*;
use crate::scratch_git::git;

/// A git project with one Done issue and a bare `origin`, so push runs
/// somewhere real.
fn release_project(tag: &str) -> (Scratch, Scratch, IboardApp) {
    let dir = scratch_dir("release");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    std::fs::write(done.join("a.md"), "# Alpha\n").unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "t"]);
    git(&dir, &["config", "user.email", "t@t"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "first"]);
    if !tag.is_empty() {
        git(&dir, &["tag", tag]);
    }
    git(
        &dir,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "[feat] [board] Add a flag",
        ],
    );
    let remote = scratch_dir("release-remote");
    git(&remote, &["init", "-q", "--bare"]);
    git(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
    git(&dir, &["push", "-q", "-u", "origin", "main"]);
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    (dir, remote, app)
}

#[test]
fn releasing_writes_a_note_commits_tags_and_pushes() {
    let (dir, remote, mut app) = release_project("v0.1.0");

    app.open_release();
    let flow = app.session.release.as_mut().unwrap();
    assert_eq!(flow.tag, "v0.2.0", "a feature bumps the minor after v0.1.0");
    assert_eq!(
        flow.body, "### New\n#### Board\n- Add a flag\n",
        "the commits since the tag become the note"
    );
    flow.body = "- Alpha\n- Beta\n".to_string();

    confirm_and_wait(&mut app);
    let note = std::fs::read_to_string(dir.join("CHANGELOG.md")).unwrap();
    assert!(note.starts_with("# Changelog\n\n## v0.2.0 — "), "{note}");
    assert!(note.contains("- Alpha\n- Beta\n"), "{note}");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]), "Release v0.2.0");
    assert_eq!(git(&dir, &["tag", "--list", "v0.2.0"]), "v0.2.0");
    assert!(app.session.release.is_none(), "one panel, one release");
    assert_eq!(
        app.session.notice.as_ref().map(|(text, _)| text.as_str()),
        Some("Released v0.2.0")
    );
    assert_eq!(
        git(&remote, &["tag", "--list", "v0.2.0"]),
        "v0.2.0",
        "the tag reached origin"
    );
    assert_eq!(
        git(&remote, &["log", "-1", "--format=%s", "main"]),
        "Release v0.2.0"
    );
}

#[test]
fn tag_only_leaves_the_note_alone_and_keep_local_leaves_the_push_out() {
    let (dir, remote, mut app) = release_project("v0.1.0");
    app.open_release();
    let flow = app.session.release.as_mut().unwrap();
    flow.note = false;
    flow.push = false;
    confirm_and_wait(&mut app);

    assert!(!dir.join("CHANGELOG.md").exists(), "no note was written");
    assert_eq!(
        git(&dir, &["log", "-1", "--format=%s"]),
        "[feat] [board] Add a flag",
        "tag only adds no commit"
    );
    assert_eq!(git(&dir, &["tag", "--list", "v0.2.0"]), "v0.2.0");
    assert!(app.session.release.is_none());
    assert_eq!(
        git(&remote, &["tag", "--list", "v0.2.0"]),
        "",
        "the push was left out"
    );
}

#[test]
fn a_written_note_goes_to_the_file_the_user_chose() {
    let (dir, _remote, mut app) = release_project("v0.1.0");
    app.open_release();
    app.session.release.as_mut().unwrap().file = "release-notes.md".to_string();
    confirm_and_wait(&mut app);

    assert!(!dir.join("CHANGELOG.md").exists());
    let note = std::fs::read_to_string(dir.join("release-notes.md")).unwrap();
    assert!(note.starts_with("# Changelog\n\n## v0.2.0 — "), "{note}");
    assert_eq!(
        git(&dir, &["show", "--name-only", "--format=", "HEAD"]),
        "release-notes.md"
    );
}

#[test]
fn cancelling_stops_the_release_before_anything_is_written() {
    let (dir, _remote, mut app) = release_project("v0.1.0");
    app.open_release();
    app.release_choice(ReleaseChoice::Cancel);

    assert!(app.session.release.is_none());
    assert!(!dir.join("CHANGELOG.md").exists());
    assert_eq!(git(&dir, &["tag", "--list", "v0.2.0"]), "");
}

#[test]
fn a_tag_git_would_refuse_stays_on_the_panel() {
    let (dir, _remote, mut app) = release_project("v0.1.0");
    app.open_release();
    app.session.release.as_mut().unwrap().tag = "two words".to_string();
    confirm_and_wait(&mut app);

    let flow = app.session.release.as_ref().unwrap();
    assert!(flow.error.is_some(), "the dialog says what is wrong");
    assert!(!dir.join("CHANGELOG.md").exists());
    assert_eq!(git(&dir, &["tag", "--list"]), "v0.1.0");
}

#[test]
fn a_note_file_outside_the_project_is_refused() {
    let (dir, _remote, mut app) = release_project("v0.1.0");
    app.open_release();
    app.session.release.as_mut().unwrap().file = "../NOTES.md".to_string();
    confirm_and_wait(&mut app);

    let flow = app.session.release.as_ref().unwrap();
    assert!(flow.error.is_some());
    assert_eq!(git(&dir, &["tag", "--list"]), "v0.1.0");
}

#[test]
fn hovering_done_offers_release_left_of_archive_all() {
    let (dir, mut app) = project("release-head", &[("done/a.md", "# A\n"), ("todo/", "")]);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "first"]);
    let index = app
        .board
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("done"))
        .unwrap();
    let release = egui::Id::unique(("release", index));
    let archive_all = egui::Id::unique(("archive_all", index));

    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    assert!(ctx.read_response(release).is_none(), "hidden at rest");
    let column = ctx
        .read_response(egui::Id::unique(("column_drop", index)))
        .unwrap()
        .rect;
    let head = egui::pos2(column.center().x, column.min.y + theme::CONTROL_H / 2.0);
    draw(&ctx, &mut app, vec![moved(head)]);

    let release_rect = ctx.read_response(release).expect("shown on hover").rect;
    let archive_rect = ctx
        .read_response(archive_all)
        .expect("archive all stays")
        .rect;
    assert!(
        release_rect.right() <= archive_rect.left(),
        "the flag sits left of Archive all"
    );

    let at = release_rect.center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(app.session.release.is_some(), "the flag opens the flow");
    assert!(!dir.join("CHANGELOG.md").exists(), "nothing yet");
}

#[test]
fn an_empty_done_column_still_releases_commits_since_the_last_tag() {
    let (dir, mut app) = project("release-empty", &[("done/", "")]);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "first"]);
    git(&dir, &["tag", "v0.1.0"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "after"]);
    let index = app
        .board
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("done"))
        .unwrap();
    assert!(app.board.columns[index].issues.is_empty());
    let release = egui::Id::unique(("release", index));

    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let column = ctx
        .read_response(egui::Id::unique(("column_drop", index)))
        .unwrap()
        .rect;
    let head = egui::pos2(column.center().x, column.min.y + theme::CONTROL_H / 2.0);
    draw(&ctx, &mut app, vec![moved(head)]);

    let response = ctx
        .read_response(release)
        .expect("the release action stays in Done");
    assert!(response.enabled(), "a commit waits for release");

    let at = response.rect.center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(
        app.session.release.is_some(),
        "the empty column can release the commit"
    );
}

#[test]
fn an_empty_done_column_without_commits_keeps_release_disabled() {
    let (dir, mut app) = project("release-empty-tagged", &[("done/", "")]);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "first"]);
    git(&dir, &["tag", "v0.1.0"]);
    let index = app
        .board
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("done"))
        .unwrap();
    let release = egui::Id::unique(("release", index));

    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let column = ctx
        .read_response(egui::Id::unique(("column_drop", index)))
        .unwrap()
        .rect;
    let head = egui::pos2(column.center().x, column.min.y + theme::CONTROL_H / 2.0);
    draw(&ctx, &mut app, vec![moved(head)]);

    let response = ctx
        .read_response(release)
        .expect("the release action is present in Done");
    assert!(!response.enabled(), "nothing landed since the last release");

    let at = response.rect.center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(
        app.session.release.is_none(),
        "a disabled flag does nothing"
    );
}

#[test]
fn escape_cancels_the_release_flow() {
    let (_dir, mut app) = project("release-esc", &[("done/a.md", "# Alpha\n")]);
    app.open_release();
    let mut harness = kittest(app);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(harness.state().1.session.release.is_none());
}

#[test]
fn the_release_panel_wording_follows_its_selectors() {
    let (_dir, mut app) = project("release-ui", &[("done/a.md", "# Alpha\n")]);
    app.open_release();
    let mut harness = kittest(app);
    harness.get_by_role_and_label(Role::Dialog, "Release");

    harness.get_by_label("Update release note, version and push");
    harness.get_by_label("Note file");

    press(&mut harness, "Tag only");
    harness.get_by_label("Set and push the release tag");
    assert!(
        harness.query_by_label("Note file").is_none(),
        "the note file goes with the note"
    );

    press(&mut harness, "Keep it local");
    harness.get_by_label("Set the release tag");

    press(&mut harness, "Update the release note");
    harness.get_by_label("Update release note and version");

    press(&mut harness, "Cancel");
    assert!(harness.state().1.session.release.is_none());
}

#[test]
fn the_panel_makes_the_whole_release_in_one_confirmation() {
    let (dir, remote, mut app) = release_project("v0.1.0");
    app.open_release();
    let mut harness = kittest(app);

    press(&mut harness, "Update release note, version and push");
    let started = Instant::now();
    while harness.state().1.session.release.is_some() {
        assert!(started.elapsed().as_secs() < 30, "the release never ended");
        harness.run_steps(1);
    }
    assert!(dir.join("CHANGELOG.md").is_file());
    assert_eq!(git(&dir, &["tag", "--list", "v0.2.0"]), "v0.2.0");
    assert_eq!(git(&remote, &["tag", "--list", "v0.2.0"]), "v0.2.0");
    assert!(harness.state().1.session.release.is_none());
}

#[test]
fn the_choices_made_are_kept_in_the_settings_file() {
    let (dir, mut app) = project("release-remember", &[("done/a.md", "# Alpha\n")]);
    let config = dir.join("iboard.toml");
    app.config_file = Some(config.clone());
    app.open_release();
    let mut harness = kittest(app);

    press(&mut harness, "Tag only");
    press(&mut harness, "Keep it local");
    press(&mut harness, "Cancel");

    let mut user = UserConfig::load(Some(&config));
    let saved = user
        .section::<crate::release::ReleaseSection>("release")
        .choices()
        .0;
    assert!(!saved.note, "tag only is remembered");
    assert!(!saved.push, "the local choice is remembered");

    let mut app = harness.into_state().1;
    app.open_release();
    let flow = app.session.release.as_ref().unwrap();
    assert!(!flow.note);
    assert!(!flow.push);
    assert_eq!(flow.file, "CHANGELOG.md");
}

#[test]
fn the_note_lists_only_the_commits_since_the_last_release() {
    let dir = scratch_dir("release-window");
    std::fs::create_dir_all(dir.join("issues")).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "old"]);
    git(&dir, &["tag", "v0.3.0"]);
    git(
        &dir,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "[feat] [board] Add a flag",
        ],
    );

    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_release();
    let flow = app.session.release.as_ref().unwrap();
    assert_eq!(flow.tag, "v0.4.0", "a feature after v0.3.0");
    assert_eq!(
        flow.body, "### New\n#### Board\n- Add a flag\n",
        "the old release's commits stay out"
    );
}

#[test]
fn a_long_note_scrolls_so_the_buttons_stay_on_screen() {
    let dir = scratch_dir("release-long");
    std::fs::create_dir_all(dir.join("issues")).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    for n in 0..40 {
        git(
            &dir,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("[bug] [board] Issue {n}"),
            ],
        );
    }
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_release();
    let harness = kittest(app);

    let confirm = harness.get_by_label("Update release note, version and push");
    assert!(
        confirm.rect().bottom() <= window().bottom(),
        "the note scrolls, its buttons stay put: {:?}",
        confirm.rect()
    );
}

/// Confirms the panel and waits for the release its thread makes.
fn confirm_and_wait(app: &mut IboardApp) {
    app.release_choice(ReleaseChoice::Confirm);
    assert!(
        until(|| {
            app.poll_release();
            app.session
                .release
                .as_ref()
                .is_none_or(|flow| !flow.running())
        }),
        "the release never ended"
    );
}

/// A git project with one commit behind an optional tag, then `subjects`.
fn labelled_project(name: &str, tag: Option<&str>, subjects: &[&str]) -> (Scratch, IboardApp) {
    let dir = scratch_dir(name);
    std::fs::create_dir_all(dir.join("issues")).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "old"]);
    if let Some(tag) = tag {
        git(&dir, &["tag", tag]);
    }
    for subject in subjects {
        git(&dir, &["commit", "-q", "--allow-empty", "-m", subject]);
    }
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    (dir, app)
}

#[test]
fn the_technical_changes_stay_out_until_their_box_is_ticked() {
    let (_dir, mut app) = labelled_project(
        "release-technical",
        None,
        &[
            "[refactor] [board] Split the module",
            "[feat] [board] Add a flag",
        ],
    );
    app.open_release();
    let flow = app.session.release.as_ref().unwrap();
    assert!(!flow.technical, "the box starts unticked");
    assert!(!flow.body.contains("Technical"), "{}", flow.body);

    let mut harness = kittest(app);
    press(&mut harness, "Include technical changes (1)");
    let flow = harness.state().1.session.release.as_ref().unwrap();
    assert!(
        flow.body.ends_with("### Technical\n- Split the module\n"),
        "{}",
        flow.body
    );
}

#[test]
fn the_panel_counts_the_commits_the_note_leaves_out() {
    let (_dir, mut app) = labelled_project(
        "release-unclassified",
        None,
        &[
            "Some chore",
            "[ux] Column actions name themselves",
            "[feat] [board] Add a flag",
        ],
    );
    app.open_release();
    let flow = app.session.release.as_ref().unwrap();
    assert_eq!(flow.unclassified, 3, "the untagged 'old' commit counts too");
    let harness = kittest(app);
    harness.get_by_value("3 commits without a kind won't appear.");
}

#[test]
fn the_bump_follows_the_features_and_yields_to_a_click() {
    let (_dir, mut app) = labelled_project(
        "release-bump",
        Some("v0.2.0"),
        &["[feat] [board] Add a flag"],
    );
    app.open_release();
    assert_eq!(app.session.release.as_ref().unwrap().tag, "v0.3.0");

    let mut harness = kittest(app);
    harness.get_by_label("1 feature since v0.2.0, so a minor.");
    press(&mut harness, "Patch");
    let flow = harness.state().1.session.release.as_ref().unwrap();
    assert_eq!(flow.tag, "v0.2.1", "the tag follows the click");
    harness.get_by_label("You chose a patch, though 1 feature shipped since v0.2.0.");
}

#[test]
fn the_opening_is_written_only_when_its_box_is_ticked() {
    let (_dir, mut app) =
        labelled_project("release-preamble", None, &["[feat] [board] Add a flag"]);
    app.prefs.agent = fake_agent("printf 'A flag arrives.\\n' # {prompt}");
    app.open_release();
    let flow = app.session.release.as_ref().unwrap();
    assert!(flow.lead.is_empty(), "nothing runs while the box is off");

    let mut harness = kittest(app);
    press(&mut harness, "Open with a sentence");
    let started = Instant::now();
    loop {
        let flow = harness.state().1.session.release.as_ref().unwrap();
        if !flow.lead.is_empty() {
            break;
        }
        assert!(started.elapsed().as_secs() < 30, "the opening never landed");
        harness.run_steps(1);
    }
    let flow = harness.state().1.session.release.as_ref().unwrap();
    assert_eq!(flow.lead, "A flag arrives.");
    assert!(
        flow.section()
            .contains("A flag arrives.\n\n### New\n#### Board\n- Add a flag"),
        "{}",
        flow.section()
    );
}

#[test]
fn reopening_with_the_box_ticked_writes_the_opening_again() {
    let (_dir, mut app) = labelled_project(
        "release-preamble-reopen",
        None,
        &["[feat] [board] Add a flag"],
    );
    app.prefs.agent = fake_agent("printf 'A flag arrives.\\n' # {prompt}");
    app.prefs.release.preamble = true;
    app.open_release();

    let mut harness = kittest(app);
    let started = Instant::now();
    loop {
        let flow = harness.state().1.session.release.as_ref().unwrap();
        if !flow.lead.is_empty() {
            break;
        }
        assert!(started.elapsed().as_secs() < 30, "the opening never landed");
        harness.run_steps(1);
    }
    let flow = harness.state().1.session.release.as_ref().unwrap();
    assert_eq!(flow.lead, "A flag arrives.", "an open panel writes it too");
}
