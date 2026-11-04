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
use crate::format;

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

/// The full catalog document. It opens in `theme` when that is `dark` and the
/// system has one; otherwise in light, or in the theme last picked with its
/// switch.
fn html(system: &DesignSystem, theme: &str) -> Result<String, String> {
    let has_dark = system.tokens.themes.iter().any(|t| t.id == "dark");
    let dark = has_dark && theme == "dark";

    let faces = fonts::faces(system);
    let mut style = fonts::font_face_css(&faces);
    style.push_str(&css::tokens_block(system, "light", ":root"));
    if has_dark {
        style.push_str(&css::tokens_block(system, "dark", "[data-theme=\"dark\"]"));
    }
    style.push_str(&css::bundle_css(system));
    style.push_str(CATALOG_CSS);

    let toggle = if has_dark {
        format!(
            "<label class=\"ds-theme\"><input type=\"checkbox\" id=\"ds-theme\"{}> Dark</label>",
            if dark { " checked" } else { "" }
        )
    } else {
        String::new()
    };

    Ok(format!(
        "<!doctype html>\n<html lang=\"en\"{opens}>\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title} — design catalog</title>\n<style>\n{style}</style>\n</head>\n\
         <body>\n<div class=\"ds-shell\">\n{side}\n\
         <main class=\"ds-main\">\n{main}</main>\n</div>\n\
         <script>\n{script}</script>\n</body>\n</html>\n",
        opens = if dark { " data-theme=\"dark\"" } else { "" },
        title = escape(&system.meta.title),
        style = style,
        side = sidebar(system, &toggle),
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
    for component in system.components_by_name("") {
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
        "<div class=\"ds-flow\" id=\"ds-flow\">\n{flow}\
         <p class=\"ds-empty\" id=\"ds-empty\" hidden>No results</p></div>\n\
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
            format::bytes(bytes),
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
    for component in system.components_by_name("") {
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
            format!(
                "<span class=\"ds-val\">{}</span>",
                format::bytes(asset.bytes)
            )
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
            format::bytes(file.bytes),
        ));
    }
    out
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
        Some(height) => format!(
            "<span class=\"ds-dim\">{} · {height}pt</span>",
            escape(&group)
        ),
        None => format!("<span class=\"ds-dim\">{}</span>", escape(&group)),
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
fn default_path(system: &DesignSystem) -> PathBuf {
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
fn viewer() -> Option<PathBuf> {
    super::helpers::catalog()
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
const CATALOG_CSS: &str = include_str!("catalog/catalog.css");

/// Category switching, the rail slide, search filtering and the theme toggle.
/// Inline and tiny: the catalog is one file with no network.
const SCRIPT: &str = include_str!("catalog/catalog.js");

#[cfg(test)]
mod tests;
