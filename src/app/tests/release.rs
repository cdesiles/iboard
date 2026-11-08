//! Done's Release flag: its place in the header, and the note, commit, tag
//! and push the one release panel makes.

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};

use super::super::release::ReleaseChoice;
use super::*;
use crate::scratch_git::{git, git_env};

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
    let file = dir.join("issues/done/a.md");

    app.open_release(&[file]);
    let flow = app.session.release.as_mut().unwrap();
    assert_eq!(flow.tag, "v0.1.1", "the next patch after the newest tag");
    assert_eq!(flow.body, "- Alpha\n", "the Done issues become the note");
    flow.body = "- Alpha\n- Beta\n".to_string();

    app.release_choice(ReleaseChoice::Confirm);
    let note = std::fs::read_to_string(dir.join("CHANGELOG.md")).unwrap();
    assert!(note.starts_with("# Changelog\n\n## v0.1.1 — "), "{note}");
    assert!(note.contains("- Alpha\n- Beta\n"), "{note}");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]), "Release v0.1.1");
    assert_eq!(git(&dir, &["tag", "--list", "v0.1.1"]), "v0.1.1");
    assert!(app.session.release.is_none(), "one panel, one release");
    assert_eq!(
        app.session.notice.as_ref().map(|(text, _)| text.as_str()),
        Some("Released v0.1.1")
    );
    assert_eq!(
        git(&remote, &["tag", "--list", "v0.1.1"]),
        "v0.1.1",
        "the tag reached origin"
    );
    assert_eq!(
        git(&remote, &["log", "-1", "--format=%s", "main"]),
        "Release v0.1.1"
    );
}

#[test]
fn tag_only_leaves_the_note_alone_and_keep_local_leaves_the_push_out() {
    let (dir, remote, mut app) = release_project("v0.1.0");
    let file = dir.join("issues/done/a.md");

    app.open_release(&[file]);
    let flow = app.session.release.as_mut().unwrap();
    flow.note = false;
    flow.push = false;
    app.release_choice(ReleaseChoice::Confirm);

    assert!(!dir.join("CHANGELOG.md").exists(), "no note was written");
    assert_eq!(
        git(&dir, &["log", "-1", "--format=%s"]),
        "first",
        "tag only adds no commit"
    );
    assert_eq!(git(&dir, &["tag", "--list", "v0.1.1"]), "v0.1.1");
    assert!(app.session.release.is_none());
    assert_eq!(
        git(&remote, &["tag", "--list", "v0.1.1"]),
        "",
        "the push was left out"
    );
}

#[test]
fn a_written_note_goes_to_the_file_the_user_chose() {
    let (dir, _remote, mut app) = release_project("v0.1.0");
    let file = dir.join("issues/done/a.md");

    app.open_release(&[file]);
    app.session.release.as_mut().unwrap().file = "release-notes.md".to_string();
    app.release_choice(ReleaseChoice::Confirm);

    assert!(!dir.join("CHANGELOG.md").exists());
    let note = std::fs::read_to_string(dir.join("release-notes.md")).unwrap();
    assert!(note.starts_with("# Changelog\n\n## v0.1.1 — "), "{note}");
    assert_eq!(
        git(&dir, &["show", "--name-only", "--format=", "HEAD"]),
        "release-notes.md"
    );
}

#[test]
fn cancelling_stops_the_release_before_anything_is_written() {
    let (dir, _remote, mut app) = release_project("v0.1.0");
    let file = dir.join("issues/done/a.md");

    app.open_release(&[file]);
    app.release_choice(ReleaseChoice::Cancel);

    assert!(app.session.release.is_none());
    assert!(!dir.join("CHANGELOG.md").exists());
    assert_eq!(git(&dir, &["tag", "--list", "v0.1.1"]), "");
}

