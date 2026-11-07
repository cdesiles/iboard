//! The groups dialog, driven through egui_kittest by label: what it saves to
//! the user configuration, and how the board on screen follows.

use super::*;
use crate::groups_ui;
use egui_kittest::kittest::Queryable;

/// Projects `app` and `site`, a folder `bare` without `issues/`, and a user
/// configuration with the groups Clients (app, site) and Solo (app), whose
/// Clients board is shown.
fn configured(tag: &str) -> (Scratch, PathBuf, IboardApp) {
    let parent = scratch_dir(tag);
    for name in ["app", "site"] {
        std::fs::create_dir_all(parent.join(name).join("issues/todo")).unwrap();
    }
    std::fs::create_dir_all(parent.join("bare")).unwrap();
    let config = parent.join("iboard.toml");
    let (app_dir, site_dir) = (parent.join("app"), parent.join("site"));
    std::fs::write(
        &config,
        format!(
            "# mine\n[[workspace.group]]\nname = \"Clients\"\nrepos = [{:?}, {:?}]\n\n\
             [[workspace.group]]\nname = \"Solo\"\nrepos = [{:?}]\n",
            app_dir.display().to_string(),
            site_dir.display().to_string(),
            app_dir.display().to_string(),
        ),
    )
    .unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&app_dir).unwrap());
    app.config_file = Some(config.clone());
    app.workspace = crate::workspace::load(&mut UserConfig::load(Some(&config)));
    app.open_group("Clients");
    (parent, config, app)
}

fn dialog(harness: &mut Kit) -> &mut crate::groups_ui::Dialog {
    harness.state_mut().1.groups_dialog.as_mut().unwrap()
}

#[test]
fn renaming_the_shown_group_saves_and_keeps_it_on_screen() {
    let (_scratch, config, mut app) = configured("groups-rename");
    app.open_groups(false);
    let mut harness = kittest(app);
    dialog(&mut harness).draft.groups[0].name = "Customers".into();
    harness.run_steps(1);
    press(&mut harness, "Save");

    let app = &harness.state().1;
    assert!(app.groups_dialog.is_none(), "saved and closed");
    assert_eq!(app.group.as_deref(), Some("Customers"));
    assert_eq!(app.board.name(), "Customers");
    let written = std::fs::read_to_string(&config).unwrap();
    assert!(written.starts_with("# mine\n"), "{written}");
    assert!(written.contains("name = \"Customers\""), "{written}");
    let notice = &app.session.notice.as_ref().unwrap().0;
    assert!(notice.starts_with("Saved 2 groups to "), "{notice}");
}

#[test]
fn deleting_the_shown_group_asks_then_the_first_left_opens() {
    let (_scratch, config, mut app) = configured("groups-delete");
    app.open_groups(false);
    let mut harness = kittest(app);
    press_nth(&mut harness, "Delete group (asks first)", 0);
    harness.get_by_role_and_label(egui::accesskit::Role::AlertDialog, "Delete this group?");
    press(&mut harness, "Delete group");
    assert_eq!(
        dialog(&mut harness).draft.groups.len(),
        1,
        "gone from the draft"
    );
    assert!(
        std::fs::read_to_string(&config)
            .unwrap()
            .contains("Clients"),
        "nothing written before Save"
    );
    press(&mut harness, "Save");

    let app = &harness.state().1;
    assert_eq!(app.group.as_deref(), Some("Solo"), "the first group left");
    assert!(
        !std::fs::read_to_string(&config)
            .unwrap()
            .contains("Clients")
    );
}

#[test]
fn projects_come_from_the_recent_ones_or_the_picker_with_an_issues_folder() {
    let (parent, _config, mut app) = configured("groups-add");
    app.recents = vec![parent.join("site")];
    app.folder_picker = || {
        let parent = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        vec![parent.join("iboard-no-such-project")]
    };
    app.open_groups(false);
    let mut harness = kittest(app);
    press_nth(&mut harness, "Add project", 1);
    // The second "site": the first is Clients' project row.
    press_nth(&mut harness, "site", 1);
    assert_eq!(
        dialog(&mut harness).draft.groups[1].repos.len(),
        2,
        "site added to Solo"
    );

    press(&mut harness, "Choose folders…");
    let notice = dialog(&mut harness).draft.groups[1].notice.clone().unwrap();
    assert!(
        notice.ends_with("has no issues/ folder. Pick the project's root."),
        "{notice}"
    );
    assert_eq!(dialog(&mut harness).draft.groups[1].repos.len(), 2);
}

#[test]
fn each_project_is_looked_up_once_per_opening() {
    let (parent, _config, mut app) = configured("groups-lookup");
    app.open_groups(false);
    let mut harness = kittest(app);
    harness.run_steps(1);
    std::fs::remove_dir_all(parent.join("site/issues")).unwrap();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label_contains("no issues/ folder")
            .is_none(),
        "not looked up every frame"
    );

    harness.state_mut().1.groups_dialog = None;
    harness.state_mut().1.open_groups(false);
    harness.run_steps(2);
    harness.get_by_label_contains("no issues/ folder");
}

