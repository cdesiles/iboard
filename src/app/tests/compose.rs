//! NewIssueModal's tests: opening it from Todo, typing, naming, drafting,
//! parking, adding, and the ticket just written.

use super::*;
use crate::app::compose::{ComposerField, TitleStage};
use crate::app::dock::IssueDrag;

/// Bug: a new ticket is renumbered to position 1 a moment after it shows,
/// so a quick click on it, or on its Add label action, held the old name:
/// "This issue no longer exists". Everything open on it follows the rename.
#[test]
fn a_ticket_clicked_while_it_is_renumbered_stays_open() {
    let dir = scratch_dir("compose-race");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::write(todo.join("001-old.md"), "# Old\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    app.write_composed("Fresh", false);
    let written = todo.join("fresh.md");
    assert_eq!(app.session.selected.as_deref(), Some(written.as_path()));
    assert!(
        app.session.pending_front.is_some(),
        "renumbered at the end of the frame"
    );

    app.open_issue(written.clone());
    app.toggle_label_picker(written.clone());
    draw(&themed_ctx(), &mut app, vec![]);

    let renamed = todo.join("001-fresh.md");
    assert!(renamed.is_file() && !written.exists());
    assert!(matches!(&app.session.screen, Screen::Detail(path, _) if *path == renamed));
    assert!(app.board.find(&renamed).is_some(), "the reader finds it");
    assert_eq!(
        app.session
            .label_picker
            .as_ref()
            .map(|picker| picker.path.as_path()),
        Some(renamed.as_path()),
        "the LabelPicker stays open on it"
    );
    assert_eq!(
        app.session.new_card.as_ref().map(|(path, _)| path),
        Some(&renamed)
    );
}

#[test]
fn the_modal_creates_an_issue_and_closes() {
    let dir = scratch_dir("compose");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.open_composer();
    assert!(app.composer_open());
    app.session.composer.as_mut().unwrap().title = "Fix login redirect".to_string();
    app.submit_composer(false);

    let created = todo.join("fix-login-redirect.md");
    assert_eq!(
        std::fs::read_to_string(&created).unwrap(),
        "# Fix login redirect\n"
    );
    assert!(app.session.composer.is_none(), "the modal closes");
    assert!(app.board.find(&created).is_some(), "the board sees it");
    assert_eq!(app.session.selected.as_deref(), Some(created.as_path()));

    // An empty title never creates a file.
    app.open_composer();
    app.submit_composer(false);
    assert_eq!(std::fs::read_dir(&todo).unwrap().count(), 1);
}

/// The modal's keys, as typed: `⇧↩` is a new line, `↩` adds, `⌘↩` adds and
/// opens the Editor.
#[test]
fn modal_keys() {
    let dir = scratch_dir("compose-keys");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();

    let ctx = themed_ctx();
    let key = |key: Key, modifiers: egui::Modifiers| {
        vec![
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            },
        ]
    };
    let text = |text: &str| vec![egui::Event::Text(text.into())];
    let files = || std::fs::read_dir(&todo).unwrap().count();

    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, text("First line"));
    frame(&ctx, &mut app, key(Key::Enter, egui::Modifiers::SHIFT));
    frame(&ctx, &mut app, text("second"));
    assert_eq!(
        app.session.composer.as_ref().unwrap().description,
        "First line\nsecond"
    );
    assert_eq!(files(), 0, "a new line never adds");

    frame(&ctx, &mut app, key(Key::Enter, egui::Modifiers::NONE));
    assert_eq!(
        std::fs::read_to_string(todo.join("first-line.md")).unwrap(),
        "# First line\n\nFirst line\nsecond\n"
    );
    assert!(app.session.composer.is_none(), "↩ closes it");

    app.open_composer();
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
    );
    frame(&ctx, &mut app, text("Edit me next"));
    frame(&ctx, &mut app, key(Key::Enter, egui::Modifiers::COMMAND));
    let created = todo.join("edit-me-next.md");
    frame(&ctx, &mut app, vec![]);
    assert!(created.exists(), "⌘↩ adds");
    assert_eq!(
        app.session.editing(),
        Some(created.as_path()),
        "and opens the Editor"
    );
}

