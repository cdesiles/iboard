//! The tests of a group's board: every move, new issue, review and build
//! stays in the card's own project, and each group is a board of its own.

use super::*;

/// Two projects, `app` (Ready: A) and `site` (Ready: B, Todo: C), in the
/// group Clients, whose board is shown; `app` alone is also in Solo.
fn two_projects(tag: &str) -> (Scratch, PathBuf, PathBuf, IboardApp) {
    let parent = scratch_dir(tag);
    let project = |name: &str, files: &[(&str, &str)]| {
        let dir = parent.join(name);
        for column in ["todo", "ready", "doing", "done"] {
            std::fs::create_dir_all(dir.join("issues").join(column)).unwrap();
        }
        for (file, text) in files {
            std::fs::write(dir.join("issues").join(file), text).unwrap();
        }
        dir
    };
    let app_dir = project("app", &[("ready/001-a.md", "# A\n")]);
    let site_dir = project(
        "site",
        &[("ready/001-b.md", "# B\n"), ("todo/c.md", "# C\n\nWhy.\n")],
    );
    let mut app = IboardApp::from_located(discovery::locate(&app_dir).unwrap());
    app.workspace = Some(Workspace {
        groups: vec![
            crate::workspace::Group {
                name: "Clients".into(),
                repos: vec![app_dir.clone(), site_dir.clone()],
            },
            crate::workspace::Group {
                name: "Solo".into(),
                repos: vec![app_dir.clone()],
            },
        ],
    });
    app.open_group("Clients");
    (parent, app_dir, site_dir, app)
}

fn column_dir(app: &IboardApp, name: &str) -> PathBuf {
    app.board
        .columns
        .iter()
        .find(|column| column.name == name)
        .map(|column| column.dir.clone())
        .unwrap()
}

#[test]
fn the_board_merges_the_projects_and_moves_stay_in_their_own() {
    let (_scratch, app_dir, site_dir, mut app) = two_projects("all-move");
    assert!(app.showing_group());
    assert_eq!(app.board.name(), "Clients");
    assert_eq!(app.pending_title.as_deref(), Some("Clients"));
    let ready = column_dir(&app, "Ready");
    assert_eq!(app.board.columns[1].issues.len(), 2, "A and B in one Ready");

    let a = app_dir.join("issues/ready/001-a.md");
    let doing = column_dir(&app, "Doing");
    let moved = app.move_issue(&a, &doing).unwrap();
    assert_eq!(moved, app_dir.join("issues/doing/a.md"));
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.board.in_column(&moved, &doing));
    assert!(
        !doing.exists() && !ready.exists(),
        "no virtual folder written"
    );
    assert!(site_dir.join("issues/ready/001-b.md").exists());
}

#[test]
fn a_new_issue_goes_to_the_filtered_project() {
    let (_scratch, app_dir, site_dir, mut app) = two_projects("all-compose");
    app.filter_bar.filter.pick("repo:site");
    app.open_composer();
    app.session.composer.as_mut().unwrap().description = "Body.".into();
    app.write_composed("Fresh idea", false);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(site_dir.join("issues/todo/fresh-idea.md").exists());
    assert!(!app_dir.join("issues/todo/fresh-idea.md").exists());
}

/// Stats measure one history: on a group's board they say so instead.
#[test]
fn a_group_board_has_no_stats() {
    let (_scratch, _app_dir, _site_dir, mut app) = two_projects("all-stats");
    app.open_stats();
    assert!(app.session.stats.is_none());
    assert!(app.session.notice.is_some());
}

#[test]
fn a_review_runs_in_its_project_and_is_kept_there() {
    let (_scratch, _app_dir, site_dir, mut app) = two_projects("all-review");
    app.prefs.agent = fake_agent(
        "printf 'RELEVANCE: Pass\\n%s\\nSIZE: Pass\\nok\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE' \"$(pwd -P)\" # {prompt}",
    );
    let c = site_dir.join("issues/todo/c.md");
    assert!(app.is_todo_issue(&c));
    app.start_review(&c, false);
    assert!(until(|| {
        app.poll_review();
        app.past_reviews.contains_key(&c)
    }));
    let kept = std::fs::read_to_string(site_dir.join("issues/.reviews/c.toml")).unwrap();
    assert!(kept.contains(&site_dir.display().to_string()), "{kept}");
    app.refresh();
    assert!(
        app.past_reviews.contains_key(&c),
        "read back from its project"
    );
}

#[test]
fn play_builds_each_card_in_its_own_project() {
    let (_scratch, app_dir, site_dir, mut app) = two_projects("all-build");
    app.prefs.build_agent = fake_agent("touch built # {prompt}");
    app.session.agents = 2;
    let ready: Vec<PathBuf> = app.board.columns[1]
        .issues
        .iter()
        .map(|issue| issue.path.clone())
        .collect();
    app.start_build(ready);
    assert!(app_dir.join("issues/doing/a.md").exists());
    assert!(site_dir.join("issues/doing/b.md").exists());
    assert!(until(|| {
        app.poll_build();
        app.session.build.is_none()
    }));
    assert!(app_dir.join("built").exists() && site_dir.join("built").exists());
}

