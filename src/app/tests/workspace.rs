//! The All projects board's tests: every move, new issue, review and build
//! stays in the card's own project.

use super::*;

/// Two projects, `app` (Ready: A) and `site` (Ready: B, Todo: C), listed in
/// one group, shown on the All projects board.
fn two_projects(tag: &str) -> (Scratch, PathBuf, PathBuf, IboardApp) {
    let scratch = scratch_dir(tag);
    let parent = std::fs::canonicalize(&*scratch).unwrap();
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
        groups: vec![crate::workspace::Group {
            name: "Clients".into(),
            repos: vec![app_dir.clone(), site_dir.clone()],
        }],
    });
    app.open_all();
    (scratch, app_dir, site_dir, app)
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
    assert!(app.showing_all());
    assert_eq!(app.board.name(), crate::workspace::TITLE);
    assert_eq!(app.pending_title.as_deref(), Some(crate::workspace::TITLE));
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
    app.write_composed("Fresh idea", "Body.");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(site_dir.join("issues/todo/fresh-idea.md").exists());
    assert!(!app_dir.join("issues/todo/fresh-idea.md").exists());
}

/// Stats measure one history: on the All projects board they say so instead.
#[test]
fn the_all_projects_board_has_no_stats() {
    let (_scratch, _app_dir, _site_dir, mut app) = two_projects("all-stats");
    app.open_stats();
    assert!(app.session.stats.is_none());
    assert!(app.session.notice.is_some());
}

#[test]
fn a_review_runs_in_its_project_and_is_kept_there() {
    let (_scratch, _app_dir, site_dir, mut app) = two_projects("all-review");
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "printf 'RELEVANCE: Pass\\n%s\\nSIZE: Pass\\nok\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE' \"$(pwd -P)\" # {prompt}"
            .to_string(),
        configured: true,
        ..AgentConfig::default()
    };
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
    app.build_agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "touch built # {prompt}".to_string(),
        configured: true,
        ..AgentConfig::default()
    };
    app.session.agents = 2;
    let ready: Vec<PathBuf> = app.board.columns[1]
        .issues
        .iter()
        .map(|issue| issue.path.clone())
        .collect();
    app.start_build(ready);
    assert!(app_dir.join("issues/doing/a.md").exists());
    assert!(site_dir.join("issues/doing/b.md").exists());
    assert!(until(
        || app_dir.join("built").exists() && site_dir.join("built").exists()
    ));
}

#[test]
fn the_header_names_the_groups_instead_of_a_path() {
    use egui_kittest::kittest::Queryable;
    let (_scratch, _app_dir, _site_dir, app) = two_projects("all-header");
    let mut harness = egui_kittest::Harness::builder()
        .with_size(window().size())
        .build_ui_state(
            |ui, (themed, app): &mut (bool, IboardApp)| {
                if !*themed {
                    theme::install(ui.ctx());
                    *themed = true;
                    return;
                }
                app.draw(ui);
            },
            (false, app),
        );
    harness.run_steps(2);
    harness.get_by_label(crate::workspace::TITLE);
    harness.get_by_label("Clients");
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
    let file = scratch.join("order.toml");
    app.order_file = Some(file.clone());
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

/// The project menu lists the workspace's projects that are not recent yet.
#[test]
fn the_menu_lists_the_workspace_projects_below_the_recent_ones() {
    let (_scratch, app_dir, site_dir, mut app) = two_projects("all-menu");
    assert_eq!(
        app.workspace_projects(),
        [
            (app_dir.clone(), "Clients".to_string()),
            (site_dir.clone(), "Clients".to_string())
        ]
    );
    app.recents = vec![app_dir];
    assert_eq!(
        app.workspace_projects(),
        [(site_dir, "Clients".to_string())]
    );
}
