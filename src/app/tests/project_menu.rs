//! The ProjectMenu, driven through egui_kittest by label: typing filters,
//! `esc` clears then closes, Reorder saves the user's order, and Add
//! project puts a project at the top.

use super::*;
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type Menu = Harness<'static, (bool, IboardApp)>;

/// Projects `alpha`, `beta` and `gamma`, each with its issues; the board
/// shows `alpha`, and a user configuration lives in the scratch folder.
fn three_projects(tag: &str) -> (Scratch, PathBuf, Vec<PathBuf>, IboardApp) {
    let parent = scratch_dir(tag);
    let projects: Vec<PathBuf> = ["alpha", "beta", "gamma"]
        .iter()
        .map(|name| {
            let dir = parent.join(name);
            std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
            dir
        })
        .collect();
    let config = parent.join("iboard.toml");
    std::fs::write(&config, "# mine\n[ai]\ntool = \"claude\"\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&projects[0]).unwrap());
    app.config_file = Some(config.clone());
    app.config_stamp = crate::app::groups::modified(&config);
    app.projects = projects.clone();
    (parent, config, projects, app)
}

fn open(harness: &mut Menu) {
    let ctx = harness.ctx.clone();
    harness.state_mut().1.toggle_project_menu(&ctx);
    harness.run_steps(2);
}

fn key(harness: &mut Menu, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    harness.run_steps(2);
}

fn shown(harness: &Menu, label: &str) -> bool {
    harness.query_all_by_label(label).next().is_some()
}

#[test]
fn the_menu_lists_the_projects_and_its_three_actions() {
    let (_scratch, _config, _projects, app) = three_projects("menu-list");
    let mut harness = kittest(app);
    open(&mut harness);
    for label in [
        "alpha",
        "beta",
        "gamma",
        "Stats",
        "Add project…",
        "Edit groups…",
        "Reorder",
    ] {
        assert!(shown(&harness, label), "{label}");
    }
    press(&mut harness, "gamma");
    let app = &harness.state().1;
    assert!(app.project_menu.is_none(), "picking closes the menu");
    assert!(app.located.project_dir.ends_with("gamma"));
}

/// Rows are 42pt, and the trigger holds the name and no path.
#[test]
fn the_rows_are_42pt_and_the_trigger_has_no_path() {
    let (_scratch, _config, projects, app) = three_projects("menu-rows");
    let mut harness = kittest(app);
    open(&mut harness);
    assert_eq!(harness.get_by_label("beta").rect().height(), 42.0);
    let path = display_path(&projects[0]);
    let in_trigger = harness.get_by_label("Switch project").rect();
    let path_rects = harness.query_all_by_label(&path).map(|node| node.rect());
    assert!(
        path_rects
            .into_iter()
            .all(|rect| !rect.intersects(in_trigger))
    );
}

/// The open menu owns the keyboard, and its Stats row keeps the key it shows.
#[test]
fn command_i_closes_the_menu_and_opens_stats() {
    let (_scratch, _config, _projects, app) = three_projects("menu-stats-key");
    let mut harness = kittest(app);
    open(&mut harness);
    key(&mut harness, Modifiers::COMMAND, Key::I);
    let app = &harness.state().1;
    assert!(app.project_menu.is_none());
    assert!(app.session.stats.is_some());
}

/// On a group's board Stats stays in the menu; the status bar says why it
/// shows nothing, and `⌘I` says the same.
#[test]
fn stats_on_a_groups_board_says_it_measures_one_project() {
    let (_scratch, _config, projects, mut app) = three_projects("menu-group-stats");
    app.workspace = Some(crate::workspace::Workspace {
        groups: vec![crate::workspace::Group {
            name: "Clients".to_string(),
            repos: projects.clone(),
        }],
    });
    app.open_group("Clients");
    let mut harness = kittest(app);
    open(&mut harness);
    press(&mut harness, "Stats");
    let app = &harness.state().1;
    assert!(app.session.stats.is_none());
    assert!(app.project_menu.is_none());
    let notice = &app.session.notice.as_ref().unwrap().0;
    assert!(notice.contains("Stats measure one project"), "{notice}");
}

#[test]
fn down_on_the_focused_trigger_opens_the_menu_and_leaves_the_board_alone() {
    let (_scratch, _config, _projects, app) = three_projects("menu-down");
    let mut harness = kittest(app);
    harness
        .ctx
        .memory_mut(|m| m.request_focus(widgets::project_trigger_id()));
    harness.run_steps(2);
    key(&mut harness, Modifiers::NONE, Key::ArrowDown);
    let app = &harness.state().1;
    assert!(app.project_menu.is_some());
    assert!(app.session.selected.is_none(), "no card stepped to");
}

#[test]
fn typing_filters_and_return_opens_the_first_match() {
    let (_scratch, _config, _projects, app) = three_projects("menu-type");
    let mut harness = kittest(app);
    open(&mut harness);
    harness.event(Event::Text("ga".into()));
    harness.run_steps(2);
    assert_eq!(
        harness.state().1.project_menu.as_ref().unwrap().filter,
        "ga"
    );
    assert!(shown(&harness, "gamma"));
    assert!(!shown(&harness, "beta"), "filtered out");
    assert!(shown(&harness, "↩ opens gamma · esc clears the filter"));

    key(&mut harness, Modifiers::NONE, Key::Escape);
    assert!(shown(&harness, "beta"), "esc clears the filter first");
    assert!(
        harness.state().1.project_menu.is_some(),
        "and keeps the menu"
    );

    harness.event(Event::Text("bet".into()));
    harness.run_steps(2);
    key(&mut harness, Modifiers::NONE, Key::Enter);
    let app = &harness.state().1;
    assert!(app.project_menu.is_none());
    assert!(app.located.project_dir.ends_with("beta"));
}

#[test]
fn esc_without_a_filter_closes_and_no_match_says_so() {
    let (_scratch, _config, _projects, app) = three_projects("menu-esc");
    let mut harness = kittest(app);
    open(&mut harness);
    harness.event(Event::Text("zz".into()));
    harness.run_steps(2);
    assert!(shown(
        &harness,
        "No project matches zz. Add project… to add one."
    ));
    key(&mut harness, Modifiers::NONE, Key::Escape);
    key(&mut harness, Modifiers::NONE, Key::Escape);
    assert!(harness.state().1.project_menu.is_none());
}

#[test]
fn reorder_moves_with_the_keys_and_done_saves_the_order() {
    let (_scratch, config, projects, app) = three_projects("menu-reorder");
    let mut harness = kittest(app);
    open(&mut harness);
    press(&mut harness, "Reorder");
    assert!(shown(&harness, "Done"));
    // alpha, the project on screen, is selected: to the bottom, then up one.
    key(
        &mut harness,
        Modifiers::ALT | Modifiers::SHIFT,
        Key::ArrowDown,
    );
    key(&mut harness, Modifiers::ALT, Key::ArrowUp);
    key(&mut harness, Modifiers::NONE, Key::Enter);

    let app = &harness.state().1;
    let want = vec![
        projects[1].clone(),
        projects[0].clone(),
        projects[2].clone(),
    ];
    assert_eq!(app.projects, want);
    assert!(
        !app.located.project_dir.ends_with("beta"),
        "the board does not change"
    );
    let written = std::fs::read_to_string(&config).unwrap();
    assert!(written.starts_with("# mine\n[ai]\n"), "{written}");
    let order: Vec<String> = want
        .iter()
        .map(|path| format!("{:?}", path.display().to_string()))
        .collect();
    assert!(
        written.contains(&format!("projects = [{}]", order.join(", "))),
        "{written}"
    );
}

#[test]
fn cancel_puts_the_order_back() {
    let (_scratch, config, projects, app) = three_projects("menu-cancel");
    let mut harness = kittest(app);
    open(&mut harness);
    press(&mut harness, "Reorder");
    key(&mut harness, Modifiers::ALT, Key::ArrowDown);
    key(&mut harness, Modifiers::NONE, Key::Escape);
    assert!(shown(&harness, "Add project…"), "back to the normal menu");
    assert_eq!(harness.state().1.projects, projects);
    assert!(
        !std::fs::read_to_string(&config)
            .unwrap()
            .contains("projects")
    );
}

fn picked() -> Vec<PathBuf> {
    PICKED.with(|picked| picked.borrow().clone())
}

thread_local! {
    static PICKED: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[test]
fn an_added_project_opens_at_the_top_of_the_list() {
    let (scratch, _config, projects, mut app) = three_projects("menu-add");
    let delta = scratch.join("delta");
    std::fs::create_dir_all(delta.join("issues/todo")).unwrap();
    PICKED.with(|picked| *picked.borrow_mut() = vec![delta.clone()]);
    app.folder_picker = picked;
    let mut harness = kittest(app);
    open(&mut harness);
    press(&mut harness, "Add project…");
    let app = &harness.state().1;
    assert_eq!(app.located.project_dir, delta);
    assert_eq!(app.ordered_projects()[0], delta);
    assert_eq!(app.ordered_projects()[1..], projects[..]);
}

/// A section's count sits at the far end of its head, over its rows' edge.
#[test]
fn a_sections_count_ends_its_head() {
    let (_scratch, _config, _projects, app) = three_projects("menu-head");
    let mut harness = kittest(app);
    open(&mut harness);
    let title = harness.get_by_label("PROJECTS").rect();
    let count = harness.get_by_label("3").rect();
    assert!(
        count.left() > title.right() + 100.0,
        "{title:?} then {count:?}"
    );
}
