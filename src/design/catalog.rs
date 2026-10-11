//! Compose a design system into one live, interactive catalog page.
//!
//! Where a single preview inlines the token `:root` and `bundle.css` into one
//! fragment, the catalog does that once for every component: a sidebar of
//! groups with a search box, a light/dark switch, and each component as a
//! titled, anchored section. It is a whole, self-contained HTML document (no
//! network), so a webview — or a plain browser — shows the components live:
//! hover, focus and any CSS state actually happen, unlike a snapshot.
//!
//! `@dsCard` supplies each section's group and subtitle; `height`/`page` are
//! hints only, because a live page must flow to the width it is given.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::fonts::{self, FontFace};
use super::{Component, DesignSystem, css};

/// The catalog's categories, in Claude Design's order: `(id, label, icon)`,
/// the icon being the inner SVG of a 24-unit line pictogram.
const CATEGORIES: [(&str, &str, &str); 10] = [
    (
        "overview",
        "Overview",
        r#"<path d="M12 7v14"/><path d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"/>"#,
    ),
    (
        "colors",
        "Colors",
        r#"<path d="M12 22a1 1 0 0 1 0-20 10 9 0 0 1 10 9 5 5 0 0 1-5 5h-2.25a1.75 1.75 0 0 0-1.4 2.8l.3.4a1.75 1.75 0 0 1-1.4 2.8z"/><circle cx="13.5" cy="6.5" r=".5"/><circle cx="17.5" cy="10.5" r=".5"/><circle cx="6.5" cy="12.5" r=".5"/><circle cx="8.5" cy="7.5" r=".5"/>"#,
    ),
    (
        "typography",
        "Typography",
        r#"<path d="m15 16 2.5-7.3a1 1 0 0 1 1.9 0L22 16"/><path d="M15.7 14h5.6"/><path d="m2 16 4-9.7a.5.5 0 0 1 .9 0L11 16"/><path d="M3.3 13h6.4"/>"#,
    ),
    (
        "spacing",
        "Spacing",
        r#"<path d="m18 8 4 4-4 4"/><path d="M2 12h20"/><path d="m6 8-4 4 4 4"/>"#,
    ),
    (
        "radius",
        "Corner radius",
        r#"<path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/>"#,
    ),
    (
        "shadows",
        "Shadows",
        r#"<rect width="14" height="14" x="8" y="8" rx="2"/><path d="M4 16a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2"/>"#,
    ),
    (
        "size",
        "Size",
        r#"<rect width="16" height="6" x="4" y="4" rx="2"/><rect width="12" height="6" x="6" y="14" rx="2"/>"#,
    ),
    (
        "components",
        "Components",
        r#"<path d="M21 8a2 2 0 0 0-1-1.7l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.7l7 4a2 2 0 0 0 2 0l7-4a2 2 0 0 0 1-1.7Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>"#,
    ),
    (
        "assets",
        "Assets",
        r#"<rect width="18" height="18" x="3" y="3" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21"/>"#,
    ),
    (
        "files",
        "Files",
        r#"<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9l-.8-1.2A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#,
    ),
];

const SEARCH_ICON: &str = r#"<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>"#;

fn icon(paths: &str) -> String {
    format!("<svg class=\"ds-ico\" viewBox=\"0 0 24 24\" aria-hidden=\"true\">{paths}</svg>")
}

/// The full catalog document for `theme` (`light`'s values become the default).
pub fn html(system: &DesignSystem, _theme: &str) -> Result<String, String> {
    let has_dark = system.tokens.themes.iter().any(|t| t.id == "dark");

    let faces = fonts::faces(system);
    let mut style = fonts::font_face_css(&faces);
    style.push_str(&css::tokens_block(system, "light", ":root"));
    if has_dark {
        style.push_str(&css::tokens_block(system, "dark", "[data-theme=\"dark\"]"));
    }
    if let Ok(bundle) = std::fs::read_to_string(system.root.join("components").join("bundle.css")) {
        style.push_str(&strip_at_imports(&bundle));
    }
    style.push_str(CATALOG_CSS);

    let toggle = if has_dark {
        "<label class=\"ds-theme\"><input type=\"checkbox\" id=\"ds-theme\"> Dark</label>"
    } else {
        ""
    };

    Ok(format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title} — design catalog</title>\n<style>\n{style}</style>\n</head>\n\
         <body>\n<div class=\"ds-shell\">\n{side}\n\
         <main class=\"ds-main\">\n{main}</main>\n</div>\n\
         <script>\n{script}</script>\n</body>\n</html>\n",
        title = escape(&system.meta.title),
        style = style,
        side = sidebar(system, toggle),
        main = main_panes(system, &faces),
        script = SCRIPT,
    ))
}