#[test]
fn the_header_names_the_group_instead_of_a_path() {
    use egui_kittest::kittest::Queryable;
    let (_scratch, _app_dir, _site_dir, app) = two_projects("all-header");
    let harness = kittest(app);
    harness.get_by_label("Clients");
    harness.get_by_label("group · 2 projects");
    assert!(harness.query_by_label("Stats").is_none(), "no Stats button");
}

fn ready_titles(app: &IboardApp) -> Vec<String> {
    app.board.columns[1]
        .issues
        .iter()
        .map(|issue| issue.title.clone())
        .collect()
}

#[test]
fn reordering_across_projects_renames_nothing() {
    let (scratch, app_dir, site_dir, mut app) = two_projects("all-order");
    app.order_dir = Some(scratch.join("orders"));
    let file = crate::workspace::order::file(&scratch.join("orders"), "Clients");
    std::fs::write(app_dir.join("issues/ready/002-x.md"), "# X\n").unwrap();
    app.refresh();
    assert_eq!(ready_titles(&app), ["A", "B", "X"]);
    let ready = column_dir(&app, "Ready");

    let b = site_dir.join("issues/ready/001-b.md");
    let a = app_dir.join("issues/ready/001-a.md");
    app.reorder_column(&ready, std::slice::from_ref(&b), Some(&a));
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(ready_titles(&app), ["B", "A", "X"]);
    assert!(a.exists() && b.exists(), "no file renamed");
    let stored = std::fs::read_to_string(&file).unwrap();
    assert!(stored.contains("site/b.md"), "{stored}");

    let x = app_dir.join("issues/ready/002-x.md");
    app.reorder_column(&ready, &[x], Some(&a));
    assert_eq!(ready_titles(&app), ["B", "X", "A"]);
    assert!(app_dir.join("issues/ready/001-x.md").exists());
    assert!(app_dir.join("issues/ready/002-a.md").exists());
    assert!(b.exists(), "the other project keeps its names");
}

/// The menu lists the stored order first, then the recent projects, then
/// the groups' projects it does not name yet.
#[test]
fn the_menu_lists_the_projects_in_the_users_order() {
    let (_scratch, app_dir, site_dir, mut app) = two_projects("all-menu");
    assert_eq!(app.ordered_projects(), [app_dir.clone(), site_dir.clone()]);
    app.recents = vec![site_dir.clone()];
    assert_eq!(app.ordered_projects(), [site_dir.clone(), app_dir.clone()]);
    app.projects = vec![app_dir.clone(), app_dir.parent().unwrap().join("gone")];
    assert_eq!(
        app.ordered_projects(),
        [app_dir, site_dir],
        "a folder that is gone drops out"
    );
}

/// `⌃⇥` goes round each group and each project, `⌃⇧⇥` the other way.
#[test]
fn ctrl_tab_switches_to_the_next_project() {
    use super::super::all::Target;
    let (_scratch, app_dir, site_dir, mut app) = two_projects("all-switch");
    let clients = Target::Group("Clients".into());
    let solo = Target::Group("Solo".into());
    assert_eq!(
        app.project_cycle(),
        [
            clients.clone(),
            solo.clone(),
            Target::Project(app_dir.clone()),
            Target::Project(site_dir.clone())
        ]
    );
    let ctx = themed_ctx();
    let ctrl = egui::Modifiers::CTRL;
    let shift_ctrl = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    let press = |modifiers: egui::Modifiers| {
        vec![
            egui::Event::ModifiersChanged(modifiers),
            key(Key::Tab, modifiers),
        ]
    };
    frame(&ctx, &mut app, press(ctrl));
    assert_eq!(app.current_target(), solo);
    frame(&ctx, &mut app, press(ctrl));
    assert_eq!(app.current_target(), Target::Project(app_dir));
    frame(&ctx, &mut app, press(ctrl));
    assert_eq!(app.current_target(), Target::Project(site_dir));
    frame(&ctx, &mut app, press(ctrl));
    assert_eq!(
        app.current_target(),
        clients,
        "round the end to the first group"
    );
    frame(&ctx, &mut app, press(shift_ctrl));
    assert!(!app.showing_group(), "⌃⇧⇥ goes back");
    assert_eq!(app.project_cycle().len(), 4, "the order stays steady");
}

/// Each group is a board of its own projects only.
#[test]
fn each_group_is_its_own_board() {
    let (_scratch, _app_dir, _site_dir, mut app) = two_projects("all-groups");
    app.open_group("Solo");
    assert_eq!(app.board.name(), "Solo");
    assert_eq!(ready_titles(&app), ["A"], "site is not in Solo");
    app.open_group("Clients");
    assert_eq!(ready_titles(&app), ["A", "B"]);
    app.open_group("Nowhere");
    assert_eq!(
        app.board.name(),
        "Clients",
        "an unknown group changes nothing"
    );
}