/// Closing with notes parks them, even across a restart; Discard drops
/// them, and `⌘Z` brings them back.
#[test]
fn closing_parks_the_notes_and_discard_can_be_undone() {
    let dir = scratch_dir("compose-park");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.open_composer();
    app.session.composer.as_mut().unwrap().description = "Half a thought".into();
    app.composer_escape();
    assert!(!app.composer_open() && app.has_parked_note());
    let parked = dir.join(".iboard/new-issue.md");
    assert!(parked.is_file(), "parked on disk");

    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    assert!(
        app.has_parked_note(),
        "Todo's + shows the dot after a restart"
    );
    app.open_composer();
    assert_eq!(
        app.session.composer.as_ref().unwrap().description,
        "Half a thought"
    );

    app.discard_composer();
    assert!(app.session.composer.is_none() && !parked.exists());
    assert!(app.restore_discarded(), "⌘Z brings it back");
    assert!(app.composer_open());
    assert_eq!(
        app.session.composer.as_ref().unwrap().description,
        "Half a thought"
    );

    app.session.composer.as_mut().unwrap().description.clear();
    app.close_composer();
    assert!(app.session.composer.is_none(), "empty notes park nothing");
    assert!(!app.has_parked_note());
}

/// A modal whose Review agent answers with `answer`'s JSON lines.
fn drafting(tag: &str, answer: &str) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::write(
        dir.join("iboard.toml"),
        "[reader]\nsections = [\"Context\", \"Acceptance criteria\", \"Out of scope\"]\n",
    )
    .unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.prefs.agent = fake_agent(&format!("printf '%s\\n' {answer} # {{prompt}}"));
    app.open_composer();
    app.session.composer.as_mut().unwrap().description = "Export the board. One per column.".into();
    (dir, todo, app)
}

#[test]
fn review_drafts_the_issue_and_add_writes_the_draft() {
    let answer = r#"'{"title":"Export the board as checklists"}' '{"labels":["feat"]}' '{"summary":"One checklist per column."}' '{"section":"Context","text":"People print boards."}'"#;
    let (_dir, todo, mut app) = drafting("compose-draft", answer);
    app.start_draft();
    assert!(app.session.composer.as_ref().unwrap().drafting.is_some());
    assert!(until(|| {
        app.poll_draft();
        app.session.composer.as_ref().unwrap().drafting.is_none()
    }));
    let composer = app.session.composer.as_ref().unwrap();
    assert!(composer.draft_error.is_none(), "{:?}", composer.draft_error);
    assert_eq!(composer.title, "Export the board as checklists");
    assert!(composer.stage == TitleStage::Drafted);
    let drafted = composer.draft.as_ref().unwrap();
    assert!(drafted.receipt.is_some());
    assert_eq!(
        drafted.draft.unwritten(&app.board.reader_sections),
        ["Acceptance criteria", "Out of scope"]
    );

    app.submit_composer(false);
    assert_eq!(
        std::fs::read_to_string(todo.join("export-the-board-as-checklists.md")).unwrap(),
        "# [feat] Export the board as checklists\n\nOne checklist per column.\n\n\
         ## Context\n\nPeople print boards.\n\n## Notes\n\nExport the board. One per column.\n"
    );
}