/// The fixed search over two rails: the categories, and the Components list
/// that slides in over it.
fn sidebar(system: &DesignSystem, toggle: &str) -> String {
    let mut out = String::from("<aside class=\"ds-side\">\n");
    out.push_str(&format!(
        "<h1>{}</h1>\n<div class=\"ds-ns\">{}</div>\n",
        escape(&system.meta.title),
        escape(&system.meta.namespace),
    ));
    out.push_str(&format!(
        "<label class=\"ds-search\">{}<input id=\"ds-q\" type=\"search\" placeholder=\"Search\"></label>\n",
        icon(SEARCH_ICON)
    ));
    out.push_str("<div class=\"ds-rails\" id=\"ds-rails\">\n");

    out.push_str("<nav class=\"ds-rail ds-rail--main\" id=\"ds-rail-main\">\n");
    for (id, label, paths) in CATEGORIES {
        let count = match id {
            "components" => Some(system.components.len()),
            "files" => Some(system.files().len()),
            _ => None,
        }
        .map(|n| format!("<span class=\"ds-count\">{n}</span>"))
        .unwrap_or_default();
        let drill = if id == "components" {
            " data-drill=\"1\""
        } else {
            ""
        };
        if id == "files" {
            out.push_str("<hr class=\"ds-rule\">\n");
        }
        out.push_str(&format!(
            "<button type=\"button\" class=\"ds-item\" data-cat=\"{id}\"{drill}>\
             {icon}<span class=\"ds-label\">{label}</span>{count}</button>\n",
            icon = icon(paths),
            label = escape(label),
        ));
    }
    out.push_str("</nav>\n");

    out.push_str("<nav class=\"ds-rail ds-rail--sub\" id=\"ds-rail-sub\">\n");
    out.push_str(
        "<button type=\"button\" class=\"ds-back\" data-back>\
         <span class=\"ds-chevron\">\u{2039}</span>Components</button>\n",
    );
    for component in ordered_components(system) {
        let search = format!(
            "{} {} {}",
            component.name,
            component.card.group.as_deref().unwrap_or(""),
            component.card.subtitle.as_deref().unwrap_or(""),
        )
        .to_lowercase();
        out.push_str(&format!(
            "<a class=\"ds-subitem\" data-name=\"{}\" href=\"#c-{}\">{}</a>\n",
            escape(&search),
            slug(&component.name),
            escape(&component.name),
        ));
    }
    out.push_str("</nav>\n</div>\n");
    out.push_str(toggle);
    out.push_str("\n</aside>\n");
    out
}

/// Every category in one scrolling flow, Components and Files as summaries
/// there; their full views are separate pages the script swaps in.
fn main_panes(system: &DesignSystem, faces: &[FontFace]) -> String {
    let flow: String = [
        ("overview", overview(system)),
        ("colors", colors(system)),
        ("typography", typography(system, faces)),
        ("spacing", spacing(system)),
        ("radius", radius(system)),
        ("shadows", shadows(system)),
        ("size", size(system)),
        ("components", components_summary(system)),
        ("assets", assets(system)),
        ("files", files_summary(system)),
    ]
    .into_iter()
    .map(|(id, inner)| {
        format!("<section class=\"ds-cat\" id=\"cat-{id}\" data-cat=\"{id}\">\n{inner}</section>\n")
    })
    .collect();
    format!(
        "<div class=\"ds-flow\" id=\"ds-flow\">\n{flow}</div>\n\
         <section class=\"ds-page\" data-page=\"components\" hidden>\n{}</section>\n\
         <section class=\"ds-page\" data-page=\"files\" hidden>\n{}</section>\n",
        components(system),
        files(system),
    )
}

fn cta(page: &str, label: &str) -> String {
    format!(
        "<button type=\"button\" class=\"ib-btn ds-cta\" data-open=\"{page}\">{}\
         <span aria-hidden=\"true\">\u{2192}</span></button>\n",
        escape(label)
    )
}

fn components_summary(system: &DesignSystem) -> String {
    let mut out = head(
        "Components",
        &format!("{} components", system.components.len()),
    );
    for (group, list) in system.component_groups() {
        let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
        out.push_str(&format!(
            "<div class=\"ds-row\"><span class=\"ds-name\">{} · {}</span>\
             <span class=\"ds-usage\">{}</span></div>\n",
            escape(&group),
            list.len(),
            escape(&names.join(", ")),
        ));
    }
    out.push_str(&cta("components", "Browse components"));
    out
}

fn files_summary(system: &DesignSystem) -> String {
    let files = system.files();
    let mut out = head("Files", &format!("{} files", files.len()));
    let mut folders: Vec<(&str, usize, u64)> = Vec::new();
    for file in &files {
        let folder = file.rel.split_once('/').map_or(".", |(dir, _)| dir);
        match folders.iter_mut().find(|(name, ..)| *name == folder) {
            Some(entry) => {
                entry.1 += 1;
                entry.2 += file.bytes;
            }
            None => folders.push((folder, 1, file.bytes)),
        }
    }
    for (folder, count, bytes) in folders {
        out.push_str(&format!(
            "<div class=\"ds-row\"><span class=\"ds-name\">{}</span>\
             <span class=\"ds-val\">{count} files · {}</span></div>\n",
            escape(if folder == "." { "./" } else { folder }),
            human_bytes(bytes),
        ));
    }
    out.push_str(&cta("files", "Browse files"));
    out
}

/// The themed token ids, or `light` when the system declares no themes.
fn theme_ids(system: &DesignSystem) -> Vec<&str> {
    if system.tokens.themes.is_empty() {
        vec!["light"]
    } else {
        system.tokens.themes.iter().map(|t| t.id.as_str()).collect()
    }
}

fn head(title: &str, note: &str) -> String {
    format!(
        "<header class=\"ds-head\"><h1>{}</h1><p>{}</p></header>\n",
        escape(title),
        escape(note)
    )
}

