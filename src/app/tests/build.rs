//! The AI build's tests: Ready's Play, its queue, stopping and failing it.

use super::*;

/// A board with `ready/001-a.md` and `ready/002-b.md`, and an empty Todo,
/// Doing and Done; its build agent runs `command`.
fn board(tag: &str, command: &str) -> (Scratch, IboardApp) {
    let dir = scratch_dir(tag);
    for column in ["todo", "ready", "doing", "done"] {
        std::fs::create_dir_all(dir.join("issues").join(column)).unwrap();
    }
    for (name, title) in [("001-a.md", "A"), ("002-b.md", "B")] {
        std::fs::write(dir.join("issues/ready").join(name), format!("# {title}\n")).unwrap();
    }
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.build_agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: command.to_string(),
        configured: true,
        ..AgentConfig::default()
    };
    (dir, app)
}

fn ready(app: &IboardApp) -> Vec<PathBuf> {
    let column = app
        .board
        .columns
        .iter()
        .find(|column| discovery::is_ready_column(&column.dir))
        .unwrap();
    column
        .issues
        .iter()
        .map(|issue| issue.path.clone())
        .collect()
}

fn in_column(dir: &Path, column: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join("issues").join(column))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn play_builds_ready_top_to_bottom_from_the_project() {
    let (dir, mut app) = board("build-play", "head -1 {issue} >> build.log # {prompt}");
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let head = ctx
        .read_response(egui::Id::new(("issue_card", &ready(&app)[0])))
        .unwrap()
        .rect
        .center_top()
        - egui::vec2(0.0, 20.0);
    draw(&ctx, &mut app, vec![moved(head)]);
    let play = ctx
        .read_response(egui::Id::new(("build", 1)))
        .expect("the hovered Ready header offers Play")
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(play)]);
    draw(&ctx, &mut app, vec![moved(play), button(play, true)]);
    draw(&ctx, &mut app, vec![button(play, false)]);
    assert!(app.session.build.is_some(), "Play starts the build");
    assert_eq!(in_column(&dir, "ready").len(), 1, "the first one moved on");

    assert!(until(|| {
        app.poll_build(&ctx);
        app.session.build.is_none()
    }));
    assert_eq!(
        std::fs::read_to_string(dir.join("build.log")).unwrap(),
        "# A\n# B\n",
        "one at a time, top first, from the project folder"
    );
    assert!(in_column(&dir, "ready").is_empty());
    assert_eq!(in_column(&dir, "doing").len(), 2);
    assert_eq!(app.error, None);
}

#[test]
fn a_failed_build_stops_the_run() {
    let (dir, mut app) = board("build-fail", "echo broke >&2; exit 3 # {prompt}");
    let ctx = themed_ctx();
    app.apply(vec![Intent::Build(ready(&app))]);
    assert!(until(|| {
        app.poll_build(&ctx);
        app.session.build.is_none()
    }));
    assert!(app.error.as_deref().is_some_and(|e| e.contains("a.md")));
    assert_eq!(in_column(&dir, "ready"), ["002-b.md"], "the rest waits");
}

#[test]
fn stop_ends_the_build_and_keeps_the_issue_where_it_is() {
    let (dir, mut app) = board("build-stop", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Build(ready(&app))]);
    assert_eq!(app.build_place(), Some((1, 2)));
    app.apply(vec![Intent::StopBuild]);
    assert!(app.session.build.is_none());
    assert_eq!(in_column(&dir, "doing").len(), 1);
    assert_eq!(in_column(&dir, "ready"), ["002-b.md"]);
}

#[test]
fn without_a_build_agent_play_opens_its_settings() {
    let (_dir, mut app) = board("build-unset", "true # {prompt}");
    app.build_agent = AgentConfig::default();
    app.apply(vec![Intent::Build(ready(&app))]);
    assert!(app.session.build.is_none());
    assert_eq!(
        app.settings.as_ref().map(|dialog| dialog.tab),
        Some(settings_ui::Tab::Build)
    );
}

fn click(ctx: &egui::Context, app: &mut IboardApp, id: egui::Id) {
    let at = ctx
        .read_response(id)
        .expect("the button is drawn")
        .rect
        .center();
    draw(ctx, app, vec![moved(at)]);
    draw(ctx, app, vec![moved(at), button(at, true)]);
    draw(ctx, app, vec![button(at, false)]);
}

