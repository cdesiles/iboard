use super::pages::{parse_color, parse_shadow, swatch_color};
use super::*;
use crate::general::Appearance;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn repo() -> DesignSystem {
    DesignSystem::discover(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("repo design")
}

fn panel_ctx() -> (egui::Context, egui::RawInput) {
    let ctx = egui::Context::default();
    crate::theme::install(&ctx);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(980.0, 640.0),
        )),
        ..Default::default()
    };
    (ctx, input)
}

/// Drive one frame of the panel and return what it asked for.
fn frame(panel: &mut Panel, system: &DesignSystem, render_available: bool) -> Action {
    let (ctx, raw) = panel_ctx();
    let mut action = Action::Keep;
    // A README's table asks for a second pass, which must not lose the first's action.
    let mut output = ctx.run_ui(raw, |ui| {
        let answer = panel.body(ui, Some(system), None, render_available, false);
        if matches!(action, Action::Keep) {
            action = answer;
        }
    });
    output.textures_delta.clear();
    action
}

type PanelKit = Harness<'static, (bool, Panel, DesignSystem, Option<&'static str>)>;

/// The panel in a kittest harness, keeping the theme of the catalog it asks for.
fn kit(panel: Panel, appearance: Appearance) -> PanelKit {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(980.0, 640.0))
        .build_ui_state(
            move |ui, (themed, panel, system, catalog): &mut (bool, _, DesignSystem, _)| {
                if !*themed {
                    crate::theme::install(ui.ctx());
                    crate::theme::apply(ui.ctx(), appearance);
                    *themed = true;
                    return;
                }
                let action: Action = Panel::body(panel, ui, Some(system), None, false, false);
                if let Action::OpenCatalog { theme } = action {
                    *catalog = Some(theme);
                }
            },
            (false, panel, repo(), None),
        );
    harness.run_steps(2);
    harness
}

/// Clicks a widget the way a pointer does: over it for a frame first.
fn press(harness: &mut PanelKit, label: &str) {
    harness.get_by_label(label).hover();
    harness.run_steps(1);
    harness.get_by_label(label).click();
    harness.run_steps(2);
}

#[test]
fn a_rail_row_shows_its_whole_focus_ring() {
    let mut harness = kit(Panel::default(), Appearance::Light);
    let row = harness.get_by_label("Colors").rect();
    let mut reached = false;
    for _ in 0..20 {
        harness.key_press(egui::Key::Tab);
        harness.run_steps(2);
        let focused = harness.ctx.memory(|m| m.focused());
        reached = focused
            .and_then(|id| harness.ctx.read_response(id))
            .is_some_and(|response| response.rect == row);
        if reached {
            break;
        }
    }
    assert!(reached, "tab reaches the row");
    let ring = row.expand(2.0);
    assert!(harness.output().shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::Shape::Rect(shape)
            if shape.stroke.width == 2.0 && shape.rect == ring)
            && clipped.clip_rect.contains_rect(ring.expand(2.0))
    }));

    harness.key_press(egui::Key::Space);
    harness.run_steps(2);
    assert_eq!(harness.state().1.category, Category::Colors);
}

#[test]
fn search_lists_hits_with_their_category_at_either_level_and_clears() {
    let panel = Panel {
        category: Category::Components,
        components_open: true,
        selected: Some("Button".to_string()),
        search: "space-".to_string(),
        ..Panel::default()
    };
    let mut harness = kit(panel, Appearance::Light);
    let hits = search(&harness.state().2, "space-");
    assert!(hits.len() > 1);
    let categories = harness
        .output()
        .shapes
        .iter()
        .filter(|clipped| {
            matches!(&clipped.shape, egui::Shape::Text(text) if text.galley.text() == "Spacing")
        })
        .count();
    assert!(categories >= hits.len(), "{categories} category tags");

    let first = hits[0].label.clone();
    press(&mut harness, &first);

    let panel = &harness.state().1;
    assert_eq!(panel.category, Category::Spacing);
    assert!(!panel.components_open, "the rail is back on the categories");
    assert!(panel.search.is_empty());
}

#[test]
fn the_status_sits_under_the_toolbar_in_every_category() {
    let panel = Panel {
        status: Some("Opened the catalog".to_string()),
        ..Panel::default()
    };
    let harness = kit(panel, Appearance::Light);
    let status = harness.get_by_label("Opened the catalog").rect();
    assert!(status.min.y >= harness.get_by_label("Close").rect().max.y);
    assert!(status.max.y <= harness.get_by_label("Overview").rect().min.y);
}

#[test]
fn the_live_catalog_opens_in_the_theme_the_panel_draws() {
    for (appearance, theme) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let mut harness = kit(Panel::default(), appearance);
        press(&mut harness, "Live catalog");
        assert_eq!(harness.state().3, Some(theme));
    }
}

#[test]
fn a_preview_card_is_as_tall_as_it_declares() {
    let card = |height| CardMeta {
        height,
        ..CardMeta::default()
    };
    assert_eq!(card_height(&card(Some(56))), 56.0);
    assert_eq!(card_height(&card(Some(1200))), 1200.0);
    assert_eq!(card_height(&card(None)), UNDECLARED_CARD_H);
}