fn overview(system: &DesignSystem) -> String {
    let themes = theme_ids(system);
    let mut out = head(
        &system.meta.title,
        &format!("{} · v{}", system.meta.namespace, system.meta.version),
    );
    out.push_str(&format!(
        "<p class=\"ds-note\">{} components · {} color, {} spacing, {} radius, {} size, \
         {} shadow, {} type styles · themes {}</p>\n",
        system.components.len(),
        system.tokens.color.len(),
        system.tokens.spacing.len(),
        system.tokens.radius.len(),
        system.tokens.size.len(),
        system.tokens.shadow.len(),
        system.tokens.type_style_count(),
        themes.join(", "),
    ));
    if let Some(change) = &system.meta.last_change {
        out.push_str(&format!(
            "<p class=\"ds-note\">Last change {}, {} via {}</p>\n",
            escape(change.by.as_deref().unwrap_or("unknown")),
            escape(change.at.as_deref().unwrap_or("")),
            escape(change.via.as_deref().unwrap_or("")),
        ));
    }
    if let Some(brief) = system.brief() {
        out.push_str("<div class=\"ds-brief\">\n");
        out.push_str(&markdown(&brief));
        out.push_str("</div>\n");
    }
    out
}

fn colors(system: &DesignSystem) -> String {
    let themes = theme_ids(system);
    let mut out = head("Colors", &format!("{} tokens", system.tokens.color.len()));
    for token in &system.tokens.color {
        out.push_str("<div class=\"ds-row\">");
        out.push_str(&format!(
            "<span class=\"ds-name\">{}</span>",
            escape(&token.name)
        ));
        for theme in &themes {
            let value = css::resolved_value(system, token, theme).unwrap_or_default();
            out.push_str(&format!(
                "<span class=\"ds-swatch\" style=\"background:{value}\"></span>\
                 <code class=\"ds-val\">{value}</code>",
                value = escape(&value),
            ));
        }
        if let Some(usage) = &token.usage {
            out.push_str(&format!(
                "<span class=\"ds-usage\">{}</span>",
                escape(usage)
            ));
        }
        out.push_str("</div>\n");
    }
    out
}

const PANGRAM: &str = "The quick brown fox jumps over the lazy dog";

fn typography(system: &DesignSystem, faces: &[FontFace]) -> String {
    let mut out = head(
        "Typography",
        &format!(
            "{} styles · {} font files",
            system.tokens.type_style_count(),
            faces.len()
        ),
    );
    for group in &system.tokens.type_groups {
        let stack = system
            .tokens
            .families
            .get(&group.family)
            .cloned()
            .unwrap_or_default();
        let face = fonts::primary(&stack);
        out.push_str(&format!(
            "<div class=\"ds-tgroup\"><h2>{}</h2>\
             <div class=\"ds-ttable\"><div class=\"ds-thead\"><span>Name</span><span>Preview</span></div>\n",
            escape(&group.name)
        ));
        for style in &group.styles {
            let size = style.font_size.clone().unwrap_or_else(|| "13px".into());
            let weight = style.weight.unwrap_or(400);
            let line = style.line_height.clone().unwrap_or_else(|| "normal".into());
            let tracking = style
                .letter_spacing
                .clone()
                .unwrap_or_else(|| "normal".into());
            let mut chips = vec![
                format!("{face} · {}", size.trim_end_matches("px")),
                weight.to_string(),
                format!("line {line}"),
            ];
            if style.letter_spacing.is_some() {
                chips.push(format!("tracking {tracking}"));
            }
            let chips: String = chips
                .iter()
                .map(|chip| format!("<span class=\"ds-chip\">{}</span>", escape(chip)))
                .collect();
            let usage = style
                .usage
                .as_deref()
                .map(|usage| format!("<span class=\"ds-usage\">{}</span>", escape(usage)))
                .unwrap_or_default();
            out.push_str(&format!(
                "<div class=\"ds-trow\"><code class=\"ds-tname\">{name}</code>\
                 <div class=\"ds-tpreview\"><div class=\"ds-type__sample\" style=\"font-family:{stack};\
                 font-size:{size};font-weight:{weight};line-height:{line};letter-spacing:{tracking}\" \
                 contenteditable=\"plaintext-only\" spellcheck=\"false\">{PANGRAM}</div>\
                 <div class=\"ds-chips\">{chips}{usage}</div></div></div>\n",
                name = escape(&style.name),
                stack = escape(&stack),
                size = escape(&size),
                line = escape(&line),
                tracking = escape(&tracking),
            ));
        }
        out.push_str("</div></div>\n");
    }
    out.push_str(&font_files(system, faces));
    out
}

/// One specimen per shipped family, then whether each declared stack's first
/// face is embedded or left to the system.
fn font_files(system: &DesignSystem, faces: &[FontFace]) -> String {
    let mut out = String::from("<div class=\"ds-tgroup\"><h2>Fonts</h2>\n");
    for (key, stack) in &system.tokens.families {
        let face = fonts::primary(stack);
        let shipped = faces.iter().any(|f| f.family == face);
        out.push_str(&format!(
            "<div class=\"ds-row\"><span class=\"ds-name\">--font-{key}</span>\
             <span class=\"ds-chip{flag}\">{state}</span><code class=\"ds-val\">{stack}</code></div>\n",
            key = escape(key),
            flag = if shipped { "" } else { " ds-chip--warn" },
            state = if shipped { "embedded" } else { "system fallback" },
            stack = escape(stack),
        ));
    }
    let mut families: Vec<&str> = faces.iter().map(|f| f.family.as_str()).collect();
    families.dedup();
    for family in families {
        out.push_str(&format!(
            "<div class=\"ds-specimen\"><div class=\"ds-specimen__aa\" style=\"font-family:'{css}'\">Aa</div>\
             <div class=\"ds-specimen__body\"><h3>{name}</h3>\
             <p style=\"font-family:'{css}'\">ABCDEFGHIJKLMNOPQRSTUVWXYZ<br>abcdefghijklmnopqrstuvwxyz<br>0123456789 !?&amp;@#%*()</p>\n",
            css = escape(&family.replace('\'', "")),
            name = escape(family),
        ));
        for face in faces.iter().filter(|f| f.family == family) {
            out.push_str(&format!(
                "<div class=\"ds-weight\" style=\"font-family:'{css}';font-weight:{weight};font-style:{italic}\">\
                 <span>{style} {weight}</span><code class=\"ds-val\">{file}</code></div>\n",
                css = escape(&family.replace('\'', "")),
                weight = face.weight,
                italic = if face.italic { "italic" } else { "normal" },
                style = escape(&face.style),
                file = escape(&face.file),
            ));
        }
        out.push_str("</div></div>\n");
    }
    out.push_str("</div>\n");
    out
}

