//! A new issue's title landing in the background: written at once as
//! `unnamed-ticket-xxxx`, renamed when the engine answers, left alone when
//! someone got there first. The engines are fake: jobs, or a custom command.

use super::*;
use crate::shell::assets;
use egui_kittest::kittest::Queryable;

const NOTES: &str = "When I export the board I want a checklist.\nOne per column.";

/// A composer with notes, and an engine ready.
fn composer_with_engine(tag: &str) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(dir.join("issues/ready")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.prefs.agent = fake_agent("true # {prompt}");
    write_another(&mut app);
    (dir, todo, app)
}

fn write_another(app: &mut IboardApp) {
    app.open_composer();
    let composer = app.session.composer.as_mut().unwrap();
    composer.engine = Some(Engine::Agent);
    composer.description = NOTES.into();
}

/// An engine that stays silent until its job is dropped.
fn silent_job() -> Job<Result<String, String>> {
    Job::spawn(|cancel| {
        while !cancel.is_cancelled() {
            std::thread::sleep(Duration::from_millis(5));
        }
        Err("never heard".to_string())
    })
}

/// `↩` with no title, and the engine at work: the file written, and its
/// placeholder.
fn added(app: &mut IboardApp) -> (PathBuf, String) {
    app.submit_composer(false);
    assert!(app.session.composer.is_none(), "it closes at once");
    let naming = app.session.namings.last_mut().expect("naming started");
    naming.answer = silent_job();
    let placeholder = naming.placeholder.clone();
    let path = app.session.selected.clone().unwrap();
    (path, placeholder)
}

fn answer(app: &mut IboardApp, placeholder: &str, answer: Result<String, String>) {
    let naming = app
        .session
        .namings
        .iter_mut()
        .find(|naming| naming.placeholder == placeholder)
        .unwrap();
    naming.answer = Job::done(answer);
}

fn poll(app: &mut IboardApp) -> Option<Duration> {
    app.poll_namings(&egui::Context::default())
}

#[test]
fn enter_writes_at_once_and_the_title_lands_later() {
    let (_dir, todo, mut app) = composer_with_engine("naming-lands");
    let (path, placeholder) = added(&mut app);

    assert!(placeholder.starts_with("unnamed-ticket-"), "{placeholder}");
    assert_eq!(placeholder.len(), "unnamed-ticket-xxxx".len());
    assert_eq!(path, todo.join(format!("{placeholder}.md")));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("# {placeholder}\n\n{NOTES}\n")
    );
    assert_eq!(poll(&mut app), Some(job::POLL_EVERY), "the wait wakes up");

    answer(&mut app, &placeholder, Ok("Export checklists".into()));
    assert_eq!(poll(&mut app), None);
    assert!(!path.exists());
    assert_eq!(
        std::fs::read_to_string(todo.join("export-checklists.md")).unwrap(),
        format!("# Export checklists\n\n{NOTES}\n")
    );
    assert_eq!(
        app.session.selected,
        Some(todo.join("export-checklists.md")),
        "the selection follows"
    );
}

#[test]
fn a_fake_command_names_the_issue() {
    let (_dir, todo, mut app) = composer_with_engine("naming-command");
    app.prefs.agent = fake_agent("printf 'Export checklists' # {prompt}");
    app.submit_composer(false);
    assert!(until(|| {
        poll(&mut app);
        app.session.namings.is_empty()
    }));
    assert!(todo.join("export-checklists.md").exists());
}

#[test]
fn several_namings_run_together_and_land_in_any_order() {
    let (_dir, todo, mut app) = composer_with_engine("naming-many");
    let (_, first) = added(&mut app);
    write_another(&mut app);
    let (_, second) = added(&mut app);
    assert_ne!(first, second);

    answer(&mut app, &second, Ok("Second".into()));
    poll(&mut app);
    assert!(todo.join("second.md").exists());
    assert_eq!(app.session.namings.len(), 1, "the first still waits");

    answer(&mut app, &first, Ok("First".into()));
    poll(&mut app);
    assert!(todo.join("first.md").exists());
    assert!(app.session.namings.is_empty());
}

#[test]
fn a_typed_title_is_used_straight_away() {
    let (_dir, todo, mut app) = composer_with_engine("naming-typed");
    app.session.composer.as_mut().unwrap().title = "Board export".into();
    app.submit_composer(false);
    assert!(todo.join("board-export.md").exists());
    assert!(app.session.namings.is_empty(), "no job");
}

/// A tag typed with no title stays on the heading while the title lands.
#[test]
fn a_tag_stays_on_the_title_that_lands() {
    let (_dir, todo, mut app) = composer_with_engine("naming-tag");
    app.session.composer.as_mut().unwrap().title_labels = vec!["bug".to_string()];
    let (path, placeholder) = added(&mut app);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("# [bug] {placeholder}\n\n{NOTES}\n")
    );
    let naming = app.session.namings.last_mut().unwrap();
    naming.answer = Job::done(Ok("Board export".to_string()));
    let ctx = themed_ctx();
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    let landed = std::fs::read_dir(&todo)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| file_name(path).ends_with("board-export.md"))
        .expect("renamed");
    assert_eq!(
        std::fs::read_to_string(landed).unwrap(),
        format!("# [bug] Board export\n\n{NOTES}\n")
    );
}

