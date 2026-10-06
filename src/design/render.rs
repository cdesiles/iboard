//! Rendering an HTML preview to a PNG, out of process.
//!
//! iboard never loads WebKit itself. `build.rs` compiles `macos/render.swift`
//! into a small helper that snapshots the page and exits, so the board's
//! process stays a plain egui window: instant to start, and a broken preview
//! cannot take it down. On a machine without the helper (or a non-macOS build)
//! nothing renders, and callers fall back to opening the HTML in the browser.

use std::path::{Path, PathBuf};
use std::process::Command;

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
/// finishes; callers run it off the UI thread.
pub fn snapshot(html: &Path, png: &Path, width: u32, height: u32) -> Result<PathBuf, String> {
    let helper = helper().ok_or_else(|| {
        "No HTML renderer is available (build the render helper, or set IBOARD_RENDER).".to_string()
    })?;
    if let Some(parent) = png.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Couldn't create {}: {err}", parent.display()))?;
    }
    let output = Command::new(&helper)
        .arg(html)
        .arg(png)
        .arg(width.to_string())
        .arg(height.to_string())
        .output()
        .map_err(|err| format!("Couldn't run the renderer: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("The renderer exited with {}", output.status)
        } else {
            stderr
        });
    }
    Ok(png.to_path_buf())
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