fn spacing(system: &DesignSystem) -> String {
    let mut out = head("Spacing", &format!("{} steps", system.tokens.spacing.len()));
    for token in &system.tokens.spacing {
        let value = css::resolved_value(system, token, "light").unwrap_or_default();
        let px = parse_px(&value).unwrap_or(0.0);
        out.push_str(&format!(
            "<div class=\"ds-row\"><span class=\"ds-name\">{}</span>\
             <span class=\"ds-bar\" style=\"width:{px}px\"></span>\
             <code class=\"ds-val\">{}</code></div>\n",
            escape(&token.name),
            escape(&value),
        ));
    }
    out
}

fn radius(system: &DesignSystem) -> String {
    let mut out = head(
        "Corner radius",
        &format!("{} tokens", system.tokens.radius.len()),
    );
    for token in &system.tokens.radius {
        let value = css::resolved_value(system, token, "light").unwrap_or_default();
        let px = parse_px(&value).unwrap_or(0.0).clamp(0.0, 28.0);
        out.push_str(&format!(
            "<div class=\"ds-row\"><span class=\"ds-name\">{}</span>\
             <span class=\"ds-box\" style=\"border-radius:{px}px\"></span>\
             <code class=\"ds-val\">{}</code></div>\n",
            escape(&token.name),
            escape(&value),
        ));
    }
    out
}

fn shadows(system: &DesignSystem) -> String {
    let themes = theme_ids(system);
    let mut out = head("Shadows", &format!("{} tokens", system.tokens.shadow.len()));
    for token in &system.tokens.shadow {
        let value = css::resolved_value(system, token, "light").unwrap_or_default();
        out.push_str(&format!(
            "<div class=\"ds-row\"><span class=\"ds-name\">{}</span>\
             <span class=\"ds-shadow\" style=\"box-shadow:{}\"></span>",
            escape(&token.name),
            escape(&value),
        ));
        for theme in &themes {
            let value = css::resolved_value(system, token, theme).unwrap_or_default();
            out.push_str(&format!("<code class=\"ds-val\">{}</code>", escape(&value)));
        }
        out.push_str("</div>\n");
    }
    out
}

fn size(system: &DesignSystem) -> String {
    let mut out = head("Size", &format!("{} tokens", system.tokens.size.len()));
    for token in &system.tokens.size {
        let value = css::resolved_value(system, token, "light").unwrap_or_default();
        let usage = token
            .usage
            .as_deref()
            .map(|usage| format!("<span class=\"ds-usage\">{}</span>", escape(usage)))
            .unwrap_or_default();
        out.push_str(&format!(
            "<div class=\"ds-row\"><span class=\"ds-name\">{}</span>\
             <code class=\"ds-val\">{}</code>{usage}</div>\n",
            escape(&token.name),
            escape(&value),
        ));
    }
    if let Some(note) = &system.tokens.size_note {
        out.push_str(&format!("<p class=\"ds-note\">{}</p>\n", escape(note)));
    }
    out
}

fn components(system: &DesignSystem) -> String {
    let mut out = head(
        "Components",
        &format!("{} components", system.components.len()),
    );
    for component in ordered_components(system) {
        out.push_str(&section(component, &format!("c-{}", slug(&component.name))));
    }
    out
}

fn assets(system: &DesignSystem) -> String {
    let assets = system.assets();
    let mut out = head("Assets", &format!("{} assets", assets.len()));
    let mut group = String::new();
    for asset in &assets {
        if asset.group != group {
            out.push_str(&format!(
                "<h3 class=\"ds-sub\">{}</h3>\n",
                escape(&asset.group)
            ));
            group = asset.group.clone();
        }
        let size = if asset.bytes > 0 {
            format!("<span class=\"ds-val\">{}</span>", human_bytes(asset.bytes))
        } else {
            String::new()
        };
        out.push_str(&format!(
            "<div class=\"ds-row\"><span class=\"ds-name\">{}</span>{size}</div>\n",
            escape(&asset.name),
        ));
    }
    out
}

fn files(system: &DesignSystem) -> String {
    let files = system.files();
    let mut out = head("Files", &format!("{} files", files.len()));
    for file in &files {
        out.push_str(&format!(
            "<div class=\"ds-row\"><span class=\"ds-name\">{}</span>\
             <span class=\"ds-val\">{}</span></div>\n",
            escape(&file.rel),
            human_bytes(file.bytes),
        ));
    }
    out
}

/// Every component by name, the order the rail and the pane list them in.
fn ordered_components(system: &DesignSystem) -> Vec<&Component> {
    let mut list: Vec<&Component> = system.components.iter().collect();
    list.sort_by_key(|component| component.name.to_lowercase());
    list
}

