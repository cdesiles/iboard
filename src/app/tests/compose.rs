//! NewIssueModal's tests: opening it from Todo, typing, drafting,
//! parking, adding, and the ticket just written.

use super::*;
use crate::app::compose::{ComposerField, TitleStage};
use crate::app::dock::IssueDrag;
use crate::app::new_issue_ui::ComposeAction;

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
    let composer = app.session.composer.as_mut().unwrap();
    composer.title = "Fix login redirect".to_string();
    composer.description = "It loses the deep link.".to_string();
    app.submit_composer(false);

    let created = todo.join("fix-login-redirect.md");
    assert_eq!(
        std::fs::read_to_string(&created).unwrap(),
        "# Fix login redirect\n\nIt loses the deep link.\n"
    );
    assert!(app.session.composer.is_none(), "the modal closes");
    assert!(app.board.find(&created).is_some(), "the board sees it");
    assert_eq!(app.session.selected.as_deref(), Some(created.as_path()));

    // An empty description never creates a file, even with a title.
    app.open_composer();
    app.submit_composer(false);
    app.session.composer.as_mut().unwrap().title = "Only a title".to_string();
    app.submit_composer(false);
    assert_eq!(std::fs::read_dir(&todo).unwrap().count(), 1);
    assert!(app.composer_open(), "it stays open, on the notes");
    assert!(app.session.composer.as_ref().unwrap().focus == Some(ComposerField::Description));
}

/// The title's text field, focused and ready for typing.
fn title_focused(tag: &str) -> (Scratch, PathBuf, IboardApp, egui::Context) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    app.session.composer.as_mut().unwrap().focus = Some(ComposerField::Title);
    let ctx = themed_ctx();
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    (dir, todo, app, ctx)
}

/// QuickAdd's Labels: `[bug]` turns into a tag as the `]` is typed, the space
/// after it is not kept, `⌫` turns it back, and the file keeps the brackets
/// while its name does not.
#[test]
fn a_label_typed_in_the_title_becomes_a_tag() {
    let (_dir, todo, mut app, ctx) = title_focused("compose-tag");
    let text = |text: &str| vec![egui::Event::Text(text.into())];
    let composer = |app: &IboardApp| {
        let composer = app.session.composer.as_ref().unwrap();
        (composer.title_labels.clone(), composer.title.clone())
    };

    frame(&ctx, &mut app, text("[Bug"));
    assert_eq!(composer(&app), (vec![], "[Bug".to_string()));
    frame(&ctx, &mut app, text("]"));
    assert_eq!(composer(&app), (vec!["Bug".to_string()], String::new()));

    frame(&ctx, &mut app, text(" "));
    frame(&ctx, &mut app, text("Fix"));
    assert_eq!(composer(&app), (vec!["Bug".to_string()], "Fix".to_string()));

    frame(
        &ctx,
        &mut app,
        vec![key(Key::Backspace, egui::Modifiers::NONE)],
    );
    assert_eq!(composer(&app), (vec!["Bug".to_string()], "Fi".to_string()));
    for _ in 0..2 {
        frame(
            &ctx,
            &mut app,
            vec![key(Key::Backspace, egui::Modifiers::NONE)],
        );
    }
    assert_eq!(composer(&app), (vec!["Bug".to_string()], String::new()));
    frame(
        &ctx,
        &mut app,
        vec![key(Key::Backspace, egui::Modifiers::NONE)],
    );
    assert_eq!(
        composer(&app),
        (vec![], "[Bug]".to_string()),
        "back to text"
    );
    frame(&ctx, &mut app, vec![]);
    assert_eq!(
        composer(&app),
        (vec![], "[Bug]".to_string()),
        "and it stays"
    );
    frame(
        &ctx,
        &mut app,
        vec![key(Key::Backspace, egui::Modifiers::NONE)],
    );
    assert_eq!(
        composer(&app),
        (vec![], "[Bug".to_string()),
        "the cursor is after ]"
    );

    frame(&ctx, &mut app, text("]"));
    frame(&ctx, &mut app, text("Fix login redirect"));
    let composer = app.session.composer.as_mut().unwrap();
    composer.description = "Notes.".into();
    app.submit_composer(false);
    assert_eq!(
        std::fs::read_to_string(todo.join("fix-login-redirect.md")).unwrap(),
        "# [Bug] Fix login redirect\n\nNotes.\n"
    );
}

