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

    let groups = system.component_groups();
    let mut nav = String::new();
    let mut main = String::new();
    let mut index = 0;
    for (group, list) in &groups {
        nav.push_str(&format!(
            "<div class=\"ds-groupblock\" data-group=\"{}\">\n<div class=\"ds-group\">{}</div>\n",
            escape(group),
            escape(&group.to_uppercase()),
        ));
        for component in list {
            let id = format!("c-{index}");
            let search = format!(
                "{} {} {}",
                component.name,
                group,
                component.card.subtitle.as_deref().unwrap_or("")
            )
            .to_lowercase();
            nav.push_str(&format!(
                "<a href=\"#{}\" data-name=\"{}\">{}</a>\n",
                id,
                escape(&search),
                escape(&component.name),
            ));
            main.push_str(&section(component, &id, group));
            index += 1;
        }
        nav.push_str("</div>\n");
    }

    let subtitle = if groups.len() == 1 {
        format!("{} components", system.components.len())
    } else {
        format!(
            "{} components · {} groups",
            system.components.len(),
            groups.len()
        )
    };
    let change = system
        .meta
        .last_change
        .as_ref()
        .and_then(|change| change.note.clone())
        .map(|note| format!("<p class=\"ds-note\">{}</p>", escape(&note)))
        .unwrap_or_default();
    let toggle = if has_dark {
        "<label class=\"ds-theme\"><input type=\"checkbox\" id=\"ds-theme\"> Dark</label>"
    } else {
        ""
    };

    Ok(format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title} — design catalog</title>\n<style>\n{style}</style>\n</head>\n\
         <body>\n<div class=\"ds-shell\">\n\
         <aside class=\"ds-side\">\n<h1>{title}</h1>\n<div class=\"ds-ns\">{ns}</div>\n\
         <input id=\"ds-q\" class=\"ds-search\" type=\"search\" placeholder=\"Search\">\n\
         <nav class=\"ds-nav\">\n{nav}</nav>\n{toggle}\n</aside>\n\
         <main class=\"ds-main\">\n<header class=\"ds-head\"><h1>{title}</h1>\
         <p>{subtitle}</p>{change}</header>\n{main}</main>\n</div>\n\
         <script>\n{script}</script>\n</body>\n</html>\n",
        title = escape(&system.meta.title),
        ns = escape(&system.meta.namespace),
        style = style,
        nav = nav,
        toggle = toggle,
        subtitle = subtitle,
        change = change,
        main = main,
        script = SCRIPT,
    ))
}

/// One component: its title, subtitle and live fragment in a bordered stage.
fn section(component: &Component, id: &str, group: &str) -> String {
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
         <div class=\"ds-stage{page}\">{body}</div>\n</section>\n",
        id = id,
        slug = escape(&component.name.to_lowercase()),
        group = escape(&group.to_lowercase()),
        name = escape(&component.name),
        subtitle = escape(&subtitle),
        meta = meta,
        page = page,
        body = body,
    )
}

/// The body of a component's fragment, with its `@dsCard` line dropped.
fn fragment(component: &Component) -> String {
    let text = std::fs::read_to_string(component.dir.join("preview.html")).unwrap_or_default();
    let text = strip_card_comment(&text);
    body_inner(&text)
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
.ds-side{border-right:1px solid var(--line);background:var(--surface);overflow:auto;padding:16px}
.ds-side h1{font:600 15px/20px var(--font-sans);margin:0}
.ds-ns{font:11px/14px var(--font-mono);color:var(--ink-muted);margin:2px 0 12px}
.ds-search{width:100%;height:28px;padding:0 8px;border:1px solid var(--line-strong);border-radius:var(--radius-sm);background:var(--surface);color:var(--ink);font:13px/18px var(--font-sans);margin-bottom:10px}
.ds-search:focus{outline:2px solid var(--focus);outline-offset:1px}
.ds-nav a{display:block;padding:4px 6px;border-radius:var(--radius-sm);color:var(--ink-muted);text-decoration:none}
.ds-nav a:hover{background:var(--surface-hover);color:var(--ink)}
.ds-nav a[aria-current]{background:var(--accent-soft);color:var(--ink)}
.ds-group{font:600 11px/14px var(--font-sans);letter-spacing:.06em;text-transform:uppercase;color:var(--ink-faint);margin:12px 0 4px}
.ds-theme{display:flex;align-items:center;gap:6px;margin-top:16px;padding-top:12px;border-top:1px solid var(--line);color:var(--ink-muted);font-size:12px;cursor:pointer}
.ds-main{overflow:auto;padding:24px 32px 64px}
.ds-head{max-width:760px;margin-bottom:24px}
.ds-head h1{font:600 20px/26px var(--font-sans);margin:0}
.ds-head p{color:var(--ink-muted);margin:4px 0 0}
.ds-note{color:var(--ink-faint);font:11px/16px var(--font-mono);margin:8px 0 0}
.ds-comp{margin:0 0 40px;scroll-margin-top:16px;max-width:900px}
.ds-comp__head{display:flex;align-items:baseline;gap:8px;flex-wrap:wrap;margin-bottom:10px}
.ds-comp__head h2{font:600 15px/20px var(--font-sans);margin:0}
.ds-comp__head p{color:var(--ink-muted);margin:0;flex:1 1 320px}
.ds-dim{font:11px/14px var(--font-mono);color:var(--ink-faint)}
.ds-stage{border:1px solid var(--line);border-radius:var(--radius-md);background:var(--bg);overflow:auto}
.ds-stage--page{max-height:660px}
"##;

/// Search filtering and the active-section highlight. Inline and tiny: the
/// catalog is one file with no network.
const SCRIPT: &str = r##"
(function () {
  var root = document.documentElement;
  var toggle = document.getElementById('ds-theme');
  if (toggle) {
    try { if (localStorage.getItem('ds-theme') === 'dark') { root.dataset.theme = 'dark'; toggle.checked = true; } } catch (e) {}
    toggle.addEventListener('change', function () {
      root.dataset.theme = toggle.checked ? 'dark' : 'light';
      try { localStorage.setItem('ds-theme', toggle.checked ? 'dark' : 'light'); } catch (e) {}
    });
  }
  var q = document.getElementById('ds-q');
  q.addEventListener('input', function () {
    var v = q.value.trim().toLowerCase();
    document.querySelectorAll('.ds-comp').forEach(function (s) {
      s.hidden = v !== '' && s.dataset.name.indexOf(v) < 0 && s.dataset.group.indexOf(v) < 0;
    });
    document.querySelectorAll('.ds-groupblock').forEach(function (block) {
      var any = false;
      block.querySelectorAll('a').forEach(function (a) {
        var show = v === '' || a.dataset.name.indexOf(v) >= 0;
        a.hidden = !show;
        if (show) any = true;
      });
      block.hidden = !any;
    });
  });
  var links = Array.prototype.slice.call(document.querySelectorAll('.ds-nav a'));
  var observer = new IntersectionObserver(function (entries) {
    entries.forEach(function (entry) {
      if (!entry.isIntersecting) return;
      links.forEach(function (l) { l.removeAttribute('aria-current'); });
      var active = document.querySelector('.ds-nav a[href="#' + entry.target.id + '"]');
      if (active) active.setAttribute('aria-current', 'page');
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
        assert!(html.contains("CONTROLS"));
        assert!(
            html.contains("ds-stage--page"),
            "page components get a stage"
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