#[test]
fn a_tag_git_would_refuse_stays_on_the_panel() {
    let (dir, _remote, mut app) = release_project("v0.1.0");
    let file = dir.join("issues/done/a.md");

    app.open_release(&[file]);
    app.session.release.as_mut().unwrap().tag = "two words".to_string();
    app.release_choice(ReleaseChoice::Confirm);

    let flow = app.session.release.as_ref().unwrap();
    assert!(flow.error.is_some(), "the dialog says what is wrong");
    assert!(!dir.join("CHANGELOG.md").exists());
    assert_eq!(git(&dir, &["tag", "--list"]), "v0.1.0");
}

#[test]
fn a_note_file_outside_the_project_is_refused() {
    let (dir, _remote, mut app) = release_project("v0.1.0");
    let file = dir.join("issues/done/a.md");

    app.open_release(&[file]);
    app.session.release.as_mut().unwrap().file = "../NOTES.md".to_string();
    app.release_choice(ReleaseChoice::Confirm);

    let flow = app.session.release.as_ref().unwrap();
    assert!(flow.error.is_some());
    assert_eq!(git(&dir, &["tag", "--list"]), "v0.1.0");
}

#[test]
fn hovering_done_offers_release_left_of_archive_all() {
    let (dir, mut app) = project("release-head", &[("done/a.md", "# A\n"), ("todo/", "")]);
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
    let (dir, mut app) = project("release-esc", &[("done/a.md", "# Alpha\n")]);
    let file = dir.join("issues/done/a.md");
    app.open_release(&[file]);
    let mut harness = kittest(app);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(harness.state().1.session.release.is_none());
}

#[test]
fn the_release_panel_wording_follows_its_selectors() {
    let (dir, mut app) = project("release-ui", &[("done/a.md", "# Alpha\n")]);
    let file = dir.join("issues/done/a.md");
    app.open_release(&[file]);
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
    let file = dir.join("issues/done/a.md");
    app.open_release(&[file]);
    let mut harness = kittest(app);

    press(&mut harness, "Update release note, version and push");
    assert!(dir.join("CHANGELOG.md").is_file());
    assert_eq!(git(&dir, &["tag", "--list", "v0.1.1"]), "v0.1.1");
    assert_eq!(git(&remote, &["tag", "--list", "v0.1.1"]), "v0.1.1");
    assert!(harness.state().1.session.release.is_none());
}

#[test]
fn the_choices_made_are_kept_in_the_settings_file() {
    let (dir, mut app) = project("release-remember", &[("done/a.md", "# Alpha\n")]);
    let config = dir.join("iboard.toml");
    app.config_file = Some(config.clone());
    app.open_release(&[dir.join("issues/done/a.md")]);
    let mut harness = kittest(app);

    press(&mut harness, "Tag only");
    press(&mut harness, "Keep it local");
    press(&mut harness, "Cancel");

    let saved = iboard::release::ReleaseChoices::load(&mut UserConfig::load(Some(&config)));
    assert!(!saved.note, "tag only is remembered");
    assert!(!saved.push, "the local choice is remembered");

    let mut app = harness.into_state().1;
    app.open_release(&[dir.join("issues/done/a.md")]);
    let flow = app.session.release.as_ref().unwrap();
    assert!(!flow.note);
    assert!(!flow.push);
    assert_eq!(flow.file, "CHANGELOG.md");
}

#[test]
fn the_agent_groups_the_note_into_sections() {
    let (dir, mut app) = project("release-agent", &[("done/a.md", "# Alpha\n")]);
    app.prefs.agent = fake_agent("printf '### Added\\n- Alpha\\n' # {prompt}");
    app.open_release(&[dir.join("issues/done/a.md")]);
    assert_eq!(
        app.session.release.as_ref().unwrap().body,
        "- Alpha\n",
        "the raw list shows while the agent writes"
    );

    assert!(until(|| {
        app.poll_release();
        app.session
            .release
            .as_ref()
            .is_some_and(|flow| flow.body == "### Added\n- Alpha\n")
    }));
    assert!(
        app.session.release.as_ref().unwrap().grouped,
        "the note says it was grouped"
    );
    assert_eq!(app.poll_release(), None, "the note is no longer writing");
}

