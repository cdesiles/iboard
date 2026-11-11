//! FoldersDialog through the app: what it offers, what it creates, and what
//! Not now and the ProjectMenu remember.

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use super::*;

/// A project whose board has only `todo/`, offered its missing folders the
/// way a launch does.
fn incomplete(tag: &str) -> (Scratch, IboardApp, PathBuf) {
    let dir = scratch_dir(tag);
    let issues = dir.join("issues");
    std::fs::create_dir_all(issues.join("todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.offer_folders();
    (dir, app, issues)
}

#[test]
fn a_board_missing_columns_is_offered_them_in_their_canonical_order() {
    let (_dir, app, _issues) = incomplete("folders-offer");
    let harness = kittest(app);
    harness.get_by_role_and_label(Role::AlertDialog, "Board folders");
    harness.get_by_label("Create 3 folders");

    let rows = [
        "Todo, todo/, present",
        "Ready, ready/, missing",
        "Doing, doing/, missing",
        "Done, done/, missing",
    ];
    let tops: Vec<f32> = rows
        .iter()
        .map(|row| harness.get_by_label(row).rect().top())
        .collect();
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
}

#[test]
fn creating_the_missing_folders_closes_and_reloads_the_board() {
    let (_dir, app, issues) = incomplete("folders-create");
    let mut harness = kittest(app);
    press(&mut harness, "Create 3 folders");

    let app = &harness.state().1;
    assert!(app.folders_dialog.is_none());
    for name in ["ready", "doing", "done"] {
        assert!(issues.join(name).join(".gitkeep").is_file(), "{name}");
    }
    assert!(
        !issues.join("todo/.gitkeep").exists(),
        "an existing folder is left as it is"
    );
    let names: Vec<&str> = app
        .board
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    assert_eq!(names, ["Todo", "Ready", "Doing", "Done"]);
}

#[test]
fn not_now_is_written_for_the_project_and_set_up_folders_brings_it_back() {
    let (dir, mut app, _issues) = incomplete("folders-not-now");
    let config = dir.join("iboard.toml");
    std::fs::write(&config, "").unwrap();
    app.config_file = Some(config.clone());
    let mut harness = kittest(app);
    press(&mut harness, "Not now");

    let app = &harness.state().1;
    assert!(app.folders_dialog.is_none());
    assert!(
        app.folders_quiet
            .iter()
            .any(|quiet| recents::canonical(quiet) == recents::canonical(&dir))
    );
    let written = std::fs::read_to_string(&config).unwrap();
    assert!(written.contains("quiet_folders"), "{written}");

    harness.state_mut().1.folders_offered.clear();
    harness.state_mut().1.offer_folders();
    harness.run_steps(2);
    assert!(
        harness.state().1.folders_dialog.is_none(),
        "a later launch keeps the answer"
    );

    harness.state_mut().1.set_up_folders();
    harness.run_steps(2);
    assert!(harness.state().1.folders_dialog.is_some());
    assert!(
        harness.state().1.folders_quiet.is_empty(),
        "the way back forgets Not now"
    );
}

#[test]
fn esc_is_not_now() {
    let (dir, mut app, _issues) = incomplete("folders-esc");
    let config = dir.join("iboard.toml");
    std::fs::write(&config, "").unwrap();
    app.config_file = Some(config);
    let mut harness = kittest(app);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    let app = &harness.state().1;
    assert!(app.folders_dialog.is_none());
    assert!(
        app.folders_quiet
            .iter()
            .any(|quiet| recents::canonical(quiet) == recents::canonical(&dir))
    );
}

#[test]
fn a_group_board_is_never_offered_folders() {
    let (_dir, mut app, _issues) = incomplete("folders-group");
    app.folders_dialog = None;
    app.folders_offered.clear();
    app.workspace = Some(crate::workspace::Workspace {
        groups: vec![crate::workspace::Group {
            name: "Clients".to_string(),
            repos: vec![app.located.project_dir.clone()],
        }],
    });
    app.open_group("Clients");
    app.offer_folders();
    assert!(app.folders_dialog.is_none());
}

#[test]
fn the_menu_offers_set_up_folders_only_while_one_is_missing() {
    let (_dir, mut app, issues) = incomplete("folders-menu-row");
    assert!(app.menu_entries().set_up_folders);
    for name in ["ready", "doing", "done"] {
        std::fs::create_dir_all(issues.join(name)).unwrap();
    }
    assert!(!app.menu_entries().set_up_folders);

    app.workspace = Some(crate::workspace::Workspace {
        groups: vec![crate::workspace::Group {
            name: "Clients".to_string(),
            repos: vec![app.located.project_dir.clone()],
        }],
    });
    app.open_group("Clients");
    assert!(
        !app.menu_entries().set_up_folders,
        "a group is not one project's shape"
    );
}

#[test]
fn a_new_name_moves_the_board_and_keeps_the_old_folder() {
    let (dir, app, issues) = incomplete("folders-rename");
    let config = dir.join("iboard.toml");
    std::fs::write(&config, "# mine\n[reader]\nsections = []\n").unwrap();
    let mut harness = kittest(app);
    harness.state_mut().1.folders_dialog.as_mut().unwrap().name = "tickets".to_string();
    harness.run_steps(2);
    press(&mut harness, "Create 4 folders");

    let app = &harness.state().1;
    assert!(dir.join("tickets/todo/.gitkeep").is_file());
    assert_eq!(
        app.board.root,
        recents::canonical(&dir.join("tickets")),
        "the board opens on the new folder"
    );
    assert!(issues.join("todo").is_dir(), "the old folder stays");
    let written = std::fs::read_to_string(&config).unwrap();
    assert!(written.contains("# mine"), "{written}");
    assert!(written.contains("root = \"tickets\""), "{written}");
    assert!(written.contains("[reader]"), "{written}");
    let ignore = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert_eq!(ignore, "/tickets/\n");
    assert!(app.folders_dialog.is_none());
    assert!(!app.missing_standard_folders());
}

#[test]
fn the_error_strip_names_the_configured_folder_it_creates() {
    let base = scratch_dir("folders-strip-base");
    std::fs::create_dir_all(base.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&base).unwrap());

    let project = scratch_dir("folders-strip");
    std::fs::write(project.join("iboard.toml"), "root = \"tickets\"\n").unwrap();
    app.open_project(project.to_path_buf());
    assert!(app.folders_dialog.is_none(), "no board folder, no dialog");

    let mut harness = kittest(app);
    press(&mut harness, "Create tickets/");
    assert!(project.join("tickets/todo/.gitkeep").is_file());
    assert!(!project.join("issues").exists());
    assert!(harness.state().1.folders_dialog.is_none());
}
