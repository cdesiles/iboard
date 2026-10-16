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
    assert!(app.build_status().unwrap().contains("(1/2)"));
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
    let taken = app.building_now().unwrap().0.to_path_buf();
    assert_eq!(taken.parent(), Some(dir.join("issues/doing").as_path()));
    assert!(
        app.build_status().unwrap().contains("· 00:0"),
        "time spent shows"
    );

    app.apply(vec![Intent::Select(taken.clone())]);
    draw(&ctx, &mut app, vec![]);
    assert!(
        ctx.read_response(egui::Id::new(("card_delete", &taken)))
            .is_none()
    );
    click(&ctx, &mut app, egui::Id::new(("card_stop_build", &taken)));
    assert!(
        app.session.build.is_none(),
        "Stop on the card ends the build"
    );
    assert_eq!(in_column(&dir, "doing"), ["a.md"]);

    draw(&ctx, &mut app, vec![]);
    click(&ctx, &mut app, egui::Id::new(("card_retry_build", &taken)));
    assert_eq!(
        app.building_now().map(|(path, _)| path),
        Some(taken.as_path())
    );
    assert!(
        !app.build_status().unwrap().contains("(1/"),
        "a retry builds one issue"
    );
    assert_eq!(in_column(&dir, "doing"), ["a.md"], "it stays where it is");
    assert_eq!(in_column(&dir, "ready"), ["002-b.md"]);
}

#[test]
fn retrying_the_issue_at_work_keeps_the_rest_of_the_run() {
    let (dir, mut app) = board("build-retry", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Build(ready(&app))]);
    let (taken, started) = app
        .building_now()
        .map(|(p, t)| (p.to_path_buf(), t))
        .unwrap();
    app.apply(vec![Intent::RetryBuild(taken.clone())]);
    let (again, restarted) = app.building_now().unwrap();
    assert_eq!(again, taken);
    assert!(restarted > started, "the clock starts over");
    assert!(app.build_status().unwrap().contains("(1/2)"));
    assert_eq!(in_column(&dir, "ready"), ["002-b.md"]);
}