/// One component: its title, subtitle and live fragment in a bordered stage.
fn section(component: &Component, id: &str) -> String {
    let group = component.card.group.clone().unwrap_or_default();
    let subtitle = component
        .card
        .subtitle
        .clone()
        .or_else(|| css::component_summary(component))
        .unwrap_or_default();
    let page = if component.card.page {
        " ds-stage--page"
    } else {
        ""
    };
    let meta = match component.card.height {
        Some(height) => format!("<span class=\"ds-dim\">{group} · {height}pt</span>"),
        None => format!("<span class=\"ds-dim\">{group}</span>"),
    };
    let body = fragment(component);
    format!(
        "<section class=\"ds-comp\" id=\"{id}\" data-name=\"{slug}\" data-group=\"{group}\">\n\
         <div class=\"ds-comp__head\"><h2>{name}</h2><p>{subtitle}</p>{meta}</div>\n\
         <div class=\"ds-stage{page}\">{body}</div></section>\n",
        slug = escape(&component.name.to_lowercase()),
        group = escape(&group.to_lowercase()),
        name = escape(&component.name),
        subtitle = escape(&subtitle),
        meta = meta,
        page = page,
        body = body,
    )
}

/// An element id from a component name: `BoardScreen` → `boardscreen`.
fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// A number with an optional `px` suffix (`"4px"`, `"0"`, `"16"`).
fn parse_px(value: &str) -> Option<f32> {
    value.trim().trim_end_matches("px").trim().parse().ok()
}

fn human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.0} kB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// A small markdown reader for the Overview brief: headings, lists, paragraphs
/// and inline code or bold.
fn markdown(text: &str) -> String {
    let mut out = String::new();
    let mut in_list = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("### ") {
            out.push_str(&format!("<h3>{}</h3>\n", inline(rest)));
        } else if let Some(rest) = line.strip_prefix("## ") {
            out.push_str(&format!("<h2>{}</h2>\n", inline(rest)));
        } else if let Some(rest) = line.strip_prefix("# ") {
            out.push_str(&format!("<h1>{}</h1>\n", inline(rest)));
        } else if let Some(rest) = line.strip_prefix("- ") {
            if !in_list {
                out.push_str("<ul>\n");
                in_list = true;
            }
            out.push_str(&format!("<li>{}</li>\n", inline(rest)));
        } else {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            out.push_str(&format!("<p>{}</p>\n", inline(line)));
        }
    }
    if in_list {
        out.push_str("</ul>\n");
    }
    out
}

/// Inline `code` and **bold** in an escaped line.
fn inline(text: &str) -> String {
    let escaped = escape(text);
    let mut out = String::new();
    let mut rest = escaped.as_str();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("**")
            && let Some(end) = after.find("**")
        {
            out.push_str("<strong>");
            out.push_str(&after[..end]);
            out.push_str("</strong>");
            rest = &after[end + 2..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('`')
            && let Some(end) = after.find('`')
        {
            out.push_str("<code>");
            out.push_str(&after[..end]);
            out.push_str("</code>");
            rest = &after[end + 1..];
            continue;
        }
        let ch = rest.chars().next().unwrap_or(' ');
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// The body of a component's fragment, with its `@dsCard` line dropped.
fn fragment(component: &Component) -> String {
    let text = std::fs::read_to_string(component.dir.join("preview.html")).unwrap_or_default();
    let text = strip_card_comment(&text);
    scoped(&head_styles(&text), &body_inner(&text))
}

/// A body that brings its own styles goes in a shadow root, so rules such as
/// `html, body { overflow:hidden }` stay inside the card. Tokens still reach it:
/// custom properties inherit across the shadow boundary.
fn scoped(styles: &str, body: &str) -> String {
    if styles.is_empty() {
        return body.to_string();
    }
    format!(
        "<div class=\"ds-scoped\"><template shadowrootmode=\"open\">\
         <style>:host{{display:block}}\n{styles}</style>{body}</template></div>"
    )
}

/// The contents of every `<style>` before `<body>`.
fn head_styles(text: &str) -> String {
    let head = &text[..text.find("<body").unwrap_or(0)];
    let mut out = String::new();
    let mut rest = head;
    while let Some(start) = rest.find("<style") {
        let Some(open_end) = rest[start..].find('>') else {
            break;
        };
        let inner = &rest[start + open_end + 1..];
        let Some(end) = inner.find("</style>") else {
            break;
        };
        out.push_str(inner[..end].trim());
        out.push('\n');
        rest = &inner[end + "</style>".len()..];
    }
    out
}

/// Drop a leading `<!-- @dsCard … -->` comment.
fn strip_card_comment(text: &str) -> String {
    let Some(at) = text.find("@dsCard") else {
        return text.to_string();
    };
    let start = text[..at].rfind("<!--").unwrap_or(at);
    match text[at..].find("-->") {
        Some(end) => {
            let end = at + end + 3;
            format!("{}{}", &text[..start], &text[end..])
        }
        None => text.to_string(),
    }
}

/// The content between `<body>` and `</body>`, or the whole text.
fn body_inner(text: &str) -> String {
    let Some(start) = text.find("<body") else {
        return text.to_string();
    };
    let Some(open_end) = text[start..].find('>') else {
        return text.to_string();
    };
    let inner_start = start + open_end + 1;
    match text[inner_start..].find("</body>") {
        Some(end) => text[inner_start..inner_start + end].trim().to_string(),
        None => text[inner_start..].trim().to_string(),
    }
}

/// Where the catalog is written when no path is given.
pub fn default_path(system: &DesignSystem) -> PathBuf {
    let slug: String = system
        .meta
        .namespace
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    std::env::temp_dir().join(format!("iboard-catalog-{slug}.html"))
}

/// Write the catalog to `out`, or to [`default_path`].
pub fn write(system: &DesignSystem, theme: &str, out: Option<&Path>) -> Result<PathBuf, String> {
    let html = html(system, theme)?;
    let path = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_path(system));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Couldn't create {}: {err}", parent.display()))?;
    }
    std::fs::write(&path, html)
        .map_err(|err| format!("Couldn't write {}: {err}", path.display()))?;
    Ok(path)
}

/// Write the catalog and show it: the webview helper when it was built, else
/// the user's browser. Returns the written path.
pub fn show(system: &DesignSystem, theme: &str) -> Result<PathBuf, String> {
    let path = write(system, theme, None)?;
    open(&path, &format!("{} — design catalog", system.meta.title))?;
    Ok(path)
}

/// Show an HTML file in the catalog webview, or the browser without it.
pub fn open(html: &Path, title: &str) -> Result<(), String> {
    match viewer() {
        Some(viewer) => Command::new(viewer)
            .arg(html)
            .arg(title)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("Couldn't open the catalog window: {err}")),
        None => open_in_browser(html),
    }
}

