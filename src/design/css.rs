//! Turning a design system into CSS, and a component into a full HTML page.
//!
//! The `design/` components are fragments: they use class names from
//! `components/bundle.css` and reference tokens as `var(--name)`, but they do
//! not declare those tokens themselves. A viewer has to supply them. This
//! module writes the token `:root` block and splices it, with the bundle, into
//! a fragment so the renderer has everything it needs — the same split Open
//! Design makes between its `tokens.css` contract and its components.

use std::path::Path;

use super::{Component, DesignSystem, Token, TokenValue};

/// The `:root` block for `theme`, ready to paste into a `<style>`.
///
/// Every token becomes a custom property, the families become `--font-<key>`,
/// and a themed token falls back to its first value when the theme is missing.
pub fn tokens_css(system: &DesignSystem, theme: &str) -> String {
    tokens_block(system, theme, ":root")
}

/// The same block under any selector, e.g. `[data-theme="dark"]`, so a page can
/// carry both themes and switch between them.
pub fn tokens_block(system: &DesignSystem, theme: &str, selector: &str) -> String {
    let mut out = String::from(selector);
    out.push_str(" {\n");
    for token in system.tokens.all() {
        if let Some(value) = token_value_for(token, theme) {
            out.push_str("  --");
            out.push_str(&token.name);
            out.push_str(": ");
            out.push_str(&resolve_refs(system, theme, &value));
            out.push_str(";\n");
        }
    }
    for (key, stack) in &system.tokens.families {
        out.push_str("  --font-");
        out.push_str(key);
        out.push_str(": ");
        out.push_str(stack);
        out.push_str(";\n");
    }
    out.push_str("}\n");
    out
}

/// Replace `{name}` references in a token value with the referenced token's
/// value for the same theme, so the emitted CSS is valid (`--focus: {accent}`
/// would otherwise be ignored by the browser).
fn resolve_refs(system: &DesignSystem, theme: &str, value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    let mut guard = 0;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = after[..end].trim();
        match system
            .token(name)
            .and_then(|token| token_value_for(token, theme))
        {
            Some(value) => out.push_str(&value),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[end + 1..];
        guard += 1;
        if guard > 32 {
            break;
        }
    }
    out.push_str(rest);
    out
}

/// The value of `token` for `theme`: the themed value when there is one, else
/// the `light` value, else any value, else `None`.
pub fn token_value_for(token: &Token, theme: &str) -> Option<String> {
    match &token.value {
        TokenValue::Plain(value) => Some(value.clone()),
        TokenValue::Themed(values) => values
            .get(theme)
            .or_else(|| values.get("light"))
            .or_else(|| values.values().next())
            .cloned(),
    }
}

/// A full HTML document for `component`: its fragment with the token block and
/// the bundle's stylesheet spliced in. The result is self-contained, so the
/// renderer needs no network and no other file.
pub fn preview_html(
    system: &DesignSystem,
    component: &Component,
    theme: &str,
) -> Result<String, String> {
    let preview = component.dir.join("preview.html");
    let mut fragment = std::fs::read_to_string(&preview)
        .map_err(|err| format!("Couldn't read {}: {err}", preview.display()))?;

    let mut style = String::from("<style>\n");
    style.push_str(&tokens_css(system, theme));
    if let Ok(bundle) = std::fs::read_to_string(system.root.join("components").join("bundle.css")) {
        style.push_str(&strip_at_imports(&bundle));
    }
    style.push_str("</style>\n");

    Ok(splice(&mut fragment, &style))
}

/// Insert `style` into an HTML fragment: before `</head>` when there is one,
/// else just inside `<body>`, else at the very front.
fn splice(fragment: &mut String, style: &str) -> String {
    if let Some(at) = fragment.find("</head>") {
        fragment.insert_str(at, style);
    } else if let Some(at) = fragment.find("<body>") {
        fragment.insert_str(at + "<body>".len(), style);
    } else {
        fragment.insert_str(0, style);
    }
    fragment.clone()
}

/// Drop `@import` lines. The bundle imports Google Fonts, which a snapshot
/// over a local file cannot reach; the family stacks fall back to system faces.
fn strip_at_imports(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    for line in css.lines() {
        if line.trim_start().starts_with("@import") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Read a component's README, if it has one, for the design panel to show.
pub fn component_readme(component: &Component) -> Option<String> {
    if let Some(text) = &component.readme {
        return Some(text.clone());
    }
    let path = component.dir.join("README.md");
    std::fs::read_to_string(path).ok()
}

/// A one-line summary of a component: its first prose line, if any.
pub fn component_summary(component: &Component) -> Option<String> {
    let readme = component_readme(component)?;
    readme
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
}

/// Whether `system.root` looks like a `design/` directory.
pub fn is_design_dir(path: &Path) -> bool {
    path.join("design-system.json").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo() -> DesignSystem {
        DesignSystem::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("design")).expect("load")
    }

    #[test]
    fn tokens_css_declares_colors_fonts_and_radii() {
        let css = tokens_css(&repo(), "light");
        assert!(css.contains(":root {"));
        assert!(css.contains("--bg: #f4f5f7;"));
        assert!(css.contains("--accent: #2b54c4;"));
        assert!(css.contains("--font-sans:"));
        assert!(css.contains("--radius-sm:"));
    }

    #[test]
    fn preview_html_is_self_contained() {
        let system = repo();
        let button = system
            .components
            .iter()
            .find(|component| component.name == "Button")
            .expect("Button ships a preview");
        let html = preview_html(&system, button, "light").expect("compose");
        assert!(html.contains("--bg: #f4f5f7;"), "tokens spliced in");
        assert!(html.contains(".ib-btn"), "bundle styles spliced in");
        assert!(html.contains("ib-btn--primary"), "fragment kept");
        assert!(!html.contains("@import"), "external imports stripped");
    }

    #[test]
    fn token_references_are_resolved_in_the_css() {
        let css = tokens_css(&repo(), "light");
        // `focus` is declared as `{accent}`; the browser needs the value.
        assert!(css.contains("--focus: #2b54c4;"), "{css}");
        assert!(!css.contains("{accent}"), "no raw references left");
    }

    #[test]
    fn themed_tokens_fall_back_to_light() {
        let system = repo();
        let bg = system.token("bg").expect("bg");
        assert_eq!(token_value_for(bg, "nope").as_deref(), Some("#f4f5f7"));
    }
}
