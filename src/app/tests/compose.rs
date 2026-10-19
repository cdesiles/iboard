//! The composer's tests: typing, naming, adding, and the ticket just written.

use super::*;

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
    app.write_composed("Fresh", "", false);
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
    assert!(matches!(&app.session.screen, Screen::Detail(path) if *path == renamed));
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
fn the_composer_creates_issues_and_stays_open() {
    let dir = scratch_dir("compose");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    app.open_composer();
    app.session.composer.as_mut().unwrap().title = "Fix login redirect".to_string();
    app.submit_composer(false);

    let created = todo.join("fix-login-redirect.md");
    assert_eq!(
        std::fs::read_to_string(&created).unwrap(),
        "# Fix login redirect\n"
    );
    let composer = app
        .session
        .composer
        .as_ref()
        .expect("the composer stays open");
    assert!(
        composer.title.is_empty(),
        "the field clears for the next idea"
    );
    assert!(matches!(app.session.screen, Screen::Board));
    assert!(app.board.find(&created).is_some(), "the board sees it");

    // An empty title never creates a file.
    app.submit_composer(false);
    assert_eq!(std::fs::read_dir(&todo).unwrap().count(), 1);
}

#[test]
fn clicking_away_adds_the_issue_and_closes_the_composer() {
    let dir = scratch_dir("compose-away");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();

    let located = discovery::locate(&dir).unwrap();
    let mut app = IboardApp::from_located(located);

    app.open_composer();
    app.session.composer.as_mut().unwrap().title = "Export board".to_string();
    app.submit_composer(true);

    let created = todo.join("export-board.md");
    assert!(created.exists(), "the new issue is written to Todo");
    app.flush_closing_composer(&egui::Context::default());
    assert!(app.session.composer.is_none(), "the composer closes");
    assert!(
        matches!(app.session.screen, Screen::Board),
        "the board stays"
    );
    assert_eq!(app.session.selected.as_deref(), Some(created.as_path()));
}

/// The composer's keys, as typed: `⇧↩` and `⌘↩` are new lines, `↩` and
/// `⌘S` add, and a click on another card adds and closes it.
#[test]
fn composer_keys_and_clicking_away() {
    let dir = scratch_dir("compose-keys");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let other = todo.join("other.md");
    std::fs::write(&other, "# Other\n").unwrap();
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
    frame(&ctx, &mut app, key(Key::Enter, egui::Modifiers::COMMAND));
    frame(&ctx, &mut app, text("third"));
    assert_eq!(
        app.session.composer.as_ref().unwrap().description,
        "First line\nsecond\nthird"
    );
    assert_eq!(files(), 1, "new lines never add");

    frame(&ctx, &mut app, key(Key::Enter, egui::Modifiers::NONE));
    assert_eq!(
        std::fs::read_to_string(todo.join("first-line.md")).unwrap(),
        "# First line\n\nFirst line\nsecond\nthird\n"
    );
    assert!(
        app.session.composer.is_some(),
        "↩ keeps it open for the next one"
    );

    frame(
        &ctx,
        &mut app,
        vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
    );
    frame(&ctx, &mut app, text("Saved with command S"));
    frame(&ctx, &mut app, key(Key::S, egui::Modifiers::COMMAND));
    // Adding a ticket at the top renumbers Todo's files.
    assert!(todo.join("001-saved-with-command-s.md").exists(), "⌘S adds");
    assert!(app.session.composer.is_some());
    let other = todo.join("003-other.md");
    assert!(other.exists(), "the older card was renumbered too");

    frame(
        &ctx,
        &mut app,
        vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
    );
    frame(&ctx, &mut app, text("Clicked away"));
    let card = ctx
        .read_response(egui::Id::new(("issue_card", &other)))
        .unwrap()
        .rect
        .center();
    let button = |pressed| egui::Event::PointerButton {
        pos: card,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(card)]);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(card), button(true)],
    );
    frame(&ctx, &mut app, vec![button(false)]);
    assert!(
        todo.join("001-clicked-away.md").exists(),
        "the click adds it"
    );
    assert!(app.session.composer.is_none(), "and closes the composer");
    assert!(matches!(app.session.screen, Screen::Board));
    assert_eq!(
        app.session.selected,
        Some(todo.join("004-other.md")),
        "the clicked card is still selected"
    );
}

/// A composer whose engine answers through `sender`, as the real ones do.
fn composer_with_engine(tag: &str) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    // A stand-in that answers at once: no test ever runs a real agent.
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        command: "true # {prompt}".to_string(),
        configured: true,
        ..AgentConfig::default()
    };
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
    let ctx = egui::Context::default();

    app.submit_composer(false);
    assert!(app.session.composer.as_ref().unwrap().stage == TitleStage::Naming);
    assert_eq!(
        std::fs::read_dir(&todo).unwrap().count(),
        0,
        "nothing written yet"
    );

    answer_with(&mut app, Ok("Export the board as checklists".into()));
    app.poll_naming(&ctx);
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
    let composer = app
        .session
        .composer
        .as_ref()
        .expect("the composer stays open");
    assert!(composer.title.is_empty() && composer.description.is_empty());
    assert!(composer.stage == TitleStage::Idle);
}

#[test]
fn a_failed_name_falls_back_to_the_first_line() {
    let (_dir, todo, mut app) = composer_with_engine("compose-fail");
    let ctx = egui::Context::default();

    app.submit_composer(false);
    answer_with(&mut app, Err("model unavailable".into()));
    app.poll_naming(&ctx);
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
fn clicking_away_while_naming_writes_once_named() {
    let (_dir, todo, mut app) = composer_with_engine("compose-name-away");
    let ctx = egui::Context::default();

    app.submit_composer(false);
    app.submit_composer(true);
    answer_with(&mut app, Ok("Export checklists".into()));
    app.poll_naming(&ctx);
    let created = todo.join("export-checklists.md");
    assert!(
        created.exists(),
        "a click away never stops on the suggestion"
    );
    app.flush_closing_composer(&ctx);
    assert!(app.session.composer.is_none(), "and closes the composer");
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

/// Bug: opening another issue dropped the composer and what was typed.
#[test]
fn opening_an_issue_keeps_an_unsaved_composer() {
    let dir = scratch_dir("compose-keep");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let other = todo.join("other.md");
    std::fs::write(&other, "# Other\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.open_composer();
    app.session.composer.as_mut().unwrap().description = "Half a thought".into();
    app.open_issue(other.clone());
    app.back_to_board();
    assert_eq!(
        app.session
            .composer
            .as_ref()
            .map(|c| c.description.as_str()),
        Some("Half a thought"),
        "the draft waits on the board"
    );

    app.session.composer.as_mut().unwrap().description.clear();
    app.open_issue(other);
    assert!(
        app.session.composer.is_none(),
        "an empty composer still closes"
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
    assert!(
        app.session.composer.is_some(),
        "and keeps the composer open"
    );

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
    assert!(app.session.composer.is_none(), "and closes the composer");
    assert_eq!(app.session.selected.as_deref(), Some(fresh.as_path()));
}