#[test]
fn the_card_at_work_stops_the_build_and_doing_retries_it() {
    let (dir, mut app) = board("build-card", "sleep 30 # {prompt}");
    let ctx = themed_ctx();
    app.apply(vec![Intent::Build(ready(&app))]);
    let taken = app.building_now()[0].0.clone();
    assert_eq!(taken.parent(), Some(dir.join("issues/doing").as_path()));

    app.apply(vec![Intent::Select(taken.clone())]);
    draw(&ctx, &mut app, vec![]);
    assert!(
        ctx.read_response(egui::Id::new(("card_delete", &taken)))
            .is_none()
    );
    assert!(
        ctx.read_response(egui::Id::new(("card_retry_build", &taken)))
            .is_none(),
        "no retry while its agent runs"
    );
    assert!(
        ctx.read_response(egui::Id::new(("card_build_timer", &taken)))
            .is_some(),
        "the card shows how long its agent has been at work"
    );
    click(&ctx, &mut app, egui::Id::new(("card_stop_build", &taken)));
    assert!(
        app.session.build.is_none(),
        "Stop on the card ends the build"
    );
    assert_eq!(in_column(&dir, "doing"), ["a.md"]);

    draw(&ctx, &mut app, vec![]);
    click(&ctx, &mut app, egui::Id::new(("card_retry_build", &taken)));
    assert_eq!(app.building_now()[0].0, taken);
    assert_eq!(app.build_place(), Some((1, 1)), "a retry builds one issue");
    assert_eq!(in_column(&dir, "doing"), ["a.md"], "it stays where it is");
    assert_eq!(in_column(&dir, "ready"), ["002-b.md"]);
}

#[test]
fn retrying_the_issue_at_work_keeps_the_rest_of_the_run() {
    let (dir, mut app) = board("build-retry", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Build(ready(&app))]);
    let (taken, started) = app.building_now()[0].clone();
    app.apply(vec![Intent::RetryBuild(taken.clone())]);
    let (again, restarted) = app.building_now()[0].clone();
    assert_eq!(again, taken);
    assert!(restarted > started, "the clock starts over");
    assert_eq!(app.build_place(), Some((1, 2)));
    assert_eq!(in_column(&dir, "ready"), ["002-b.md"]);
}

#[test]
fn the_agent_log_shows_what_the_build_did_on_the_issue() {
    let (_dir, mut app) = board("build-log", "echo working on it # {prompt}");
    let ctx = themed_ctx();
    app.apply(vec![Intent::Build(vec![ready(&app)[0].clone()])]);
    let taken = app.building_now()[0].0.clone();
    assert!(app.has_agent_log(&taken), "offered while the agent runs");
    assert!(until(|| {
        app.poll_build(&ctx);
        app.session.build.is_none()
    }));
    assert!(app.has_agent_log(&taken), "and once it is done");
    assert!(!app.has_agent_log(&ready(&app)[0]));

    app.open_issue(taken.clone());
    app.toggle_agent_log(&taken);
    draw(&ctx, &mut app, vec![]);
    let text = &app.session.agent_log.as_ref().unwrap().text;
    assert!(text.starts_with("── ") && text.contains("custom"), "{text}");
    assert!(text.ends_with("working on it\n✓ Finished\n"), "{text}");

    app.toggle_agent_log(&taken);
    assert!(
        app.session.agent_log.is_none(),
        "the button closes it again"
    );
}

#[test]
fn two_agents_build_two_issues_at_once() {
    let (dir, mut app) = board("build-two", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Agents(2), Intent::Build(ready(&app))]);
    assert_eq!(app.building_now().len(), 2);
    assert!(in_column(&dir, "ready").is_empty());
    assert_eq!(in_column(&dir, "doing"), ["a.md", "b.md"]);
}

#[test]
fn one_agent_takes_the_first_issue_and_more_take_the_next() {
    let (dir, mut app) = board("build-more", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Build(ready(&app))]);
    assert_eq!(in_column(&dir, "doing"), ["a.md"], "one agent, the first");
    app.apply(vec![Intent::Agents(2)]);
    assert_eq!(app.building_now().len(), 2, "the new agent takes the next");
    app.apply(vec![Intent::Agents(1)]);
    assert_eq!(
        app.building_now().len(),
        2,
        "fewer let the ones at work finish"
    );
}