/// The webview helper `build.rs` compiled, when there is one.
pub fn viewer() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("IBOARD_CATALOG") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    build_viewer()
}

#[cfg(apple_catalog)]
fn build_viewer() -> Option<PathBuf> {
    let path = PathBuf::from(env!("IBOARD_CATALOG_HELPER"));
    path.is_file().then_some(path)
}

#[cfg(not(apple_catalog))]
fn build_viewer() -> Option<PathBuf> {
    None
}

/// Whether a live catalog window can be opened here.
pub fn available() -> bool {
    viewer().is_some()
}

fn open_in_browser(html: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(format!(
        "No catalog window; open {} in a browser.",
        html.display()
    ));
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    command
        .arg(html)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("Couldn't open {}: {err}", html.display()))
}

/// Drop `@import` lines: the catalog is self-contained, so the family stacks
/// fall back to system faces rather than reaching for Google Fonts.
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

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// The catalog's own chrome, on top of the bundle.
const CATALOG_CSS: &str = r##"
html,body{height:100%}
body{margin:0;background:var(--bg);color:var(--ink);font-family:var(--font-sans);font-size:13px;line-height:18px;-webkit-font-smoothing:antialiased}
*{box-sizing:border-box}
.ds-shell{display:grid;grid-template-columns:236px minmax(0,1fr);height:100vh}
.ds-side{display:flex;flex-direction:column;border-right:1px solid var(--line);background:var(--surface);padding:16px;overflow:hidden}
.ds-side h1{font:600 15px/20px var(--font-sans);margin:0}
.ds-ns{font:11px/14px var(--font-mono);color:var(--ink-muted);margin:2px 0 12px}
.ds-search{display:flex;align-items:center;gap:6px;width:100%;height:28px;padding:0 8px;border:1px solid var(--line-strong);border-radius:var(--radius-sm);background:var(--surface);color:var(--ink-faint);margin-bottom:10px;flex:0 0 auto;cursor:text}
.ds-search:focus-within{outline:2px solid var(--focus);outline-offset:1px}
.ds-search input{flex:1 1 auto;min-width:0;height:100%;padding:0;border:0;outline:0;background:none;color:var(--ink);font:13px/18px var(--font-sans)}
.ds-ico{width:16px;height:16px;flex:0 0 auto;fill:none;stroke:currentColor;stroke-width:1.6;stroke-linecap:round;stroke-linejoin:round}
.ds-ico circle[r=".5"]{fill:currentColor}
.ds-label{flex:1 1 auto}
.ds-rule{border:0;border-top:1px solid var(--line);margin:6px 0}
.ds-rails{position:relative;flex:1 1 auto;min-height:0;overflow:hidden}
.ds-rail{position:absolute;inset:0;overflow:auto;transition:transform .22s ease}
.ds-rail--main{transform:translateX(0)}
.ds-rail--sub{transform:translateX(100%)}
.ds-rails.ds-open .ds-rail--main{transform:translateX(-100%)}
.ds-rails.ds-open .ds-rail--sub{transform:translateX(0)}
.ds-item{display:flex;align-items:center;gap:10px;width:100%;text-align:left;background:none;border:0;padding:6px 8px;border-radius:var(--radius-sm);color:var(--ink-muted);font:13px/18px var(--font-sans);cursor:pointer}
.ds-item:hover{background:var(--surface-hover);color:var(--ink)}
.ds-item[aria-current]{background:var(--accent-soft);color:var(--ink)}
.ds-count{font:11px/14px var(--font-mono);color:var(--ink-faint)}
.ds-back{display:flex;align-items:center;gap:6px;width:100%;text-align:left;background:none;border:0;padding:6px 8px;border-radius:var(--radius-sm);color:var(--ink);font:600 13px/18px var(--font-sans);cursor:pointer}
.ds-back:hover{background:var(--surface-hover)}
.ds-chevron{font-size:15px;color:var(--ink-muted)}
.ds-subitem{display:block;padding:4px 8px;border-radius:var(--radius-sm);color:var(--ink-muted);text-decoration:none}
.ds-subitem:hover{background:var(--surface-hover);color:var(--ink)}
.ds-subitem[aria-current]{background:var(--accent-soft);color:var(--ink)}
.ds-theme{display:flex;align-items:center;gap:6px;margin-top:12px;padding-top:12px;border-top:1px solid var(--line);color:var(--ink-muted);font-size:12px;cursor:pointer;flex:0 0 auto}
.ds-main{position:relative;overflow:auto;padding:24px 32px 64px}
.ds-flow[hidden],.ds-page[hidden]{display:none}
.ds-flow>.ds-cat{padding-bottom:24px;margin-bottom:24px;border-bottom:1px solid var(--line)}
.ds-flow>.ds-cat:last-child{border-bottom:0;min-height:calc(100vh - 88px)}
.ds-cta{margin-top:14px}
.ds-head{max-width:760px;margin-bottom:20px}
.ds-head h1{font:600 20px/26px var(--font-sans);margin:0}
.ds-head p{color:var(--ink-muted);margin:4px 0 0}
.ds-note{color:var(--ink-faint);font:11px/16px var(--font-mono);margin:8px 0 0}
.ds-sub{font:600 11px/14px var(--font-sans);letter-spacing:.06em;text-transform:uppercase;color:var(--ink-faint);margin:20px 0 6px}
.ds-row{display:flex;align-items:center;gap:8px;padding:5px 0;border-bottom:1px solid var(--line);max-width:900px}
.ds-name{flex:0 0 170px;font:11px/14px var(--font-mono);color:var(--ink);overflow-wrap:anywhere}
.ds-val{font:11px/14px var(--font-mono);color:var(--ink-muted)}
.ds-usage{font-size:11px;color:var(--ink-faint)}
.ds-swatch{width:18px;height:18px;border-radius:var(--radius-sm);border:1px solid var(--line-strong);flex:0 0 auto}
.ds-bar{height:12px;background:var(--accent);border-radius:2px;flex:0 0 auto;min-width:1px}
.ds-box{width:40px;height:40px;background:var(--surface-sunken);border:1px solid var(--line-strong);flex:0 0 auto}
.ds-shadow{width:64px;height:40px;background:var(--surface);border-radius:var(--radius-md);flex:0 0 auto}
.ds-type__sample{color:var(--ink);overflow-wrap:anywhere;outline:none}
.ds-type__sample:focus{box-shadow:inset 0 -1px 0 var(--focus)}
.ds-tgroup{max-width:1040px;margin:0 0 40px}
.ds-tgroup>h2{font:500 15px/20px var(--font-sans);margin:0 0 10px}
.ds-ttable{border:1px solid var(--line);border-radius:var(--radius-md);background:var(--surface);overflow:hidden}
.ds-thead,.ds-trow{display:grid;grid-template-columns:180px minmax(0,1fr)}
.ds-thead{color:var(--ink-faint);font-size:12px;border-bottom:1px solid var(--line)}
.ds-thead span,.ds-tname,.ds-tpreview{padding:10px 16px}
.ds-trow+.ds-trow{border-top:1px solid var(--line)}
.ds-thead span+span,.ds-tpreview{border-left:1px solid var(--line)}
.ds-tname{font:12px/18px var(--font-mono);color:var(--ink)}
.ds-chips{display:flex;flex-wrap:wrap;align-items:center;gap:6px;margin-top:8px}
.ds-chip{display:inline-flex;align-items:center;height:22px;padding:0 8px;border:1px solid var(--line);border-radius:var(--radius-sm);font:12px/16px var(--font-sans);color:var(--ink-muted);white-space:nowrap}
.ds-chip--warn{border-style:dashed;color:var(--ink-faint)}
.ds-specimen{display:grid;grid-template-columns:120px minmax(0,1fr);gap:20px;margin-top:16px;padding:16px;border:1px solid var(--line);border-radius:var(--radius-md);background:var(--surface)}
.ds-specimen__aa{font-size:64px;line-height:72px;color:var(--ink)}
.ds-specimen__body h3{font:600 13px/18px var(--font-sans);margin:0 0 6px}
.ds-specimen__body p{font-size:18px;line-height:26px;margin:0 0 10px;color:var(--ink);overflow-wrap:anywhere}
.ds-weight{display:flex;align-items:baseline;justify-content:space-between;gap:12px;padding:4px 0;border-top:1px solid var(--line);font-size:15px;color:var(--ink)}
.ds-brief{max-width:720px}
.ds-brief h1{font:600 18px/24px var(--font-sans)}
.ds-brief h2{font:600 15px/20px var(--font-sans);margin:20px 0 6px}
.ds-brief h3{font:600 13px/18px var(--font-sans);margin:16px 0 4px}
.ds-brief p{margin:8px 0;color:var(--ink-muted)}
.ds-brief ul{margin:8px 0;padding-left:20px;color:var(--ink-muted)}
.ds-brief code{font-family:var(--font-mono);font-size:12px;background:var(--surface-sunken);padding:1px 4px;border-radius:var(--radius-sm)}
.ds-brief strong{color:var(--ink)}
.ds-comp{margin:0 0 40px;scroll-margin-top:16px;max-width:900px}
.ds-comp__head{display:flex;align-items:baseline;gap:8px;flex-wrap:wrap;margin-bottom:10px}
.ds-comp__head h2{font:600 15px/20px var(--font-sans);margin:0}
.ds-comp__head p{color:var(--ink-muted);margin:0;flex:1 1 320px}
.ds-dim{font:11px/14px var(--font-mono);color:var(--ink-faint)}
.ds-stage{border:1px solid var(--line);border-radius:var(--radius-md);background:var(--bg);overflow:auto}
.ds-stage--page{max-height:660px}
"##;

