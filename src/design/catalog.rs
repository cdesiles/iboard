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

use super::{Component, DesignSystem, css};

/// The catalog's categories, in Claude Design's order: `(id, label)`.
const CATEGORIES: [(&str, &str); 10] = [
    ("overview", "Overview"),
    ("colors", "Colors"),
    ("typography", "Typography"),
    ("spacing", "Spacing"),
    ("radius", "Corner radius"),
    ("shadows", "Shadows"),
    ("size", "Size"),
    ("components", "Components"),
    ("assets", "Assets"),
    ("files", "Files"),
];

/// The full catalog document for `theme` (`light`'s values become the default).
pub fn html(system: &DesignSystem, _theme: &str) -> Result<String, String> {
    let has_dark = system.tokens.themes.iter().any(|t| t.id == "dark");

    let mut style = String::new();
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
        main = main_panes(system),
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
    out.push_str(
        "<input id=\"ds-q\" class=\"ds-search\" type=\"search\" placeholder=\"Search\">\n",
    );
    out.push_str("<div class=\"ds-rails\" id=\"ds-rails\">\n");

    out.push_str("<nav class=\"ds-rail ds-rail--main\" id=\"ds-rail-main\">\n");
    for (id, label) in CATEGORIES {
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
        out.push_str(&format!(
            "<button type=\"button\" class=\"ds-item\" data-cat=\"{id}\"{drill}>\
             <span>{label}</span>{count}</button>\n",
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

/// One pane per category; only Overview starts visible, the script reveals the
/// selected one.
fn main_panes(system: &DesignSystem) -> String {
    [
        ("overview", overview(system), false),
        ("colors", colors(system), true),
        ("typography", typography(system), true),
        ("spacing", spacing(system), true),
        ("radius", radius(system), true),
        ("shadows", shadows(system), true),
        ("size", size(system), true),
        ("components", components(system), true),
        ("assets", assets(system), true),
        ("files", files(system), true),
    ]
    .into_iter()
    .map(|(id, inner, hidden)| {
        format!(
            "<section class=\"ds-cat\" data-cat=\"{id}\"{}>\n{inner}</section>\n",
            if hidden { " hidden" } else { "" }
        )
    })
    .collect()
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

fn typography(system: &DesignSystem) -> String {
    let mut out = head(
        "Typography",
        &format!("{} styles", system.tokens.type_style_count()),
    );
    for group in &system.tokens.type_groups {
        out.push_str(&format!(
            "<h3 class=\"ds-sub\">{}</h3>\n",
            escape(&group.name)
        ));
        let family = system
            .tokens
            .families
            .get(&group.family)
            .cloned()
            .unwrap_or_default();
        if !family.is_empty() {
            out.push_str(&format!("<p class=\"ds-usage\">{}</p>\n", escape(&family)));
        }
        for style in &group.styles {
            let size = style.font_size.clone().unwrap_or_else(|| "13px".into());
            let weight = style
                .weight
                .map(|w| w.to_string())
                .unwrap_or_else(|| "400".into());
            let line = style.line_height.clone().unwrap_or_else(|| "normal".into());
            let sample = style.sample.clone().unwrap_or_else(|| style.name.clone());
            out.push_str(&format!(
                "<div class=\"ds-type\">\
                 <div class=\"ds-type__sample\" style=\"font-family:{family};font-size:{size};\
                 font-weight:{weight};line-height:{line}\">{sample}</div>\
                 <div class=\"ds-usage\">{name} · {size} · {weight} · line {line}</div></div>\n",
                family = escape(&family),
                size = escape(&size),
                weight = weight,
                line = escape(&line),
                sample = escape(&sample),
                name = escape(&style.name),
            ));
        }
    }
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
.ds-search{width:100%;height:28px;padding:0 8px;border:1px solid var(--line-strong);border-radius:var(--radius-sm);background:var(--surface);color:var(--ink);font:13px/18px var(--font-sans);margin-bottom:10px;flex:0 0 auto}
.ds-search:focus{outline:2px solid var(--focus);outline-offset:1px}
.ds-rails{position:relative;flex:1 1 auto;min-height:0;overflow:hidden}
.ds-rail{position:absolute;inset:0;overflow:auto;transition:transform .22s ease}
.ds-rail--main{transform:translateX(0)}
.ds-rail--sub{transform:translateX(100%)}
.ds-rails.ds-open .ds-rail--main{transform:translateX(-100%)}
.ds-rails.ds-open .ds-rail--sub{transform:translateX(0)}
.ds-item{display:flex;align-items:center;justify-content:space-between;gap:8px;width:100%;text-align:left;background:none;border:0;padding:6px 8px;border-radius:var(--radius-sm);color:var(--ink-muted);font:13px/18px var(--font-sans);cursor:pointer}
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
.ds-main{overflow:auto;padding:24px 32px 64px}
.ds-cat[hidden]{display:none}
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
.ds-type{margin:0 0 14px}
.ds-type__sample{color:var(--ink);overflow-wrap:anywhere}
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

  function select(id) {
    document.querySelectorAll('.ds-cat').forEach(function (s) {
      s.hidden = s.dataset.cat !== id;
    });
    items.forEach(function (b) {
      if (b.dataset.cat === id) { b.setAttribute('aria-current', 'page'); }
      else { b.removeAttribute('aria-current'); }
    });
    document.querySelector('.ds-main').scrollTop = 0;
  }
  function open(drill) { rails.classList.toggle('ds-open', !!drill); }

  items.forEach(function (b) {
    b.addEventListener('click', function () { select(b.dataset.cat); open(b.dataset.drill === '1'); });
  });
  var back = document.querySelector('.ds-back');
  if (back) { back.addEventListener('click', function () { open(false); }); }
  subs.forEach(function (a) {
    a.addEventListener('click', function () { select('components'); open(true); });
  });

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
        assert!(html.contains("class=\"ds-back\""), "a back button");
        assert!(
            html.contains("href=\"#c-button\""),
            "the second level links a component"
        );
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
