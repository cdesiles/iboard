//! Opening another project, or a later `iboard` handing the open window its
//! board: the window shows it, after the editor's edits, and comes forward.

use std::sync::mpsc::{Sender, channel};

use super::*;

/// `alpha` (Todo: a.md) on screen and `beta` (Ready: b.md) beside it, with a
/// channel in place of the socket.
fn alpha_and_beta(tag: &str) -> (Scratch, PathBuf, PathBuf, IboardApp, Sender<Request>) {
    let scratch = scratch_dir(tag);
    let (alpha, beta) = (scratch.join("alpha"), scratch.join("beta"));
    std::fs::create_dir_all(alpha.join("issues/todo")).unwrap();
    std::fs::write(alpha.join("issues/todo/a.md"), "# Alpha\n").unwrap();
    std::fs::create_dir_all(beta.join("issues/ready")).unwrap();
    std::fs::write(beta.join("issues/ready/b.md"), "# Beta\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&alpha).unwrap());
    let (later, handovers) = channel();
    app.handovers = Some(handovers);
    (scratch, alpha, beta, app, later)
}

/// One frame of `app`, and the commands it gave its window.
fn commands(ctx: &egui::Context, app: &mut IboardApp) -> Vec<egui::ViewportCommand> {
    let input = egui::RawInput {
        screen_rect: Some(window()),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        app.frame_logic(ui.ctx());
        app.draw(ui);
    });
    output.textures_delta.clear();
    output
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map(|viewport| viewport.commands.clone())
        .unwrap_or_default()
}

fn shows(app: &IboardApp, project: &Path) -> bool {
    recents::canonical(&app.board.root) == recents::canonical(&project.join("issues"))
}

#[test]
fn the_window_shows_the_project_a_later_iboard_hands_it() {
    let (_scratch, alpha, beta, mut app, later) = alpha_and_beta("handover");
    let ctx = themed_ctx();
    let a = alpha.join("issues/todo/a.md");
    app.session.selected = Some(a.clone());
    let focus = egui::ViewportCommand::Focus;
    assert!(!commands(&ctx, &mut app).contains(&focus), "none asked yet");

    later.send(Request::Focus).unwrap();
    assert!(
        commands(&ctx, &mut app).contains(&focus),
        "it comes forward"
    );
    later.send(Request::Project(alpha.clone())).unwrap();
    assert!(commands(&ctx, &mut app).contains(&focus));
    assert_eq!(
        app.session.selected,
        Some(a),
        "the board it shows already stays as it was"
    );

    later.send(Request::Project(beta.clone())).unwrap();
    assert!(commands(&ctx, &mut app).contains(&focus));
    assert!(shows(&app, &beta));
    assert!(app.session.selected.is_none());
    assert_eq!(app.recents, vec![recents::canonical(&beta)]);
}

#[test]
fn from_the_editor_the_handover_waits_for_the_edits() {
    let (_scratch, alpha, beta, mut app, later) = alpha_and_beta("handover-editor");
    let ctx = themed_ctx();
    let a = alpha.join("issues/todo/a.md");
    app.prefs.auto = false;
    app.open_issue(a.clone());
    app.start_editing(&a);
    app.session.editor.as_mut().unwrap().text = "# Alpha\n\nmore\n".to_string();

    later.send(Request::Project(beta.clone())).unwrap();
    frame(&ctx, &mut app, Vec::new());
    assert!(shows(&app, &alpha), "the editor asks first");
    assert!(matches!(app.session.leave_prompt, Some(Leave::Open(_))));

    app.answer_leave(LeaveChoice::Save);
    assert!(shows(&app, &beta), "saved, then on to beta");
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "# Alpha\n\nmore\n");
}

#[test]
fn opening_another_project_rebuilds_the_board_and_remembers_it() {
    let alpha = scratch_dir("switch-alpha");
    let beta = scratch_dir("switch-beta");
    std::fs::create_dir_all(alpha.join("issues/todo")).unwrap();
    std::fs::write(alpha.join("issues/todo/a.md"), "# Alpha\n").unwrap();
    std::fs::create_dir_all(beta.join("issues/ready")).unwrap();
    std::fs::write(beta.join("issues/ready/b.md"), "# Beta\n").unwrap();

    let located = discovery::locate(&alpha).unwrap();
    let mut app = IboardApp::from_located(located);
    // What belongs to alpha must not follow the switch.
    let a = alpha.join("issues/todo/a.md");
    app.session.undo = Some(Undo::Deleted(vec![Deleted {
        path: a.clone(),
        trashed: None,
        content: Vec::new(),
        images: None,
    }]));
    app.session.notice = Some(("Archived a.md".to_string(), Instant::now()));
    app.session.pending_front = Some((alpha.join("issues/todo"), a.clone()));
    app.session.design_error = Some("alpha has no design".to_string());

    app.open_project(beta.to_path_buf());

    assert!(
        app.session.undo.is_none(),
        "⌘Z cannot restore into the old project"
    );
    assert!(app.session.notice.is_none() && app.session.pending_front.is_none());
    assert!(app.session.design_error.is_none() && app.session.design_system.is_none());
    assert!(app.error.is_none());
    assert_eq!(
        recents::canonical(&app.board.root),
        recents::canonical(&beta.join("issues")),
        "the board now belongs to the target project"
    );
    assert!(matches!(app.session.screen, Screen::Board));
    assert_eq!(app.recents, vec![recents::canonical(&beta)]);
    assert_eq!(
        app.pending_title.as_deref(),
        beta.file_name().and_then(|name| name.to_str()),
        "the window takes the new project's name"
    );

    // A folder with no issues directory reports an error and changes nothing.
    let empty = scratch_dir("switch-empty");
    let before = app.board.root.clone();
    app.open_project(empty.to_path_buf());
    assert!(app.error.is_some());
    assert_eq!(app.board.root, before);
}