#[test]
fn the_add_project_menu_moves_with_the_arrows_and_adds_with_return() {
    let (parent, _config, mut app) = configured("groups-add-keys");
    app.recents = vec![parent.join("site")];
    app.open_groups(false);
    let mut harness = kittest(app);
    press_nth(&mut harness, "Add project", 1);
    harness.key_press(Key::ArrowDown);
    harness.run_steps(2);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    let repos = &dialog(&mut harness).draft.groups[1].repos;
    assert_eq!(repos.len(), 2);
    assert!(repos[1].ends_with("site"));
    harness.get_by_label("Choose folders…");
}

#[test]
fn cancelling_with_changes_asks_first_and_writes_nothing() {
    let (_scratch, config, mut app) = configured("groups-cancel");
    let before = std::fs::read_to_string(&config).unwrap();
    app.open_groups(false);
    let mut harness = kittest(app);
    dialog(&mut harness).draft.groups[1].name = "Alone".into();
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(harness.state().1.groups_dialog.is_some(), "asks first");
    harness.get_by_label("Discard changes to groups?");
    press(&mut harness, "Discard");
    assert!(harness.state().1.groups_dialog.is_none());
    assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
}

#[test]
fn a_file_changed_meanwhile_is_kept_until_asked() {
    let (_scratch, config, mut app) = configured("groups-conflict");
    app.open_groups(false);
    let mut harness = kittest(app);
    let outside = "[ai]\ntool = \"claude\"\n";
    std::fs::write(&config, outside).unwrap();
    dialog(&mut harness).draft.groups[1].name = "Alone".into();
    press(&mut harness, "Save");
    assert!(dialog(&mut harness).conflict);
    assert_eq!(std::fs::read_to_string(&config).unwrap(), outside);

    harness.get_by_label("Keep your version").hover();
    harness.run_steps(1);
    press(&mut harness, "Keep your version");
    if let Some(d) = harness.state().1.groups_dialog.as_ref() {
        panic!(
            "conflict {} error {:?} groups {:?}",
            d.conflict, d.save_error, d.draft.groups
        );
    }
    let written = std::fs::read_to_string(&config).unwrap();
    assert!(written.starts_with(outside), "other keys kept: {written}");
    assert!(written.contains("name = \"Alone\""), "{written}");
}

#[test]
fn invalid_names_keep_the_dialog_open() {
    let (_scratch, config, mut app) = configured("groups-invalid");
    let before = std::fs::read_to_string(&config).unwrap();
    app.open_groups(true);
    let mut harness = kittest(app);
    assert_eq!(dialog(&mut harness).draft.groups.len(), 3, "a new group");
    dialog(&mut harness).draft.groups[2].name = "solo".into();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::Enter);
    harness.run_steps(2);
    harness.get_by_label("Another group is already called solo. Rename one of them.");
    assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
}

#[test]
fn an_outside_edit_reloads_the_groups() {
    let (_scratch, config, mut app) = configured("groups-watch");
    app.config_stamp = None;
    std::fs::write(
        &config,
        "[[workspace.group]]\nname = \"Solo\"\nrepos = []\n",
    )
    .unwrap();
    app.config_checked = Instant::now() - Duration::from_secs(5);
    app.watch_config();
    assert_eq!(app.workspace.as_ref().unwrap().groups.len(), 1);
    assert_eq!(app.group.as_deref(), Some("Solo"), "Clients is gone");
}

/// Writes `text` to the configuration as an editor outside iboard does, then
/// runs the frame that notices.
fn edit_outside(harness: &mut Kit, config: &Path, text: &str) {
    std::fs::write(config, text).unwrap();
    let app = &mut harness.state_mut().1;
    app.config_stamp = None;
    app.config_checked = Instant::now() - Duration::from_secs(5);
    harness.run_steps(1);
}

#[test]
fn a_file_saved_mid_edit_keeps_the_board_until_it_reads_again() {
    let (_scratch, config, app) = configured("groups-mid-edit");
    let good = std::fs::read_to_string(&config).unwrap();
    let mut harness = kittest(app);
    for broken in [
        "[[workspace.group]]\nname = \n",
        "[[workspace.group]]\nrepos = []\n",
    ] {
        edit_outside(&mut harness, &config, broken);
        harness.get_by_label_contains("Settings: ");
        let app = &harness.state().1;
        assert_eq!(app.board.name(), "Clients", "{broken}");
        assert_eq!(app.workspace.as_ref().map(|w| w.groups.len()), Some(2));
    }

    edit_outside(&mut harness, &config, &good);
    assert!(harness.query_by_label_contains("Settings: ").is_none());
    assert!(harness.state().1.config_problem.is_none());
    assert_eq!(harness.state().1.board.name(), "Clients");
}

