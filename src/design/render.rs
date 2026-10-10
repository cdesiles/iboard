//! Rendering an HTML preview to a PNG, out of process.
//!
//! iboard never loads WebKit itself. `build.rs` compiles `macos/render.swift`
//! into a small helper that snapshots the page and exits, so the board's
//! process stays a plain egui window: instant to start, and a broken preview
//! cannot take it down. On a machine without the helper (or a non-macOS build)
//! nothing renders, and the Design panel says so in place of each preview.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::DesignSystem;
use crate::job::{Cancel, Job};
use crate::process;

/// The default snapshot size when a component's card declares no height.
const DEFAULT_WIDTH: u32 = 900;
const DEFAULT_HEIGHT: u32 = 640;

/// How long a render may take; the helper gives up on a page by itself well
/// before.
const RENDER_TIMEOUT: Duration = Duration::from_secs(60);

/// The renderer helper's path: the `IBOARD_RENDER` override when it points at
/// a file, else the helper `build.rs` compiled.
pub fn helper() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("IBOARD_RENDER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    build_helper()
}

#[cfg(apple_render)]
fn build_helper() -> Option<PathBuf> {
    let path = PathBuf::from(env!("IBOARD_RENDER_HELPER"));
    path.is_file().then_some(path)
}

#[cfg(not(apple_render))]
fn build_helper() -> Option<PathBuf> {
    None
}

/// Whether a renderer is available on this machine.
pub fn available() -> bool {
    helper().is_some()
}

/// Render `html` to a PNG at `png`, `width`×`height`. Blocks until the helper
/// finishes.
pub fn snapshot(html: &Path, png: &Path, width: u32, height: u32) -> Result<PathBuf, String> {
    render(html, png, width, height, &Cancel::default())
}

/// [`snapshot`] on a background thread, so the UI stays responsive while
/// WebKit runs. Dropping the job stops the helper.
pub fn start(html: &Path, png: &Path, width: u32, height: u32) -> Job<Result<PathBuf, String>> {
    let (html, png) = (html.to_path_buf(), png.to_path_buf());
    Job::spawn(move |cancel| render(&html, &png, width, height, cancel))
}

fn render(
    html: &Path,
    png: &Path,
    width: u32,
    height: u32,
    cancel: &Cancel,
) -> Result<PathBuf, String> {
    let helper = helper().ok_or_else(|| {
        "No HTML renderer is available (build the render helper, or set IBOARD_RENDER).".to_string()
    })?;
    if let Some(parent) = png.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Couldn't create {}: {err}", parent.display()))?;
    }
    let mut command = Command::new(&helper);
    command
        .arg(html)
        .arg(png)
        .arg(width.to_string())
        .arg(height.to_string());
    let what = "the renderer";
    process::run(command, what, cancel, RENDER_TIMEOUT)?.into_result(what)?;
    Ok(png.to_path_buf())
}

/// Compose a component's preview and render it on a background thread. The
/// job yields the PNG path, or why the render failed.
pub fn component_preview(
    system: &DesignSystem,
    name: &str,
) -> Result<Job<Result<PathBuf, String>>, String> {
    let Some(component) = system
        .components
        .iter()
        .find(|component| component.name == name)
    else {
        return Err(format!("no component named `{name}`"));
    };
    let html = super::css::preview_html(system, component, "light")?;
    let html_path = std::env::temp_dir().join(format!("iboard-preview-{name}.html"));
    std::fs::write(&html_path, html)
        .map_err(|err| format!("Couldn't write {}: {err}", html_path.display()))?;
    let png = std::env::temp_dir().join(format!("iboard-preview-{name}.png"));
    // Snapshot at the height the card declares, so a 56pt checkbox is not a
    // full 640pt frame of empty space.
    let height = component
        .card
        .height
        .unwrap_or(DEFAULT_HEIGHT)
        .clamp(56, 2400);
    Ok(start(&html_path, &png, DEFAULT_WIDTH, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end through the helper, when this machine built one.
    #[test]
    fn renders_a_png_when_a_helper_is_available() {
        if !available() {
            return;
        }
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("idesign-render-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let html = dir.join("preview.html");
        std::fs::write(
            &html,
            "<!doctype html><html><body style=\"margin:0;background:#102030\"></body></html>",
        )
        .expect("html");
        let png = dir.join("out.png");

        let result = snapshot(&html, &png, 240, 160);
        assert!(result.is_ok(), "snapshot failed: {result:?}");
        assert!(png.is_file(), "no PNG written");

        std::fs::remove_dir_all(&dir).ok();
    }
}
