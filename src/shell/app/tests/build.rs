//! The AI build's tests: Ready's Play, its queue, stopping and failing it.

use egui_kittest::kittest::Queryable;

use super::*;
use crate::agent::service::{Choice, Field, FieldKind, Permission, Question, QuestionKind};
use crate::build_log::{self, Entry};
use crate::shell::app::build::{self, Builds};
use crate::shell::app::card::CardBuild;
use crate::shell::app::intent::Intent;

/// A board with `ready/001-a.md` and `ready/002-b.md`, and an empty Todo,
/// Doing and Done; its build agent runs `command`.
pub(super) fn board(tag: &str, command: &str) -> (Scratch, IboardApp) {
    let dir = scratch_dir(tag);
    for column in ["todo", "ready", "doing", "done"] {
        std::fs::create_dir_all(dir.join("issues").join(column)).unwrap();
    }
    for (name, title) in [("001-a.md", "A"), ("002-b.md", "B")] {
        std::fs::write(dir.join("issues/ready").join(name), format!("# {title}\n")).unwrap();
    }
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.prefs.build_agent = fake_agent(command);
    (dir, app)
}

pub(super) fn ready(app: &IboardApp) -> Vec<PathBuf> {
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

/// Stops the build, then waits until its agents logged how all `runs` ended:
/// one still at work while the scratch folder is removed could write its log
/// in it and keep it from going.
fn stop_agents(app: &mut IboardApp, dir: &Path, runs: usize) {
    app.stop_build();
    let ended = |log: std::fs::DirEntry| {
        let text = std::fs::read_to_string(log.path()).unwrap_or_default();
        let entries = build_log::parse(&text);
        entries
            .iter()
            .filter(|entry| matches!(entry, Entry::Outcome { .. }))
            .count()
    };
    let logs = dir.join("issues").join(build_log::DIR);
    let all_ended = || {
        let logs = std::fs::read_dir(&logs).into_iter().flatten().flatten();
        logs.map(ended).sum::<usize>() == runs
    };
    assert!(until(all_ended), "the {runs} runs never all ended");
}

#[test]
fn play_builds_ready_top_to_bottom_from_the_project() {
    let (dir, mut app) = board("build-play", "head -1 {issue} >> build.log # {prompt}");
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let head = ctx
        .read_response(egui::Id::unique(("issue_card", &ready(&app)[0])))
        .unwrap()
        .rect
        .center_top()
        - egui::vec2(0.0, 20.0);
    draw(&ctx, &mut app, vec![moved(head)]);
    let play = ctx
        .read_response(egui::Id::unique(("build", 1)))
        .expect("the hovered Ready header offers Play")
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(play)]);
    draw(&ctx, &mut app, vec![moved(play), button(play, true)]);
    draw(&ctx, &mut app, vec![button(play, false)]);
    assert!(app.session.build.is_some(), "Play starts the build");
    assert_eq!(in_column(&dir, "ready").len(), 1, "the first one moved on");

    assert!(until(|| {
        app.poll_build();
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
fn a_build_prompt_names_the_issue_from_the_project() {
    let (dir, mut app) = board(
        "build-prompt",
        "cat \"$IBOARD_PROMPT\" > prompt.md # {prompt}",
    );
    app.apply(vec![Intent::Build(vec![ready(&app)[0].clone()])]);
    assert!(until(|| {
        app.poll_build();
        app.session.build.is_none()
    }));
    let prompt = std::fs::read_to_string(dir.join("prompt.md")).unwrap();
    assert!(prompt.contains("`issues/doing/a.md`"), "{prompt}");
}

#[test]
fn a_failed_build_stops_the_run() {
    let (dir, mut app) = board("build-fail", "echo broke >&2; exit 3 # {prompt}");
    app.apply(vec![Intent::Build(ready(&app))]);
    assert!(until(|| {
        app.poll_build();
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
    stop_agents(&mut app, &dir, 1);
}

#[test]
fn without_a_build_agent_play_opens_its_settings() {
    let (_dir, mut app) = board("build-unset", "true # {prompt}");
    app.prefs.build_agent = AgentConfig::default();
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
        ctx.read_response(egui::Id::unique(("card_delete", &taken)))
            .is_none()
    );
    assert!(
        ctx.read_response(egui::Id::unique(("card_retry_build", &taken)))
            .is_none(),
        "no retry while its agent runs"
    );
    assert!(
        ctx.read_response(egui::Id::unique(("card_build_timer", &taken)))
            .is_some(),
        "the card shows how long its agent has been at work"
    );
    click(
        &ctx,
        &mut app,
        egui::Id::unique(("card_stop_build", &taken)),
    );
    assert!(
        app.session.build.is_none(),
        "Stop on the card ends the build"
    );
    assert_eq!(in_column(&dir, "doing"), ["a.md"]);

    draw(&ctx, &mut app, vec![]);
    click(
        &ctx,
        &mut app,
        egui::Id::unique(("card_retry_build", &taken)),
    );
    assert_eq!(app.building_now()[0].0, taken);
    assert_eq!(app.build_place(), Some((1, 1)), "a retry builds one issue");
    assert_eq!(in_column(&dir, "doing"), ["a.md"], "it stays where it is");
    assert_eq!(in_column(&dir, "ready"), ["002-b.md"]);
    stop_agents(&mut app, &dir, 2);
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
    stop_agents(&mut app, &dir, 2);
}

#[test]
fn the_agent_log_shows_what_the_build_did_on_the_issue() {
    let (_dir, mut app) = board("build-log", "echo working on it # {prompt}");
    let ctx = themed_ctx();
    app.apply(vec![Intent::Build(vec![ready(&app)[0].clone()])]);
    let taken = app.building_now()[0].0.clone();
    assert!(app.has_agent_log(&taken), "offered while the agent runs");
    assert!(until(|| {
        app.poll_build();
        app.session.build.is_none()
    }));
    assert!(app.has_agent_log(&taken), "and once it is done");
    assert!(!app.has_agent_log(&ready(&app)[0]));

    app.open_issue(taken.clone());
    assert!(app.agent_log_open(&taken), "a Doing issue opens its log");
    draw(&ctx, &mut app, vec![]);
    let text = &app.session.agent_log.as_ref().unwrap().text;
    assert!(text.starts_with("── ") && text.contains("custom"), "{text}");
    assert!(text.contains("working on it\n✓ Finished in 00:0"), "{text}");

    app.toggle_agent_log(&taken);
    assert!(
        app.session.agent_log.is_none(),
        "the button closes it again"
    );
}

#[test]
fn a_finished_build_does_not_enter_the_bell() {
    let (_dir, mut app) = board("build-notify", "printf done # {prompt}");
    app.apply(vec![Intent::Build(vec![ready(&app)[0].clone()])]);
    assert!(until(|| {
        app.poll_build();
        app.session.build.is_none()
    }));
    assert_eq!(app.notification_count(), 0);
    let harness = kittest(app);
    assert!(
        harness.get_by_label("Notifications").rect().width() > 0.0,
        "the bell has no count once the build finished"
    );
    assert!(
        harness.query_by_label("Notifications, 1 waiting").is_none(),
        "only a waiting question counts"
    );
}

#[test]
fn an_instruction_after_the_build_starts_an_ask_and_stays_in_the_log() {
    let (_dir, mut app) = board("build-instruction", "printf done # {prompt}");
    let path = ready(&app)[0].clone();
    app.apply(vec![Intent::Build(vec![path.clone()])]);
    assert!(until(|| {
        app.poll_build();
        app.session.build.is_none()
    }));
    app.open_issue(path.clone());
    app.send_instruction(path.clone(), "add a test".to_string());
    let log = crate::build_log::load(&crate::build_log::path(&app.board.root, &path)).unwrap();
    assert!(log.contains("▸ add a test"), "{log}");
    assert!(until(|| {
        app.poll_asks();
        app.session.asks.is_empty()
    }));
}

/// Bug: a renumber in Ready closed the agent log open beside one of its
/// issues. Deleting the issue closes it.
#[test]
fn the_agent_log_follows_its_issue() {
    let (dir, mut app) = board("build-log-renumber", "true # {prompt}");
    let ready_dir = dir.join("issues/ready");
    let (a, b) = (ready_dir.join("001-a.md"), ready_dir.join("002-b.md"));
    let log = build_log::path(&app.board.root, &a);
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, "built it\n").unwrap();
    app.apply(vec![Intent::AgentLog(a.clone())]);
    app.place_issue(&b, &ready_dir, Some(a));
    let a = ready_dir.join("002-a.md");
    assert!(
        app.agent_log_open(&a),
        "it stays beside the renumbered issue"
    );
    draw(&themed_ctx(), &mut app, vec![]);
    assert_eq!(app.session.agent_log.as_ref().unwrap().text, "built it\n");

    app.prefs.confirm_delete = false;
    app.request_delete(a);
    assert!(app.session.agent_log.is_none(), "and goes with it");
}

#[test]
fn two_agents_build_two_issues_at_once() {
    let (dir, mut app) = board("build-two", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Agents(2), Intent::Build(ready(&app))]);
    assert_eq!(app.building_now().len(), 2);
    assert!(in_column(&dir, "ready").is_empty());
    assert_eq!(in_column(&dir, "doing"), ["a.md", "b.md"]);
    stop_agents(&mut app, &dir, 2);
}

/// The frame where one agent lands still wakes up for the one at work.
#[test]
fn an_agent_still_at_work_keeps_the_frame_awake() {
    let (dir, mut app) = board("build-wake", "grep -q A {issue} || sleep 30 # {prompt}");
    let ctx = themed_ctx();
    app.apply(vec![Intent::Agents(2), Intent::Build(ready(&app))]);
    let mut wake = Duration::MAX;
    assert!(until(|| {
        let output = ctx.run_ui(egui::RawInput::default(), |ui| app.frame_logic(ui.ctx()));
        wake = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        app.building_now().len() == 1
    }));
    assert!(wake <= job::POLL_EVERY, "it wakes after {wake:?}");
    stop_agents(&mut app, &dir, 2);
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
    stop_agents(&mut app, &dir, 2);
}

#[test]
fn doing_header_agents_are_saved_and_restored() {
    let (dir, mut app) = board("build-agents-save", "true # {prompt}");
    let config = dir.join("iboard.toml");
    app.config_file = Some(config.clone());
    app.apply(vec![Intent::Agents(4)]);
    assert_eq!(app.prefs.agents, 4);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "[build]\nagents = 4\n"
    );

    let mut restarted = IboardApp::from_located(discovery::locate(&dir).unwrap());
    restarted.config_file = Some(config.clone());
    restarted.load_preferences(&mut UserConfig::load(Some(&config)));
    assert_eq!(restarted.prefs.agents, 4);
    assert_eq!(restarted.session.agents, 4);
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
    stop_agents(&mut app, &dir, 1);
}

#[test]
fn doing_header_adds_and_removes_agents() {
    let (_dir, app) = board("build-stepper", "true # {prompt}");
    let mut harness = kittest(app);
    let agents = "Build agents working at once: 1 agent";
    assert!(
        harness.query_by_label(agents).is_none(),
        "Doing shows only its count at rest"
    );
    harness.get_by_label("Doing").hover();
    harness.run_steps(2);
    let selector = harness.get_by_label(agents);
    let anchor = selector.rect();
    assert!(
        harness.query_by_label("One more build agent").is_none(),
        "closed at rest"
    );
    selector.click();
    harness.run_steps(2);
    let more = harness.get_by_label("One more build agent").rect();
    assert!(more.min.y >= anchor.max.y, "{more:?} sits under {anchor:?}");
    press(&mut harness, "One more build agent");
    assert_eq!(harness.state().1.session.agents, 2);
    press(&mut harness, "One build agent fewer");
    press(&mut harness, "One build agent fewer");
    assert_eq!(harness.state().1.session.agents, 0);
    assert!(harness.state().1.session.build.is_none());
}

#[test]
fn stopping_one_card_keeps_the_other_agent_at_work() {
    let (dir, mut app) = board("build-stop-one", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Agents(2), Intent::Build(ready(&app))]);
    let first = app.building_now()[0].0.clone();
    app.apply(vec![Intent::StopAgent(first)]);
    assert_eq!(app.building_now().len(), 1);
    stop_agents(&mut app, &dir, 2);
}

#[test]
fn while_play_is_pressed_an_issue_dropped_in_ready_is_built() {
    let (dir, mut app) = board("build-watch-drop", "true # {prompt}");
    std::fs::write(dir.join("issues/todo/003-c.md"), "# C\n").unwrap();
    app.apply(vec![Intent::Agents(2), Intent::Build(ready(&app))]);
    assert!(until(|| {
        app.poll_build();
        app.session.build.is_none()
    }));
    assert!(
        app.watching_ready(),
        "Play stays pressed once Ready is empty"
    );

    let todo = dir.join("issues/todo/003-c.md");
    app.move_issue(&todo, &dir.join("issues/ready")).unwrap();
    app.poll_build();
    assert_eq!(in_column(&dir, "doing"), ["a.md", "b.md", "c.md"]);
    assert!(in_column(&dir, "ready").is_empty());
    stop_agents(&mut app, &dir, 3);
}

#[test]
fn while_play_is_pressed_ready_is_rescanned_for_other_processes_issues() {
    let (dir, mut app) = board("build-watch-disk", "sleep 30 # {prompt}");
    let ctx = themed_ctx();
    app.prefs.auto = false;
    app.apply(vec![Intent::Agents(3), Intent::Build(ready(&app))]);
    std::fs::write(dir.join("issues/ready/003-c.md"), "# C\n").unwrap();
    assert!(until(|| {
        frame(&ctx, &mut app, vec![]);
        app.building_now().len() == 3
    }));
    assert_eq!(in_column(&dir, "doing"), ["a.md", "b.md", "c.md"]);
    stop_agents(&mut app, &dir, 3);
}

#[test]
fn stop_stops_watching_ready() {
    let (dir, mut app) = board("build-watch-stop", "sleep 30 # {prompt}");
    app.apply(vec![Intent::Build(ready(&app)), Intent::StopBuild]);
    assert!(!app.watching_ready());
    std::fs::write(dir.join("issues/ready/003-c.md"), "# C\n").unwrap();
    app.refresh();
    app.poll_build();
    assert!(app.session.build.is_none());
    assert_eq!(in_column(&dir, "ready"), ["002-b.md", "003-c.md"]);
    stop_agents(&mut app, &dir, 1);
}

/// Bug: a renumber in Ready dropped the build's queued issues and queued
/// them again at the end, as new arrivals.
#[test]
fn a_queued_issue_keeps_its_place_through_a_renumber() {
    let (dir, mut app) = board("build-renumber", "sleep 30 # {prompt}");
    let ready_dir = dir.join("issues/ready");
    std::fs::write(ready_dir.join("003-c.md"), "# C\n").unwrap();
    app.refresh();
    app.apply(vec![Intent::Build(ready(&app))]);
    let (b, c) = (ready_dir.join("002-b.md"), ready_dir.join("003-c.md"));
    app.place_issue(&b, &ready_dir, Some(c));
    assert_eq!(in_column(&dir, "ready"), ["001-b.md", "002-c.md"]);
    app.poll_build();
    app.apply(vec![Intent::Agents(2)]);
    app.apply(vec![Intent::StopAgent(dir.join("issues/doing/a.md"))]);
    assert_eq!(app.build_place(), Some((2, 3)), "B is second of three");
    stop_agents(&mut app, &dir, 2);
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
        .read_response(egui::Id::unique(("card_agent_log", &done)))
        .expect("Done cards offer their agent log")
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(matches!(&app.session.screen, Screen::Detail(path, _) if *path == done));
    draw(&ctx, &mut app, vec![]);
    assert_eq!(app.session.agent_log.as_ref().unwrap().text, "built it\n");

    std::fs::remove_file(&log).unwrap();
    app.back_to_board();
    app.apply(vec![Intent::AgentLog(done.clone())]);
    assert!(matches!(app.session.screen, Screen::Board));
    assert!(app.session.notice.is_some(), "says no agent built it");
}

#[test]
fn a_done_card_shows_how_long_its_agent_took_until_hovered() {
    let (dir, _) = board("build-done-timer", "true # {prompt}");
    let done = dir.join("issues/done/003-c.md");
    std::fs::write(&done, "# C\n").unwrap();
    let log = crate::build_log::path(&dir.join("issues"), &done);
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, "── a · custom\n✓ Finished in 02:05\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    assert_eq!(app.facts.built_in(&done), Some(Duration::from_secs(125)));

    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    draw(&ctx, &mut app, vec![]);
    let actions = |ctx: &egui::Context| {
        ctx.read_response(egui::Id::unique(("card_agent_log", &done)))
            .is_some()
    };
    assert!(!actions(&ctx), "the timer stands where the actions go");
    let card = ctx
        .read_response(egui::Id::unique(("issue_card", &done)))
        .unwrap()
        .rect
        .center();
    for _ in 0..3 {
        draw(&ctx, &mut app, vec![moved(card)]);
    }
    assert!(actions(&ctx), "hovering the card shows its actions");
}

/// A `[manual]` issue is not code: Play leaves it in Ready and builds the rest.
#[test]
fn play_leaves_manual_issues_in_ready() {
    let (dir, mut app) = board("build-manual", "printf ok # {prompt}");
    std::fs::write(dir.join("issues/ready/001-a.md"), "# [manual] A\n").unwrap();
    app.refresh();
    app.start_build(ready(&app));
    assert_eq!(in_column(&dir, "ready"), ["001-a.md"]);
    assert_eq!(in_column(&dir, "doing"), ["b.md"]);
    stop_agents(&mut app, &dir, 1);
}

/// A `[manual]` issue in Doing offers no Retry, and a retry leaves it alone.
#[test]
fn a_manual_issue_in_doing_is_never_retried() {
    let (dir, mut app) = board("build-manual-retry", "printf ok # {prompt}");
    let manual = dir.join("issues/doing/quote.md");
    std::fs::write(&manual, "# [manual] Quote\n").unwrap();
    std::fs::write(dir.join("issues/doing/code.md"), "# Code\n").unwrap();
    app.refresh();
    let doing = app
        .board
        .columns
        .iter()
        .find(|column| discovery::is_doing_column(&column.dir))
        .unwrap();
    let builds = Builds {
        running: &[],
        agents: 1,
        watching: false,
    };
    let offer = |name: &str| {
        let issue = doing
            .issues
            .iter()
            .find(|i| i.path.ends_with(name))
            .unwrap();
        build::card_build(doing, issue, builds)
    };
    assert!(offer("quote.md").is_none());
    assert!(matches!(offer("code.md"), Some(CardBuild::Retry)));
    app.retry_build(manual);
    assert!(app.session.build.is_none());
}

#[test]
fn doing_keeps_retry_at_no_agents_and_a_retry_brings_one_back() {
    let (dir, mut app) = board("build-retry-none", "sleep 30 # {prompt}");
    let ctx = themed_ctx();
    app.apply(vec![Intent::Build(ready(&app))]);
    let taken = app.building_now()[0].0.clone();
    app.apply(vec![Intent::Agents(0), Intent::Select(taken.clone())]);
    draw(&ctx, &mut app, vec![]);
    click(
        &ctx,
        &mut app,
        egui::Id::unique(("card_retry_build", &taken)),
    );
    assert_eq!(app.building_now()[0].0, taken);
    assert_eq!(app.session.agents, 1);
    stop_agents(&mut app, &dir, 2);
}

/// Bug: a deleted issue a build agent works on kept it running and counted.
/// It asks first; agreed, the agent stops and the next issue is taken.
#[test]
fn deleting_an_issue_at_work_asks_to_stop_its_agent() {
    let (dir, mut app) = board("build-delete", "sleep 30 # {prompt}");
    app.prefs.confirm_delete = false;
    app.apply(vec![Intent::Build(ready(&app))]);
    let taken = app.building_now()[0].0.clone();

    app.request_delete(taken.clone());
    let Some(Confirm::Stop(removal, paths)) = app.session.confirm.clone() else {
        panic!("it asks to stop the agent");
    };
    app.remove_anyway(removal, &paths);
    assert!(!taken.exists() && app.building_now().is_empty());
    app.poll_build();
    assert_eq!(app.build_place(), Some((1, 1)), "the next issue is taken");
    assert_eq!(in_column(&dir, "doing"), ["b.md"]);
    stop_agents(&mut app, &dir, 2);
}

pub(super) fn form_question() -> Question {
    Question {
        session: "ses_1".into(),
        id: "frm_1".into(),
        title: "Which approach?".into(),
        kind: QuestionKind::Form(vec![Field {
            key: "approach".into(),
            title: "Approach".into(),
            description: String::new(),
            required: true,
            hidden: false,
            kind: FieldKind::Text {
                options: vec![
                    Choice {
                        value: "fast".into(),
                        label: "Fast".into(),
                        description: "Ship it today.".into(),
                    },
                    Choice {
                        value: "careful".into(),
                        label: "Careful".into(),
                        description: "Check every edge first.".into(),
                    },
                ],
                placeholder: String::new(),
                default: String::new(),
                custom: true,
            },
        }]),
    }
}

pub(super) fn permission_question() -> Question {
    Question {
        session: "ses_1".into(),
        id: "per_1".into(),
        title: "Allow bash?".into(),
        kind: QuestionKind::Permission(Permission {
            action: "bash".into(),
            resources: vec!["git push origin main".into()],
            message: Some("Push the branch?".into()),
        }),
    }
}

/// Adoption asks the service about every project of a group's board, not
/// only the one the window had open before the switch.
#[test]
fn adoption_asks_every_project_of_a_group_board() {
    let parent = scratch_dir("build-adopt-projects");
    let project = |name: &str| {
        let dir = parent.join(name);
        std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
        dir
    };
    let (one, two) = (project("one"), project("two"));
    let mut app = IboardApp::from_located(discovery::locate(&one).unwrap());
    app.workspace = Some(crate::workspace::Workspace {
        groups: vec![crate::workspace::Group {
            name: "Both".into(),
            repos: vec![one.clone(), two.clone()],
        }],
    });
    app.open_group("Both");
    settled(&mut app);
    let mut asked = app.build_projects();
    asked.sort();
    assert_eq!(asked, [recents::canonical(&one), recents::canonical(&two)]);

    app.open_project(one.clone());
    settled(&mut app);
    assert_eq!(app.build_projects(), [recents::canonical(&one)]);
}

#[test]
fn a_build_question_shows_on_its_card_and_opens_in_the_ticket() {
    let (_dir, mut app) = board("build-question", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path.clone(), form_question());
    let mut harness = kittest(app);
    assert!(
        harness
            .get_by_label("Answer: Which approach?")
            .rect()
            .width()
            > 0.0,
        "the question is on its card"
    );
    assert!(
        harness
            .get_by_label("Question: Which approach?")
            .rect()
            .width()
            > 0.0,
        "the pin sits at the card's top right"
    );
    assert!(
        harness
            .get_by_label("Notifications, 1 waiting")
            .rect()
            .width()
            > 0.0,
        "the bell counts it"
    );
    harness.get_by_label("Answer: Which approach?").click();
    harness.run_steps(2);
    assert!(
        harness.state().1.agent_log_open(&path),
        "the ticket opens on its agent log"
    );
    assert!(
        harness.state().1.session.answer.is_some(),
        "the question waits in the ticket"
    );
    assert!(harness.get_by_label("Approach").rect().width() > 0.0);
}

#[test]
fn the_bell_lists_a_waiting_question_and_opens_its_ticket() {
    let (_dir, mut app) = board("build-tray", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path.clone(), form_question());
    let mut harness = kittest(app);
    harness.get_by_label("Notifications, 1 waiting").click();
    harness.run_steps(2);
    assert!(
        harness
            .get_by_label("A: Which approach?, just now")
            .rect()
            .width()
            > 0.0,
        "the tray lists the ticket and its question"
    );
    harness.get_by_label("A: Which approach?, just now").click();
    harness.run_steps(2);
    assert!(
        matches!(
            &harness.state().1.session.screen,
            Screen::Detail(open, _) if *open == path
        ),
        "the row opens its ticket"
    );
    assert_eq!(
        harness.state().1.notification_count(),
        1,
        "looking at a question does not clear it"
    );
    assert!(
        harness
            .query_by_label("A: Which approach?, just now")
            .is_none(),
        "the tray closes on the row"
    );
}

#[test]
fn escape_closes_the_tray() {
    let (_dir, mut app) = board("build-tray-escape", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path, form_question());
    let mut harness = kittest(app);
    harness.get_by_label("Notifications, 1 waiting").click();
    harness.run_steps(2);
    assert!(
        harness
            .get_by_label("A: Which approach?, just now")
            .rect()
            .width()
            > 0.0
    );
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label("A: Which approach?, just now")
            .is_none(),
        "esc closes the tray and leaves the board"
    );
}

