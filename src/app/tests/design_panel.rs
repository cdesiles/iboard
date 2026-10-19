//! The Design panel's tests: loading, creating and drafting components.

use super::*;

/// A tiny `design/` so the panel tests need no display and no repo data.
fn write_min_design(root: &Path) {
    let design = root.join("design");
    std::fs::create_dir_all(design.join("components/Button")).unwrap();
    std::fs::write(
        design.join("design-system.json"),
        r#"{ "v": 1, "title": "Fixture", "namespace": "fixture" }"#,
    )
    .unwrap();
    std::fs::write(
            design.join("tokens.json"),
            r##"{
                "name": "Fixture", "version": 1,
                "color": {
                    "themes": [ {"id":"light","name":"Light"}, {"id":"dark","name":"Dark"} ],
                    "tokens": [ {"name":"bg","value":{"light":"#ffffff","dark":"#000000"},"usage":"ground"} ]
                },
                "type": { "families": {"sans":"-apple-system"}, "groups": [] },
                "spacing": {"tokens":[]}, "radius": {"tokens":[]},
                "size": {"tokens":[]}, "shadow": {"tokens":[]}
            }"##,
        )
        .unwrap();
    std::fs::write(
        design.join("components/Button/README.md"),
        "# Button\n\nA button.\n",
    )
    .unwrap();
    std::fs::write(
        design.join("components/Button/preview.html"),
        "<!doctype html><html><head></head><body><button>Go</button></body></html>",
    )
    .unwrap();
}

#[test]
fn design_panel_loads_and_creates_a_component() {
    let dir = scratch_dir("design-panel");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    write_min_design(&dir);
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.open_design();
    assert!(
        app.session.design_system.is_some(),
        "loaded: {:?}",
        app.session.design_error
    );
    assert!(app.session.design.is_some());

    let (ctx, raw) = panel_ctx();
    let mut output = ctx.run_ui(raw.clone(), |ui| app.draw(ui));
    output.textures_delta.clear();

    // The same path the Create button drives.
    let mut panel = app.session.design.take().expect("panel open");
    let keep = app.handle_design_action(
        &mut panel,
        design_ui::Action::Create {
            name: "Toast".to_string(),
            group: "Controls".to_string(),
            intent: "Transient confirmation.".to_string(),
            draft: false,
        },
    );
    assert!(keep);
    assert!(dir.join("design/components/Toast/README.md").is_file());
    assert!(
        app.session
            .design_system
            .as_ref()
            .unwrap()
            .components
            .iter()
            .any(|c| c.name == "Toast")
    );

    app.session.design = Some(panel);
    let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
    output.textures_delta.clear();
}

#[test]
fn design_panel_reports_a_missing_system() {
    let dir = scratch_dir("design-missing");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

    app.open_design();
    assert!(app.session.design_system.is_none());
    assert!(
        app.session.design.is_some(),
        "the panel opens even with no system"
    );

    let (ctx, raw) = panel_ctx();
    let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
    output.textures_delta.clear();
}

#[test]
fn preview_render_helper_composes_and_renders() {
    if !iboard::design::render::available() {
        return;
    }
    let design = Path::new(env!("CARGO_MANIFEST_DIR")).join("design");
    let system = DesignSystem::load(&design).expect("repo design");
    let render = design::render::component_preview(&system, "Button").expect("spawn render");
    match render.wait() {
        Some(Ok(path)) => assert!(path.is_file(), "PNG written"),
        Some(Err(err)) => panic!("render failed: {err}"),
        None => panic!("the render thread died"),
    }
}

#[test]
fn design_panel_drafts_a_component_with_the_agent() {
    let dir = scratch_dir("design-draft");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    write_min_design(&dir);
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_design();

    // A fake agent: prints a fenced HTML fragment, ignores the prompt.
    app.agent = AgentConfig {
        tool: agent::Tool::Custom,
        model: String::new(),
        command: "printf '```html\\n<button class=\"ib-btn\">Hi</button>\\n```' # {prompt}"
            .to_string(),
        configured: true,
        ..AgentConfig::default()
    };

    let mut panel = app.session.design.take().expect("panel open");
    let keep = app.handle_design_action(
        &mut panel,
        design_ui::Action::Create {
            name: "Toast".to_string(),
            group: "Controls".to_string(),
            intent: "Transient confirmation.".to_string(),
            draft: true,
        },
    );
    assert!(keep);
    app.session.design = Some(panel);
    assert!(app.session.design_draft.is_some(), "a draft was started");

    let (ctx, _raw) = panel_ctx();
    let preview = dir.join("design/components/Toast/preview.html");
    until(|| {
        app.poll_design_draft(&ctx);
        std::fs::read_to_string(&preview)
            .unwrap_or_default()
            .contains("ib-btn")
    });

    let written = std::fs::read_to_string(&preview).unwrap();
    assert!(
        written.contains("ib-btn"),
        "drafted fragment written: {written}"
    );
    assert!(!written.contains("```"), "code fences stripped: {written}");
    assert!(app.session.design_draft.is_none(), "the draft finished");
}

/// Without the catalog's window helper, or without a design system to show,
/// the Design button falls back to the panel, which says what is missing.
#[test]
fn the_design_button_falls_back_to_the_panel() {
    let dir = scratch_dir("design-button");
    std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.show_design(true, false);
    assert!(app.session.design.is_some(), "no system: the panel says so");
    assert!(app.session.design_error.is_some());

    write_min_design(&dir);
    app.session.design = None;
    app.show_design(false, false);
    assert!(app.session.design.is_some(), "no helper: the panel");
    assert!(app.session.design_system.is_some());
}