/// Category switching, the rail slide, search filtering and the theme toggle.
/// Inline and tiny: the catalog is one file with no network.
const SCRIPT: &str = r##"
(function () {
  var root = document.documentElement;
  var rails = document.getElementById('ds-rails');
  var items = Array.prototype.slice.call(document.querySelectorAll('.ds-item'));
  var subs = Array.prototype.slice.call(document.querySelectorAll('.ds-subitem'));

  var main = document.querySelector('.ds-main');
  var flow = document.getElementById('ds-flow');
  var pages = Array.prototype.slice.call(document.querySelectorAll('.ds-page'));
  var cats = Array.prototype.slice.call(flow.querySelectorAll('.ds-cat'));

  function mark(id) {
    items.forEach(function (b) {
      if (b.dataset.cat === id) { b.setAttribute('aria-current', 'page'); }
      else { b.removeAttribute('aria-current'); }
    });
  }
  function page(id) {
    flow.hidden = true;
    pages.forEach(function (p) { p.hidden = p.dataset.page !== id; });
    mark(id);
    main.scrollTop = 0;
  }
  function select(id) {
    var smooth = !flow.hidden;
    flow.hidden = false;
    pages.forEach(function (p) { p.hidden = true; });
    var target = document.getElementById('cat-' + id);
    if (target) { main.scrollTo({ top: target.offsetTop, behavior: smooth ? 'smooth' : 'auto' }); }
    mark(id);
  }
  function spy() {
    if (flow.hidden) { return; }
    var line = main.scrollTop + 80;
    var current = cats[0];
    cats.forEach(function (s) { if (s.offsetTop <= line) { current = s; } });
    if (main.scrollTop + main.clientHeight >= main.scrollHeight - 2) { current = cats[cats.length - 1]; }
    mark(current.dataset.cat);
  }
  function open(drill) { rails.classList.toggle('ds-open', !!drill); }

  main.addEventListener('scroll', spy, { passive: true });
  items.forEach(function (b) {
    b.addEventListener('click', function () {
      var id = b.dataset.cat;
      if (pages.some(function (p) { return p.dataset.page === id; })) { page(id); } else { select(id); }
      open(b.dataset.drill === '1');
    });
  });
  document.querySelectorAll('[data-open]').forEach(function (c) {
    c.addEventListener('click', function () { page(c.dataset.open); open(c.dataset.open === 'components'); });
  });
  var back = document.querySelector('.ds-back');
  if (back) { back.addEventListener('click', function () { open(false); select('components'); }); }
  subs.forEach(function (a) {
    a.addEventListener('click', function () { page('components'); open(true); });
  });
  spy();

  var q = document.getElementById('ds-q');
  q.addEventListener('input', function () {
    var v = q.value.trim().toLowerCase();
    items.forEach(function (b) { b.hidden = v !== '' && b.textContent.toLowerCase().indexOf(v) < 0; });
    subs.forEach(function (a) { a.hidden = v !== '' && (a.dataset.name || '').indexOf(v) < 0; });
  });

  var toggle = document.getElementById('ds-theme');
  if (toggle) {
    try { if (localStorage.getItem('ds-theme') === 'dark') { root.dataset.theme = 'dark'; toggle.checked = true; } } catch (e) {}
    toggle.addEventListener('change', function () {
      root.dataset.theme = toggle.checked ? 'dark' : 'light';
      try { localStorage.setItem('ds-theme', toggle.checked ? 'dark' : 'light'); } catch (e) {}
    });
  }

  var observer = new IntersectionObserver(function (entries) {
    entries.forEach(function (entry) {
      if (!entry.isIntersecting) { return; }
      subs.forEach(function (a) { a.removeAttribute('aria-current'); });
      var active = document.querySelector('.ds-subitem[href="#' + entry.target.id + '"]');
      if (active) { active.setAttribute('aria-current', 'page'); }
    });
  }, { rootMargin: '-10% 0px -80% 0px' });
  document.querySelectorAll('.ds-comp').forEach(function (s) { observer.observe(s); });
})();
"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> DesignSystem {
        DesignSystem::discover(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("repo design")
    }

    #[test]
    fn the_catalog_carries_every_component_and_the_tokens() {
        let html = html(&repo(), "light").expect("compose");
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("--bg: #f4f5f7;"), "token block inlined");
        assert!(html.contains(".ib-btn"), "bundle inlined");
        assert!(html.contains("[data-theme=\"dark\"]"), "dark theme block");
        assert!(html.contains("id=\"ds-q\""), "search box");
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
        let names: Vec<String> = ordered_components(&system)
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
        assert!(html.contains("@font-face{font-family:\"IBM Plex Sans\";font-weight:600"));
        assert!(
            html.contains("class=\"ds-ttable\""),
            "a Name | Preview table"
        );
        assert!(
            html.contains(">IBM Plex Sans · 15</span>"),
            "a family · size chip"
        );
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

    #[test]
    fn card_meta_parses_keys_and_the_page_flag() {
        let meta = super::super::parse_card_meta(
            "<!-- @dsCard group=\"Screens\" height=640 page subtitle=\"Click a column\" -->",
        );
        assert_eq!(meta.group.as_deref(), Some("Screens"));
        assert_eq!(meta.height, Some(640));
        assert!(meta.page);
        assert_eq!(meta.subtitle.as_deref(), Some("Click a column"));

        let minimal = super::super::parse_card_meta("<!-- @dsCard height=56 -->");
        assert_eq!(minimal.height, Some(56));
        assert!(!minimal.page);
        assert!(
            super::super::parse_card_meta("no card here")
                .group
                .is_none()
        );
    }
}