#[test]
fn a_grouping_note_blocks_edits_and_shows_the_thinking_mark() {
    use egui::accesskit::Live;
    let (dir, mut app) = project("release-grouping-a11y", &[("done/a.md", "# Alpha\n")]);
    app.prefs.agent = fake_agent("sleep 3; printf '### Added\\n- Alpha\\n' # {prompt}");
    app.open_release(&[dir.join("issues/done/a.md")]);
    let mut harness = kittest(app);
    harness.run_steps(2);

    let dialog = harness.get_by_role_and_label(Role::Dialog, "Release");
    assert!(dialog.accesskit_node().is_busy(), "the dialog is busy");
    assert!(
        harness.query_by_label("Working").is_some(),
        "the Thinking mark turns while the agent groups"
    );
    let live = harness
        .get_all_by_label("Grouping the note into sections")
        .filter(|node| node.accesskit_node().live() == Live::Polite)
        .count();
    assert_eq!(live, 1, "the screen reader hears the grouping");
    let note = harness.get_by_role(Role::MultilineTextInput);
    assert!(
        note.accesskit_node().is_disabled(),
        "the note can't be edited while the agent groups it"
    );

    let started = Instant::now();
    while harness
        .state()
        .1
        .session
        .release
        .as_ref()
        .is_some_and(|flow| !flow.grouped)
    {
        assert!(started.elapsed().as_secs() < 30, "the note never landed");
        harness.run_steps(1);
    }
    harness.run_steps(2);
    let dialog = harness.get_by_role_and_label(Role::Dialog, "Release");
    assert!(!dialog.accesskit_node().is_busy(), "the grouping ended");
    let note = harness.get_by_role(Role::MultilineTextInput);
    assert!(!note.accesskit_node().is_disabled(), "the note is editable");
    harness.get_by_label("Grouped into sections.");
}

#[test]
fn the_note_lists_only_the_issues_done_since_the_last_release() {
    let dir = scratch_dir("release-window");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    let old = done.join("old.md");
    let new = done.join("new.md");
    std::fs::write(&old, "# Old\n").unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "."]);
    let january = [
        ("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00"),
        ("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00"),
    ];
    git_env(&dir, &january, &["commit", "-q", "-m", "old"]);
    let release = [
        ("GIT_AUTHOR_DATE", "2026-01-15T00:00:00+00:00"),
        ("GIT_COMMITTER_DATE", "2026-01-15T00:00:00+00:00"),
    ];
    git_env(
        &dir,
        &release,
        &["commit", "-q", "--allow-empty", "-m", "release"],
    );
    git(&dir, &["tag", "v0.3.0"]);
    std::fs::write(&new, "# New\n").unwrap();
    git(&dir, &["add", "."]);
    let june = [
        ("GIT_AUTHOR_DATE", "2026-06-01T00:00:00+00:00"),
        ("GIT_COMMITTER_DATE", "2026-06-01T00:00:00+00:00"),
    ];
    git_env(&dir, &june, &["commit", "-q", "-m", "new"]);

    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_release(&[old, new]);
    let flow = app.session.release.as_ref().unwrap();
    assert_eq!(flow.tag, "v0.3.1");
    assert_eq!(flow.body, "- New\n", "the old release's issues stay out");
}

#[test]
fn a_long_note_scrolls_so_the_buttons_stay_on_screen() {
    let dir = scratch_dir("release-long");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&done).unwrap();
    let mut paths = Vec::new();
    for n in 0..40 {
        let path = done.join(format!("{n:02}.md"));
        std::fs::write(&path, format!("# Issue {n}\n")).unwrap();
        paths.push(path);
    }
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_release(&paths);
    let harness = kittest(app);

    let confirm = harness.get_by_label("Update release note, version and push");
    assert!(
        confirm.rect().bottom() <= window().bottom(),
        "the note scrolls, its buttons stay put: {:?}",
        confirm.rect()
    );
}