#[test]
fn the_selected_component_requests_one_lazy_render() {
    let system = repo();
    let mut panel = Panel {
        category: Category::Components,
        components_open: true,
        selected: Some("Button".to_string()),
        ..Panel::default()
    };
    match frame(&mut panel, &system, true) {
        Action::Render(name) => assert_eq!(name, "Button"),
        _ => panic!("the selected component should load its preview lazily"),
    }
    // Only one card is asked for per frame, and it is marked pending.
    assert_eq!(panel.previews.len(), 1);
    assert!(panel.running.is_none(), "the host has not started it yet");
}

#[test]
fn no_render_without_a_renderer() {
    let system = repo();
    let mut panel = Panel {
        category: Category::Components,
        components_open: true,
        selected: Some("Button".to_string()),
        ..Panel::default()
    };
    assert!(matches!(frame(&mut panel, &system, false), Action::Keep));
    assert!(panel.previews.is_empty());
}

#[test]
fn a_second_frame_waits_while_a_render_runs() {
    let system = repo();
    let mut panel = Panel {
        category: Category::Components,
        components_open: true,
        selected: Some("Button".to_string()),
        ..Panel::default()
    };
    let _ = frame(&mut panel, &system, true);
    // Pretend the host started the render and it is still running.
    let running = Job::spawn(|cancel| {
        while !cancel.is_cancelled() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Err("stopped".to_string())
    });
    panel.start_render("Button".to_string(), running);
    assert!(matches!(frame(&mut panel, &system, true), Action::Keep));
}

#[test]
fn a_redrafted_preview_is_rendered_again() {
    let mut panel = Panel::default();
    panel.fail_render("Toast", "old fragment".to_string());
    panel.start_render("Toast".to_string(), Job::done(Err("old".to_string())));
    panel.fail_render("Button", "unrelated".to_string());

    panel.forget_preview("Toast");

    assert!(!panel.previews.contains_key("Toast"));
    assert!(
        panel.running.is_none(),
        "the render of the old fragment is dropped"
    );
    assert!(panel.previews.contains_key("Button"));
}

#[test]
fn opening_components_picks_one_and_back_leaves_the_pane() {
    let system = repo();
    let mut panel = Panel::default();
    panel.open_components(&system, None);
    assert!(panel.components_open);
    assert_eq!(panel.category, Category::Components);
    assert!(panel.selected.is_some(), "the first component is picked");

    panel.open_components(&system, Some("Button".to_string()));
    assert_eq!(panel.selected.as_deref(), Some("Button"));

    // Back only closes the rail; the pane keeps the chosen component.
    panel.components_open = false;
    assert_eq!(panel.selected.as_deref(), Some("Button"));
}

#[test]
fn components_are_listed_alphabetically() {
    let system = repo();
    let names = |query: &str| {
        system
            .components_by_name(query)
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
    };
    let all = names("");
    let mut sorted = all.clone();
    sorted.sort_by_key(|name| name.to_lowercase());
    assert_eq!(all, sorted, "the rail lists components alphabetically");
    assert!(all.contains(&"Button"));
    assert_eq!(names("button"), vec!["Button", "ReviewButton"]);
}

#[test]
fn the_opacity_and_motion_pages_list_their_tokens() {
    for (category, token) in [
        (Category::Opacity, "label-tint-dark"),
        (Category::Motion, "scroll-linger"),
    ] {
        let panel = Panel {
            category,
            ..Panel::default()
        };
        let harness = kit(panel, Appearance::Light);
        assert!(harness.query_by_label(token).is_some(), "{token}");
    }
}

#[test]
fn every_category_renders_without_panicking() {
    let system = repo();
    for category in Category::ALL {
        let mut panel = Panel {
            category,
            ..Panel::default()
        };
        frame(&mut panel, &system, false);
    }
}

#[test]
fn a_component_readme_drops_its_title_heading() {
    assert_eq!(without_heading("# Button\n\nA verb.\n"), "A verb.");
    assert_eq!(without_heading("No heading here"), "No heading here");
    assert_eq!(without_heading("# Only a title"), "");
}

#[test]
fn colors_and_shadows_parse() {
    assert_eq!(
        parse_color("#ffffff"),
        Some(Color32::from_rgb(255, 255, 255))
    );
    assert_eq!(parse_color("#000"), Some(Color32::from_rgb(0, 0, 0)));
    assert_eq!(
        parse_color("rgba(21,24,29,0.12)"),
        Some(Color32::from_rgba_unmultiplied(21, 24, 29, 31))
    );
    assert_eq!(parse_color("oklch(0.6 0.2 250)"), None);
    assert_eq!(parse_color("#€€"), None, "six bytes, not six digits");
    assert_eq!(parse_color("#é"), None);

    let (offset, blur, _) = parse_shadow("0 4px 16px rgba(21,24,29,0.12)");
    assert_eq!(offset, [0, 4]);
    assert_eq!(blur, 16);
}

#[test]
fn search_finds_tokens_and_components() {
    let system = repo();
    let hits = search(&system, "butt");
    assert!(
        hits.iter()
            .any(|hit| hit.category == Category::Components && hit.label == "Button")
    );
    let hits = search(&system, "space-");
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|hit| hit.category == Category::Spacing));
}

#[test]
fn a_swatch_resolves_its_references_in_its_own_theme() {
    let system = repo();
    let focus = system.token("focus").expect("focus is {accent}");
    let accent = |theme| {
        let accent = system.token("accent").unwrap();
        parse_color(&design::css::token_value_for(accent, theme).unwrap())
    };
    assert_eq!(swatch_color(&system, focus, "light"), accent("light"));
    assert_eq!(swatch_color(&system, focus, "dark"), accent("dark"));
    assert_ne!(accent("light"), accent("dark"));
}