#[test]
fn the_bell_says_when_no_question_waits() {
    let (_dir, app) = board("bell-empty", "printf done # {prompt}");
    let mut harness = kittest(app);
    harness.get_by_label("Notifications").click();
    harness.run_steps(2);
    assert!(harness.get_by_label("No questions waiting.").rect().width() > 0.0);
    assert!(
        harness
            .get_by_label("Questions from the AI builder land here.")
            .rect()
            .width()
            > 0.0
    );
}

#[test]
fn answering_a_question_takes_it_off_the_board() {
    let (_dir, mut app) = board("build-answer", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    app.seed_question(path.clone(), form_question());
    assert_eq!(app.pending_questions().len(), 1);
    app.open_answer(path);
    app.finish_answer_for_test();
    assert!(until(|| app.poll_answers().is_none()));
    assert!(app.session.answer.is_none());
    assert!(app.pending_questions().is_empty());
    let (notice, _) = app.session.notice.clone().expect("the answer shows");
    assert_eq!(notice, "Answered Which approach?");
}

#[test]
fn a_tool_approval_answers_from_the_same_feed() {
    let (_dir, mut app) = board("build-permission", "sleep 30 # {prompt}");
    let path = ready(&app)[0].clone();
    let project = app.board.project_of(&path).to_path_buf();
    app.seed_question(path, permission_question());
    let mut harness = kittest(app);
    harness.get_by_label("Answer: Allow bash?").click();
    harness.run_steps(2);
    assert!(harness.get_by_label("Allow once").rect().width() > 0.0);
    assert!(
        harness
            .get_by_label(&format!("Always allow in {}/", file_name(&project)))
            .rect()
            .width()
            > 0.0
    );
    assert!(harness.get_by_label("Reject").rect().width() > 0.0);
    harness.get_by_label_contains("Inside ");
}

/// Bug: a queued issue deleted was skipped but still counted as built.
#[test]
fn a_deleted_queued_issue_is_not_counted_as_built() {
    let (dir, mut app) = board("build-delete-queued", "sleep 0.2 # {prompt}");
    app.prefs.confirm_delete = false;
    app.apply(vec![Intent::Build(ready(&app))]);
    app.request_delete(dir.join("issues/ready/002-b.md"));
    assert!(app.session.confirm.is_none());
    assert!(until(|| {
        app.poll_build();
        app.session.build.is_none()
    }));
    let (notice, _) = app.session.notice.clone().unwrap();
    assert_eq!(notice, "Built 1 issue");
}

/// Play stays in Ready's header with no issue in it: it turns the auto-build
/// on and waits for one.
#[test]
fn play_is_offered_in_an_empty_ready() {
    let (_dir, mut app) = board("build-empty-play", "true # {prompt}");
    for issue in ready(&app) {
        std::fs::remove_file(issue).unwrap();
    }
    app.refresh();
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let head = ctx
        .read_response(egui::Id::unique(("column_drop", 1)))
        .unwrap()
        .rect;
    let over = head.center_top() + egui::vec2(0.0, theme::CONTROL_H / 2.0);
    draw(&ctx, &mut app, vec![moved(over)]);
    let play = ctx
        .read_response(egui::Id::unique(("build", 1)))
        .expect("Ready offers Play with no issue in it")
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(play)]);
    draw(&ctx, &mut app, vec![moved(play), button(play, true)]);
    draw(&ctx, &mut app, vec![button(play, false)]);
    assert!(app.watching_ready(), "Play turns the auto-build on");
    assert!(app.session.build.is_none(), "nothing to build yet");
}

/// Ready's empty well says what a drop does while the auto-build waits for a
/// ticket, and stays quiet otherwise.
#[test]
fn an_empty_ready_says_what_the_auto_build_does_with_a_drop() {
    let (_dir, mut app) = project(
        "build-empty-well",
        &[("todo/", ""), ("ready/", ""), ("doing/", ""), ("done/", "")],
    );
    app.prefs.build_agent = fake_agent("true # {prompt}");
    let hint = "Bring a ticket here to process it with AI auto-build";
    let mut harness = kittest(app);
    assert!(
        harness.query_by_label(hint).is_none(),
        "off: nothing to say"
    );
    harness.state_mut().1.apply(vec![Intent::Build(Vec::new())]);
    harness.run_steps(2);
    assert!(
        harness.state().1.watching_ready(),
        "Play waits for a ticket"
    );
    assert!(harness.get_by_label(hint).rect().width() > 0.0);
}