#[test]
fn without_an_engine_there_is_no_job() {
    let (_dir, todo, mut app) = composer_with_engine("naming-off");
    app.session.composer.as_mut().unwrap().engine = None;
    app.submit_composer(false);
    assert!(
        todo.join("when-i-export-the-board-i-want-a-checklist.md")
            .exists()
    );
    assert!(app.session.namings.is_empty(), "no job");
}

#[test]
fn a_failure_leaves_the_placeholder() {
    let (_dir, _todo, mut app) = composer_with_engine("naming-fails");
    let (path, placeholder) = added(&mut app);
    answer(&mut app, &placeholder, Err("model unavailable".into()));
    assert_eq!(poll(&mut app), None);
    assert!(app.session.namings.is_empty());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("# {placeholder}\n\n{NOTES}\n"),
        "the author's text stays"
    );
}

#[test]
fn past_its_deadline_the_wait_stops_and_the_placeholder_stays() {
    let (_dir, _todo, mut app) = composer_with_engine("naming-deadline");
    let (path, _) = added(&mut app);
    let naming = app.session.namings.last_mut().unwrap();
    let left = naming.deadline - Instant::now();
    assert!(left > Duration::from_secs(14), "15s: {left:?}");

    naming.deadline = Instant::now();
    assert_eq!(poll(&mut app), None);
    assert!(app.session.namings.is_empty());
    assert!(path.exists());
}

#[test]
fn leaving_the_project_with_a_job_pending_leaves_the_unnamed_file() {
    let (_dir, _todo, mut app) = composer_with_engine("naming-leave");
    let (path, placeholder) = added(&mut app);
    answer(&mut app, &placeholder, Ok("Too late".into()));
    app.leave_project();
    assert_eq!(poll(&mut app), None);
    assert!(path.exists());
}

#[test]
fn a_file_renamed_by_hand_keeps_its_name() {
    let (_dir, todo, mut app) = composer_with_engine("naming-renamed");
    let (path, placeholder) = added(&mut app);
    let mine = todo.join("mine.md");
    std::fs::rename(&path, &mine).unwrap();
    answer(&mut app, &placeholder, Ok("Late".into()));
    poll(&mut app);
    assert!(mine.exists());
    assert!(!todo.join("late.md").exists());
    assert_eq!(
        std::fs::read_to_string(&mine).unwrap(),
        format!("# {placeholder}\n\n{NOTES}\n")
    );
}

#[test]
fn a_heading_edited_meanwhile_is_never_overwritten() {
    let (_dir, todo, mut app) = composer_with_engine("naming-heading");
    let (path, placeholder) = added(&mut app);
    std::fs::write(&path, "# My own title\n").unwrap();
    answer(&mut app, &placeholder, Ok("Late".into()));
    poll(&mut app);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# My own title\n");
    assert!(!todo.join("late.md").exists());
}

#[test]
fn the_title_finds_the_issue_where_it_moved_and_keeps_its_labels() {
    let (dir, _todo, mut app) = composer_with_engine("naming-moved");
    let (path, placeholder) = added(&mut app);
    std::fs::write(
        &path,
        format!("# [bug] {placeholder}\n\nA {placeholder}.\n"),
    )
    .unwrap();
    let ready = dir.join("issues/ready");
    let moved = app.move_issue(&path, &ready).unwrap();
    assert_eq!(moved, ready.join(format!("001-{placeholder}.md")));

    answer(&mut app, &placeholder, Ok("Fix it".into()));
    poll(&mut app);
    assert_eq!(
        std::fs::read_to_string(ready.join("001-fix-it.md")).unwrap(),
        format!("# [bug] Fix it\n\nA {placeholder}.\n")
    );
    assert!(!moved.exists());
}

#[test]
fn a_taken_name_takes_a_suffix() {
    let (_dir, todo, mut app) = composer_with_engine("naming-taken");
    let (_, placeholder) = added(&mut app);
    std::fs::write(todo.join("fix-it.md"), "# Other\n").unwrap();
    answer(&mut app, &placeholder, Ok("Fix it".into()));
    poll(&mut app);
    assert_eq!(
        std::fs::read_to_string(todo.join("fix-it.md")).unwrap(),
        "# Other\n"
    );
    assert!(todo.join("fix-it-2.md").exists());
}

#[test]
fn the_images_follow_the_title() {
    let (_dir, todo, mut app) = composer_with_engine("naming-images");
    let (path, placeholder) = added(&mut app);
    let root = app.board.root.clone();
    let folder = assets::folder(&root, &placeholder);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("01.png"), "png").unwrap();
    let link = assets::link(&placeholder, "01.png");
    std::fs::write(&path, format!("# {placeholder}\n\n![]({link})\n")).unwrap();

    answer(&mut app, &placeholder, Ok("Fix it".into()));
    poll(&mut app);
    assert!(assets::folder(&root, "fix-it").join("01.png").is_file());
    assert_eq!(
        std::fs::read_to_string(todo.join("fix-it.md")).unwrap(),
        format!("# Fix it\n\n![]({})\n", assets::link("fix-it", "01.png"))
    );
}

