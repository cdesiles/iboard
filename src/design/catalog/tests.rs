use super::*;

#[test]
fn a_component_group_is_escaped() {
    let mut component = repo().components.remove(0);
    component.card.group = Some("<img src=x onerror=alert(1)>".to_string());
    let html = section(&component, "c-x");
    assert!(!html.contains("<img"), "{html}");
    assert!(html.contains("&lt;img"));
}

fn repo() -> DesignSystem {
    DesignSystem::discover(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("repo design")
}

#[test]
fn the_catalog_opens_in_the_theme_asked_for() {
    let dark = html(&repo(), "dark").expect("compose");
    assert!(dark.contains("<html lang=\"en\" data-theme=\"dark\">"));
    assert!(
        dark.contains("id=\"ds-theme\" checked"),
        "the switch agrees"
    );
    let light = html(&repo(), "light").expect("compose");
    assert!(light.contains("<html lang=\"en\">"));
    assert!(!light.contains("id=\"ds-theme\" checked"));
}

#[test]
fn the_catalog_carries_every_component_and_the_tokens() {
    let html = html(&repo(), "light").expect("compose");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("--bg: #f4f5f7;"), "token block inlined");
    assert!(html.contains(".ib-btn"), "bundle inlined");
    assert!(html.contains("[data-theme=\"dark\"]"), "dark theme block");
    assert!(html.contains("id=\"ds-q\""), "search box");
    assert!(
        html.contains("--hint-hold: 500ms;"),
        "motion tokens inlined"
    );
    for section in ["cat-opacity", "cat-motion"] {
        assert!(html.contains(section), "missing {section}");
    }
    for component in repo().components {
        assert!(
            html.contains(&format!(">{}</h2>", component.name)),
            "missing section for {}",
            component.name
        );
    }
    assert!(!html.contains("@dsCard"), "card comment must not leak");
    assert!(!html.contains("@import"), "no external imports");
}

#[test]
fn the_catalog_uses_the_declared_group_and_subtitle() {
    let html = html(&repo(), "light").expect("compose");
    // Button's fragment says group="Controls"; BoardScreen is a page.
    assert!(html.contains("data-group=\"controls\""));
    assert!(
        html.contains("ds-stage--page"),
        "page components get a stage"
    );
}

#[test]
fn the_component_rail_is_alphabetical_and_drills_in() {
    let system = repo();
    let names: Vec<String> = system
        .components_by_name("")
        .iter()
        .map(|component| component.name.clone())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_by_key(|name| name.to_lowercase());
    assert_eq!(names, sorted, "the rail lists components alphabetically");

    let html = html(&system, "light").expect("compose");
    assert!(html.contains("data-cat=\"components\""), "category item");
    assert!(html.contains("data-drill=\"1\""), "components drills in");
    assert_eq!(
        html.matches("<svg class=\"ds-ico\"").count(),
        CATEGORIES.len() + 1,
        "a pictogram per category and one in the search box"
    );
    assert!(html.contains("class=\"ds-back\""), "a back button");
    assert!(
        html.contains("href=\"#c-button\""),
        "the second level links a component"
    );
}

#[test]
fn typography_embeds_the_shipped_fonts_as_a_specimen() {
    let html = html(&repo(), "light").expect("compose");
    assert!(html.contains("@font-face{font-family:\"Inter\";font-weight:600"));
    assert!(
        html.contains("class=\"ds-ttable\""),
        "a Name | Preview table"
    );
    assert!(html.contains(">Inter · 15</span>"), "a family · size chip");
    assert!(
        html.contains(">embedded</span>"),
        "the sans stack is shipped"
    );
}

#[test]
fn the_categories_flow_and_components_and_files_open_a_page() {
    let html = html(&repo(), "light").expect("compose");
    for (id, ..) in CATEGORIES {
        assert!(
            html.contains(&format!("id=\"cat-{id}\"")),
            "{id} in the flow"
        );
    }
    assert!(html.contains("data-page=\"components\" hidden"));
    assert!(html.contains("data-page=\"files\" hidden"));
    assert!(
        html.contains("data-open=\"components\""),
        "a components CTA"
    );
    assert!(html.contains("data-open=\"files\""), "a files CTA");
    assert!(
        html.contains("[hidden]{display:none!important}"),
        "a filtered row hides even when its class sets display"
    );
    assert!(html.contains("id=\"ds-empty\" hidden"), "a no-results line");
}

#[test]
fn a_fragment_is_reduced_to_its_body() {
    let text = "<!-- @dsCard group=\"X\" height=56 -->\n<!doctype html>\n<body>\n<div>hi</div>\n</body>\n</html>\n";
    let stripped = strip_card_comment(text);
    assert!(!stripped.contains("@dsCard"));
    let body = body_inner(&stripped);
    assert_eq!(body, "<div>hi</div>");
}

#[test]
fn a_fragment_keeps_its_head_styles_in_a_shadow_root() {
    let text = "<!doctype html><head><style>.x{color:red}</style></head>\n<body><div class=\"x\">hi</div></body>";
    let html = scoped(&head_styles(text), &body_inner(text));
    assert!(html.contains("shadowrootmode=\"open\""));
    assert!(html.contains(".x{color:red}"));
    assert!(html.contains("<div class=\"x\">hi</div>"));
    assert_eq!(scoped("", "<p>plain</p>"), "<p>plain</p>");
}