#[test]
fn undo_draft_goes_back_to_the_notes() {
    let (_dir, todo, mut app) = drafting("compose-undo", r#"'{"title":"Drafted"}'"#);
    app.session.composer.as_mut().unwrap().title = "Mine".into();
    app.start_draft();
    assert!(until(|| {
        app.poll_draft();
        app.session.composer.as_ref().unwrap().drafting.is_none()
    }));
    assert_eq!(app.session.composer.as_ref().unwrap().title, "Drafted");
    app.undo_draft();
    let composer = app.session.composer.as_ref().unwrap();
    assert!(composer.draft.is_none());
    assert_eq!(composer.title, "Mine", "the title comes back");
    app.submit_composer(false);
    assert_eq!(
        std::fs::read_to_string(todo.join("mine.md")).unwrap(),
        "# Mine\n\nExport the board. One per column.\n"
    );
}

#[test]
fn a_failed_draft_keeps_the_notes() {
    let (_dir, _todo, mut app) = drafting("compose-draft-fail", "");
    app.prefs.agent.command = "exit 3 # {prompt}".into();
    app.start_draft();
    assert!(until(|| {
        app.poll_draft();
        app.session.composer.as_ref().unwrap().drafting.is_none()
    }));
    let composer = app.session.composer.as_ref().unwrap();
    assert!(composer.draft.is_none(), "back to the mirror");
    assert!(composer.draft_error.is_some());
    assert_eq!(composer.description, "Export the board. One per column.");
}

/// A composer whose engine answers through `sender`, as the real ones do.
fn composer_with_engine(tag: &str) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    // A stand-in that answers at once: no test ever runs a real agent.
    app.prefs.agent = fake_agent("true # {prompt}");
    app.open_composer();
    let composer = app.session.composer.as_mut().unwrap();
    composer.engine = Some(Engine::Agent);
    composer.description = "When I export the board I want a checklist.\nOne per column.".into();
    (dir, todo, app)
}

/// Stands in for the engine: the answer is already waiting.
fn answer_with(app: &mut IboardApp, answer: Result<String, String>) {
    let composer = app.session.composer.as_mut().unwrap();
    let run = composer.naming.as_mut().expect("naming started");
    run.answer = Job::done(answer);
}

#[test]
fn enter_names_the_issue_then_adds_it() {
    let (_dir, todo, mut app) = composer_with_engine("compose-name");

    app.submit_composer(false);
    assert!(app.session.composer.as_ref().unwrap().stage == TitleStage::Naming);
    assert_eq!(
        std::fs::read_dir(&todo).unwrap().count(),
        0,
        "nothing written yet"
    );

    answer_with(&mut app, Ok("Export the board as checklists".into()));
    app.poll_naming();
    let composer = app.session.composer.as_ref().unwrap();
    assert!(
        composer.stage == TitleStage::Inferred,
        "it stops on the suggestion"
    );
    assert_eq!(composer.title, "Export the board as checklists");
    assert!(composer.select_title, "selected, so typing replaces it");

    app.submit_composer(false);
    let created = todo.join("export-the-board-as-checklists.md");
    assert_eq!(
        std::fs::read_to_string(&created).unwrap(),
        "# Export the board as checklists\n\nWhen I export the board I want a checklist.\nOne per column.\n"
    );
    assert!(app.session.composer.is_none(), "and closes the modal");
}

#[test]
fn a_failed_name_falls_back_to_the_first_line() {
    let (_dir, todo, mut app) = composer_with_engine("compose-fail");

    app.submit_composer(false);
    answer_with(&mut app, Err("model unavailable".into()));
    app.poll_naming();
    let composer = app.session.composer.as_ref().unwrap();
    assert!(composer.stage == TitleStage::Failed);
    assert!(
        composer.focus == Some(ComposerField::Title),
        "cursor in the title"
    );
    assert!(!composer.description.is_empty(), "nothing lost");

    app.submit_composer(false);
    assert!(
        todo.join("when-i-export-the-board-i-want-a-checklist.md")
            .exists(),
        "↩ uses the first line"
    );
}

#[test]
fn command_enter_while_naming_writes_once_named() {
    let (_dir, todo, mut app) = composer_with_engine("compose-name-edit");

    app.submit_composer(false);
    app.submit_composer(true);
    answer_with(&mut app, Ok("Export checklists".into()));
    app.poll_naming();
    assert!(
        todo.join("export-checklists.md").exists(),
        "⌘↩ never stops on the suggestion"
    );
    assert!(app.session.composer.is_none(), "and closes the modal");
}

#[test]
fn esc_while_naming_lets_you_type_the_title() {
    let (_dir, todo, mut app) = composer_with_engine("compose-skip");
    app.submit_composer(false);
    app.composer_escape();
    let composer = app
        .session
        .composer
        .as_ref()
        .expect("esc does not close while naming");
    assert!(composer.stage == TitleStage::Skipped);
    assert!(composer.naming.is_none());

    app.session.composer.as_mut().unwrap().title = "Board export".into();
    app.submit_composer(false);
    assert!(todo.join("board-export.md").exists());
    app.open_composer();
    app.composer_escape();
    assert!(app.session.composer.is_none(), "esc closes it otherwise");
}