#[test]
fn the_reader_follows_the_rename() {
    let (_dir, todo, mut app) = composer_with_engine("naming-reader");
    let (path, placeholder) = added(&mut app);
    app.open_issue(path);
    answer(&mut app, &placeholder, Ok("Fix it".into()));
    poll(&mut app);
    let renamed = todo.join("fix-it.md");
    assert!(matches!(&app.session.screen, Screen::Detail(open, _) if *open == renamed));
    assert_eq!(app.board.find(&renamed).unwrap().title, "Fix it");
}

#[test]
fn the_title_waits_for_the_editor_and_lands_once_it_closes() {
    let (_dir, todo, mut app) = composer_with_engine("naming-editor");
    app.session.composer.as_mut().unwrap().title.clear();
    app.submit_composer(true);
    let naming = app.session.namings.last_mut().unwrap();
    let placeholder = naming.placeholder.clone();
    let path = todo.join(format!("{placeholder}.md"));
    assert!(app.session.editing().is_some(), "⌘↩ opens the Editor");

    answer(&mut app, &placeholder, Ok("Fix it".into()));
    assert!(poll(&mut app).is_some(), "held");
    assert!(path.exists(), "nothing moves under the Editor");

    app.leave(Leave::Reader);
    assert_eq!(poll(&mut app), None);
    let renamed = todo.join("fix-it.md");
    assert!(renamed.exists() && !path.exists());
    assert!(matches!(&app.session.screen, Screen::Detail(open, _) if *open == renamed));
    app.start_editing(&renamed);
    let editor = app.session.editor.as_ref().unwrap();
    assert_eq!(
        editor.name, "fix-it.md",
        "a stale buffer does not come back"
    );
    assert!(editor.text.starts_with("# Fix it\n"));
}

#[test]
fn unsaved_edits_of_the_body_are_kept_and_the_title_lands_after() {
    let (_dir, todo, mut app) = composer_with_engine("naming-unsaved");
    let (path, placeholder) = added(&mut app);
    app.start_editing(&path);
    let edited = format!("# {placeholder}\n\nMine.\n");
    app.session.editor.as_mut().unwrap().replace(edited.clone());

    answer(&mut app, &placeholder, Ok("Fix it".into()));
    poll(&mut app);
    let editor = app.session.editor.as_ref().unwrap();
    assert_eq!(editor.text, edited, "the buffer is untouched");
    assert_eq!(editor.path, path);

    app.leave(Leave::Reader);
    poll(&mut app);
    assert_eq!(
        std::fs::read_to_string(todo.join("fix-it.md")).unwrap(),
        "# Fix it\n\nMine.\n"
    );
}

#[test]
fn a_title_typed_in_the_editor_wins() {
    let (_dir, todo, mut app) = composer_with_engine("naming-editor-title");
    let (path, placeholder) = added(&mut app);
    app.start_editing(&path);
    let editor = app.session.editor.as_mut().unwrap();
    editor.replace("# Mine\n".to_string());

    answer(&mut app, &placeholder, Ok("Fix it".into()));
    poll(&mut app);
    app.leave(Leave::Reader);
    poll(&mut app);
    assert!(app.session.namings.is_empty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Mine\n");
    assert!(!todo.join("fix-it.md").exists());
}

#[test]
fn a_pressed_pointer_holds_the_rename_back() {
    let (_dir, _todo, mut app) = composer_with_engine("naming-pointer");
    let (path, placeholder) = added(&mut app);
    answer(&mut app, &placeholder, Ok("Fix it".into()));
    let ctx = egui::Context::default();
    run(&ctx, vec![button(egui::pos2(1.0, 1.0), true)], |_| {});
    assert!(app.poll_namings(&ctx).is_some());
    assert!(path.exists(), "a drag keeps its card");
    run(&ctx, vec![button(egui::pos2(1.0, 1.0), false)], |_| {});
    assert_eq!(app.poll_namings(&ctx), None);
    assert!(!path.exists());
}

#[test]
fn the_card_says_it_is_being_named() {
    let (_dir, _todo, mut app) = composer_with_engine("naming-card");
    added(&mut app);
    let mut harness = kittest(app);
    let naming = harness.get_by_label("Naming…").rect();
    let placeholder = harness.state().1.session.namings[0].placeholder.clone();
    let file = harness.state().1.board.columns[0].issues[0].path.clone();
    let card = harness
        .ctx
        .read_response(egui::Id::unique(("issue_card", &file)))
        .unwrap()
        .rect;
    assert_eq!(naming.max.y + 11.0, card.max.y, "its last row");
    answer(
        &mut harness.state_mut().1,
        &placeholder,
        Ok("Fix it".into()),
    );
    harness.run_steps(2);
    assert!(harness.query_by_label("Naming…").is_none());
}