#[test]
fn no_agents_stops_the_build_and_at_most_sixteen() {
    let (dir, mut app) = board("build-none", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Build(ready(&app))]);
    app.apply(vec![Intent::Agents(0)]);
    assert!(app.session.build.is_none());
    assert_eq!(in_column(&dir, "ready"), ["002-b.md"]);
    app.apply(vec![Intent::Agents(40)]);
    assert_eq!(app.session.agents, 16);
}

#[test]
fn doing_header_adds_and_removes_agents() {
    let (_dir, mut app) = board("build-stepper", "true # {prompt}");
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    click(&ctx, &mut app, egui::Id::new(("more_agents", 2)));
    assert_eq!(app.session.agents, 2);
    click(&ctx, &mut app, egui::Id::new(("fewer_agents", 2)));
    click(&ctx, &mut app, egui::Id::new(("fewer_agents", 2)));
    assert_eq!(app.session.agents, 0);
}

#[test]
fn stopping_one_card_keeps_the_other_agent_at_work() {
    let (_dir, mut app) = board("build-stop-one", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Agents(2), Intent::Build(ready(&app))]);
    let first = app.building_now()[0].0.clone();
    app.apply(vec![Intent::StopAgent(first)]);
    assert_eq!(app.building_now().len(), 1);
}

#[test]
fn while_play_is_pressed_an_issue_dropped_in_ready_is_built() {
    let (dir, mut app) = board("build-watch-drop", "true # {prompt}");
    let ctx = themed_ctx();
    std::fs::write(dir.join("issues/todo/003-c.md"), "# C\n").unwrap();
    app.apply(vec![Intent::Agents(2), Intent::Build(ready(&app))]);
    assert!(until(|| {
        app.poll_build(&ctx);
        app.session.build.is_none()
    }));
    assert!(
        app.watching_ready(),
        "Play stays pressed once Ready is empty"
    );

    let todo = dir.join("issues/todo/003-c.md");
    app.apply(vec![Intent::Move {
        path: todo,
        to: dir.join("issues/ready"),
    }]);
    app.poll_build(&ctx);
    assert_eq!(in_column(&dir, "doing"), ["a.md", "b.md", "c.md"]);
    assert!(in_column(&dir, "ready").is_empty());
}

#[test]
fn while_play_is_pressed_ready_is_rescanned_for_other_processes_issues() {
    let (dir, mut app) = board("build-watch-disk", "sleep 30 # {prompt}");
    let ctx = themed_ctx();
    app.auto = false;
    app.apply(vec![Intent::Agents(3), Intent::Build(ready(&app))]);
    std::fs::write(dir.join("issues/ready/003-c.md"), "# C\n").unwrap();
    assert!(until(|| {
        frame(&ctx, &mut app, vec![]);
        app.building_now().len() == 3
    }));
    assert_eq!(in_column(&dir, "doing"), ["a.md", "b.md", "c.md"]);
}

#[test]
fn stop_stops_watching_ready() {
    let (dir, mut app) = board("build-watch-stop", "sleep 30 # {prompt}");
    let ctx = themed_ctx();
    app.apply(vec![Intent::Build(ready(&app)), Intent::StopBuild]);
    assert!(!app.watching_ready());
    std::fs::write(dir.join("issues/ready/003-c.md"), "# C\n").unwrap();
    app.refresh();
    app.poll_build(&ctx);
    assert!(app.session.build.is_none());
    assert_eq!(in_column(&dir, "ready"), ["002-b.md", "003-c.md"]);
}

#[test]
fn a_done_card_opens_its_agent_log() {
    let (dir, _) = board("build-done-log", "true # {prompt}");
    let done = dir.join("issues/done/003-c.md");
    std::fs::write(&done, "# C\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let log = crate::build_log::path(&app.board.root, &done);
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, "built it\n").unwrap();
    app.session.selected = Some(done.clone());
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let at = ctx
        .read_response(egui::Id::new(("card_agent_log", &done)))
        .expect("Done cards offer their agent log")
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(matches!(&app.session.screen, Screen::Detail(path) if *path == done));
    draw(&ctx, &mut app, vec![]);
    assert_eq!(app.session.agent_log.as_ref().unwrap().text, "built it\n");

    std::fs::remove_file(&log).unwrap();
    app.back_to_board();
    app.apply(vec![Intent::AgentLog(done.clone())]);
    assert!(matches!(app.session.screen, Screen::Board));
    assert!(app.session.notice.is_some(), "says no agent built it");
}