#[test]
fn a_tag_alone_is_no_title() {
    let (_dir, todo, mut app, _ctx) = title_focused("compose-tag-alone");
    let composer = app.session.composer.as_mut().unwrap();
    composer.title_labels = vec!["bug".to_string()];
    app.submit_composer(false);
    assert_eq!(std::fs::read_dir(&todo).unwrap().count(), 0, "no notes");

    let composer = app.session.composer.as_mut().unwrap();
    composer.description = "Login loses the link\nDetails.".into();
    app.submit_composer(false);
    assert_eq!(
        std::fs::read_to_string(todo.join("login-loses-the-link.md")).unwrap(),
        "# [bug] Login loses the link\n\nLogin loses the link\nDetails.\n"
    );
}

#[test]
fn parked_tags_come_back_as_tags() {
    let (dir, _todo, mut app, _ctx) = title_focused("compose-tag-park");
    let composer = app.session.composer.as_mut().unwrap();
    composer.title_labels = vec!["bug".to_string()];
    composer.title = "Fix".to_string();
    composer.description = "Notes.".to_string();
    app.close_composer();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    let composer = app.session.composer.as_ref().unwrap();
    assert_eq!(composer.title_labels, ["bug"]);
    assert_eq!(composer.title, "Fix");
}

/// The sheet's head says how an untitled issue will get its title.
#[test]
fn the_sheet_head_follows_the_engine() {
    use egui_kittest::kittest::Queryable;
    let dir = scratch_dir("compose-head");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    let mut harness = kittest(app);
    let note = harness
        .get_by_label("empty title uses the first line")
        .rect();
    assert!(
        harness
            .get_by_label("Add to Todo")
            .rect()
            .y_range()
            .contains(note.center().y),
        "Manual's note sits in the foot"
    );
    assert!(harness.query_by_label(" named in the background").is_none());

    let app = &mut harness.state_mut().1;
    app.session.composer.as_mut().unwrap().engine = Some(Engine::Apple);
    harness.run_steps(1);
    assert!(harness.query_by_label(" named in the background").is_some());
    assert!(
        harness
            .query_by_label("empty title uses the first line")
            .is_none()
    );
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
fn a_ghost_tile_adds_the_issue_and_opens_the_editor_on_its_section() {
    let answer = r#"'{"title":"Exports"}' '{"section":"Context","text":"People print boards."}'"#;
    let (_dir, todo, mut app) = drafting("compose-ghost", answer);
    app.start_draft();
    assert!(until(|| {
        app.poll_draft();
        app.session.composer.as_ref().unwrap().drafting.is_none()
    }));
    let at = app
        .board
        .reader_sections
        .iter()
        .position(|section| section == "Out of scope")
        .unwrap();
    app.compose_action(ComposeAction::SubmitForSection(at));

    assert!(app.session.composer.is_none(), "added");
    assert!(todo.join("exports.md").exists());
    let editor = app.session.editor.as_ref().unwrap();
    assert!(editor.path.ends_with("exports.md"));
    assert!(
        editor.text.ends_with("## Out of scope\n\n"),
        "{}",
        editor.text
    );
    assert!(app.session.focus_editor, "the caret under the heading");
}

#[test]
fn keep_my_notes_is_remembered() {
    let answer = r#"'{"title":"Exports"}'"#;
    let (dir, _todo, mut app) = drafting("compose-keep", answer);
    let file = dir.join("user.toml");
    app.config_file = Some(file.clone());
    app.start_draft();
    assert!(until(|| {
        app.poll_draft();
        app.session.composer.as_ref().unwrap().drafting.is_none()
    }));
    let mut harness = kittest(app);
    press(&mut harness, "Keep my notes under ## Notes");
    assert!(!harness.state().1.prefs.keep_notes);
    let mut config = UserConfig::load(Some(&file));
    assert!(
        !Preferences::load(&mut config).keep_notes,
        "for the next run"
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

#[test]
fn a_new_cards_highlight_runs_out_over_two_seconds() {
    let dir = scratch_dir("fresh-fade");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    let file = dir.join("issues/todo/fresh.md");
    let at = |secs| Some((file.clone(), Instant::now() - Duration::from_secs(secs)));
    app.session.new_card = at(1);
    let (path, left) = app.flashing_card().unwrap();
    assert_eq!(path, file);
    assert!((0.4..=0.5).contains(&left), "{left}");
    app.session.new_card = at(2);
    assert!(app.flashing_card().is_none());
}

/// The notes are prose: their placeholder reads at `read-body`, as they do.
#[test]
fn the_notes_placeholder_is_in_read_body() {
    let dir = scratch_dir("compose-notes-hint");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_composer();
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let (_, shapes) = painted(&ctx, &mut app, vec![]);
    let font = shapes.iter().find_map(|shape| match shape {
        egui::Shape::Text(text) if text.galley.job.text.starts_with("Describe the issue") => {
            Some(text.galley.job.sections[0].format.font_id.clone())
        }
        _ => None,
    });
    assert_eq!(font, Some(theme::read_body()));
}