#[test]
fn reloading_a_file_saved_mid_edit_keeps_the_groups() {
    let (_scratch, config, mut app) = configured("groups-reload-broken");
    app.open_groups(false);
    let mut harness = kittest(app);
    std::fs::write(&config, "[[workspace.group]]\nname = \n").unwrap();
    dialog(&mut harness).draft.groups[1].name = "Alone".into();
    press(&mut harness, "Save");
    harness.run_steps(2);
    press(&mut harness, "Reload");

    let names: Vec<String> = dialog(&mut harness)
        .draft
        .groups
        .iter()
        .map(|group| group.name.clone())
        .collect();
    assert_eq!(names, ["Clients", "Solo"]);
    assert_eq!(harness.state().1.board.name(), "Clients");
}

#[test]
fn the_dialog_opens_with_the_focus_in_a_name() {
    let (_scratch, _config, mut app) = configured("groups-focus");
    app.open_groups(false);
    let harness = kittest(app);
    assert!(harness.ctx.memory(|m| m.has_focus(groups_ui::name_id(0))));

    let (_scratch, _config, mut app) = configured("groups-focus-new");
    app.open_groups(true);
    let harness = kittest(app);
    assert!(harness.ctx.memory(|m| m.has_focus(groups_ui::name_id(2))));
}

#[test]
fn del_removes_the_focused_project_and_moves_to_the_next_row() {
    let (_scratch, _config, mut app) = configured("groups-del");
    app.open_groups(false);
    let mut harness = kittest(app);
    let remove = "Remove from this group. The folder is not touched. (Del)";
    harness.get_by_label_contains("Clients").hover();
    assert_eq!(
        harness.query_all_by_label(remove).count(),
        0,
        "× shows on hover or focus only"
    );

    harness
        .ctx
        .memory_mut(|m| m.request_focus(groups_ui::row_id(0, 0)));
    harness.run_steps(2);
    harness.get_by_label(remove);
    harness.key_press(Key::Delete);
    harness.run_steps(2);
    let repos = &dialog(&mut harness).draft.groups[0].repos;
    assert_eq!(repos.len(), 1);
    assert!(repos[0].ends_with("site"));
    assert!(
        harness.ctx.memory(|m| m.has_focus(groups_ui::row_id(0, 0))),
        "site's row, now the first"
    );

    harness.key_press(Key::Delete);
    harness.run_steps(2);
    assert!(dialog(&mut harness).draft.groups[0].repos.is_empty());
    harness.get_by_label("No projects in this group yet —");
}

#[test]
fn cmd_backspace_inside_a_group_asks_first_and_leaves_its_name() {
    let (_scratch, _config, mut app) = configured("groups-cmd-delete");
    app.open_groups(false);
    let mut harness = kittest(app);
    let delete = |harness: &mut Kit| {
        harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::Backspace);
        harness.run_steps(2);
        harness.get_by_role_and_label(egui::accesskit::Role::AlertDialog, "Delete this group?");
        harness.key_press(Key::Escape);
        harness.run_steps(2);
    };
    delete(&mut harness);
    assert_eq!(
        dialog(&mut harness).draft.groups[0].name,
        "Clients",
        "from the name, which keeps its text"
    );

    harness
        .ctx
        .memory_mut(|m| m.request_focus(groups_ui::row_id(1, 0)));
    harness.run_steps(2);
    let ctx = harness.ctx.clone();
    assert_eq!(dialog(&mut harness).focused_group(&ctx), Some(1));
    delete(&mut harness);
}

#[test]
fn a_change_shows_the_unsaved_pill() {
    let (_scratch, _config, mut app) = configured("groups-unsaved");
    app.open_groups(false);
    let mut harness = kittest(app);
    assert_eq!(harness.query_all_by_label("Unsaved").count(), 0);
    dialog(&mut harness).draft.groups[1].name = "Alone".into();
    harness.run_steps(2);
    harness.get_by_label("Unsaved");
}

#[test]
fn a_folder_already_in_the_group_is_skipped_and_its_row_flashes() {
    let (parent, _config, mut app) = configured("groups-flash");
    app.open_groups(false);
    let mut harness = kittest(app);
    let app_dir = parent.join("app");
    let (at, flash) = {
        let dialog = dialog(&mut harness);
        dialog.add_picked(0, &app_dir, 1.0);
        (dialog.draft.groups[0].repos.len(), dialog.flash.clone())
    };
    assert_eq!(at, 2, "skipped");
    assert_eq!(flash, Some((0, app_dir, 1.0)));
}

#[test]
fn a_conflict_ends_the_body_and_names_the_file() {
    let (_scratch, config, mut app) = configured("groups-conflict-text");
    app.open_groups(false);
    let mut harness = kittest(app);
    std::fs::write(&config, "[ai]\ntool = \"claude\"\n").unwrap();
    dialog(&mut harness).draft.groups[1].name = "Alone".into();
    press(&mut harness, "Save");
    harness.get_by_label(&format!(
        "Couldn't save {}: the file changed on disk. Reload or keep your version.",
        display_path(&config)
    ));
}
