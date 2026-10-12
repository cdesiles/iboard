//! Draft design's tests: the artifact, its agent, and where it is offered.

use super::super::enrich::enrichment_dir;
use super::*;

#[test]
fn enrichment_artifact_path_is_per_issue() {
    let path = Path::new("/p/issues/todo/01-demo.md");
    assert_eq!(
        enrichment_dir(path),
        Path::new("/p/issues/todo/assets/01-demo")
    );
}

#[test]
fn enrichment_drafts_an_artifact_for_an_issue() {
    let dir = scratch_dir("enrich");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\nMake the header taller.\n").unwrap();

    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    // A fake agent: prints a whole HTML page, ignores the prompt.
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "printf '<!doctype html><html><body><h1>Demo</h1></body></html>' # {prompt}"
            .to_string(),
        configured: true,
    };

    app.start_enrichment(&file);
    assert!(app.session.enrichment.is_some(), "an enrichment started");

    let (ctx, raw) = panel_ctx();
    let artifact = todo.join("assets/01-demo/ui.html");
    until(|| {
        app.poll_enrichment(&ctx);
        artifact.is_file()
    });

    let html = std::fs::read_to_string(&artifact).unwrap_or_else(|err| {
        let status = match app.session.enrichment.as_ref().map(|e| &e.status) {
            Some(EnrichStatus::Failed(why)) => format!("failed: {why}"),
            Some(EnrichStatus::Running(_)) => "still running".to_string(),
            Some(EnrichStatus::Rendering(_)) => "rendering".to_string(),
            Some(EnrichStatus::Done) => "done".to_string(),
            None => "gone".to_string(),
        };
        panic!("no artifact ({err}); the enrichment is {status}")
    });
    assert!(html.contains("<h1>Demo</h1>"), "mockup saved: {html}");

    // The window renders without a display, loader and all.
    let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
    output.textures_delta.clear();
}

#[test]
fn enrichment_without_an_agent_opens_the_settings_instead() {
    let dir = scratch_dir("enrich-noagent");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, "# Demo\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.start_enrichment(&file);
    assert!(app.session.enrichment.is_none(), "nothing started");
    assert!(app.settings.is_some(), "the settings dialog opened");
}

/// Draft design mocks up a change still to make, so Done issues offer none.
#[test]
fn done_issues_offer_no_draft_design() {
    use egui_kittest::kittest::Queryable;
    let dir = scratch_dir("enrich-done");
    let todo = dir.join("issues/todo");
    let done = dir.join("issues/done");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::create_dir_all(&done).unwrap();
    let open = todo.join("open.md");
    let shipped = done.join("shipped.md");
    std::fs::write(&open, "# Open\n").unwrap();
    std::fs::write(&shipped, "# Shipped\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.agent.configured = true;

    let offers_draft = |mut app: IboardApp, path: &Path| {
        app.open_issue(path.to_path_buf());
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
        let offered = harness.query_by_label("Draft design").is_some();
        (offered, harness.into_state().1)
    };
    let (offered, app) = offers_draft(app, &open);
    assert!(offered, "a Todo issue offers it");
    let (offered, mut app) = offers_draft(app, &shipped);
    assert!(!offered, "a Done issue does not");

    app.start_enrichment(&shipped);
    assert!(
        app.session.enrichment.is_none(),
        "nor starts one any other way"
    );
}