#[test]
fn without_an_engine_the_first_line_names_the_issue() {
    let dir = scratch_dir("compose-manual");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    assert!(
        app.session.composer.as_ref().unwrap().engine.is_none(),
        "Off in tests"
    );
    app.session.composer.as_mut().unwrap().description =
        "  Fix the login redirect\nIt loses the deep link.".into();
    app.submit_composer(false);
    assert_eq!(
        std::fs::read_to_string(todo.join("fix-the-login-redirect.md")).unwrap(),
        "# Fix the login redirect\n\n  Fix the login redirect\nIt loses the deep link.\n"
    );
}

/// Bug: right after `↩` added a ticket, a click on its Add label action
/// closed the now empty composer on the press, the cards slid up from under
/// the held pointer, and the card started a drag: the board flickered.
#[test]
fn a_click_on_the_new_ticket_right_after_adding_it_lands() {
    let dir = scratch_dir("compose-label");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::write(todo.join("001-old.md"), "# Old\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    let ctx = themed_ctx();
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![egui::Event::Text("Fresh".into())]);
    frame(&ctx, &mut app, vec![key(Key::Enter, egui::Modifiers::NONE)]);
    frame(&ctx, &mut app, vec![]);
    let fresh = todo.join("001-fresh.md");
    assert!(fresh.is_file(), "↩ adds it at the top");
    assert!(app.session.composer.is_none(), "and closes the modal");

    let add = ctx
        .read_response(egui::Id::new(("card_add_label", &fresh)))
        .expect("the selected new card shows its actions")
        .rect
        .center();
    frame(&ctx, &mut app, vec![moved(add)]);
    frame(&ctx, &mut app, vec![moved(add), button(add, true)]);
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    assert!(
        !egui::DragAndDrop::has_payload_of_type::<IssueDrag>(&ctx),
        "the held click never turns into a drag"
    );
    frame(&ctx, &mut app, vec![button(add, false)]);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(
        app.session
            .label_picker
            .as_ref()
            .map(|picker| picker.path.as_path()),
        Some(fresh.as_path()),
        "the click opens its LabelPicker"
    );
    assert_eq!(app.session.selected.as_deref(), Some(fresh.as_path()));
}

/// Todo's `+` is a column action, shown on hover.
#[test]
fn todo_offers_new_issue_on_hover() {
    let dir = scratch_dir("plus-head");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let index = app
        .board
        .columns
        .iter()
        .position(|c| c.accepts_new_issues())
        .unwrap();
    let plus = egui::Id::new(("new_issue", index));

    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    assert!(ctx.read_response(plus).is_none(), "hidden at rest");
    let column = ctx
        .read_response(egui::Id::new(("column_drop", index)))
        .unwrap()
        .rect;
    let head = egui::pos2(column.center().x, column.min.y + theme::CONTROL_H / 2.0);
    draw(&ctx, &mut app, vec![moved(head)]);
    let at = ctx.read_response(plus).expect("shown on hover").rect;
    let at = at.center();
    draw(&ctx, &mut app, vec![moved(at)]);
    draw(&ctx, &mut app, vec![button(at, true)]);
    draw(&ctx, &mut app, vec![button(at, false)]);
    assert!(app.session.composer.is_some(), "+ opens the composer");
}

#[test]
fn only_the_todo_column_offers_new_issues() {
    let dir = scratch_dir("new-columns");
    let todo = dir.join("issues/todo");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(&doing).unwrap();

    let located = discovery::locate(&dir).unwrap();
    let board = discovery::build(&located).unwrap();

    let find = |name: &str| {
        board
            .columns
            .iter()
            .find(|column| column.name == name)
            .unwrap()
    };
    assert!(find("Todo").accepts_new_issues());
    assert!(!find("Doing").accepts_new_issues());
}
