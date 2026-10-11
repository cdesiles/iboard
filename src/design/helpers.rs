//! Finding the WebKit helpers `build.rs` compiles. The binaries embed them and
//! write them out once to iboard's cache, so an installed `iboard` or
//! `idesign` keeps them after its build folder is gone.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::{files, settings};

/// The preview renderer, `macos/render.swift`.
pub fn renderer() -> Option<PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    #[cfg(apple_render)]
    const EMBEDDED: Option<&[u8]> = Some(include_bytes!(env!("IBOARD_RENDER_HELPER")));
    #[cfg(not(apple_render))]
    const EMBEDDED: Option<&[u8]> = None;
    locate("iboard-render", "IBOARD_RENDER", EMBEDDED, &PATH)
}

/// The live catalog window, `macos/catalog.swift`.
pub fn catalog() -> Option<PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    #[cfg(apple_catalog)]
    const EMBEDDED: Option<&[u8]> = Some(include_bytes!(env!("IBOARD_CATALOG_HELPER")));
    #[cfg(not(apple_catalog))]
    const EMBEDDED: Option<&[u8]> = None;
    locate("iboard-catalog", "IBOARD_CATALOG", EMBEDDED, &PATH)
}

/// The helper `name`: the file `$override_var` names when it exists, else the
/// embedded copy, written out to iboard's cache under a name that changes
/// with its content, so a newer build never runs an older helper. `path`
/// keeps that name, so the content is hashed once.
fn locate(
    name: &str,
    override_var: &str,
    embedded: Option<&[u8]>,
    path: &OnceLock<Option<PathBuf>>,
) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(override_var).map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    let bytes = embedded?;
    let path = path
        .get_or_init(|| {
            let dir = settings::cache_dir()?.join("helpers");
            Some(dir.join(format!("{name}-{:016x}", fnv1a(bytes))))
        })
        .clone()?;
    if !path.is_file() {
        write_executable(&path, bytes).ok()?;
        prune(&path, name);
    }
    Some(path)
}

/// Removes the other builds of helper `name` beside `path` that went a day
/// without being written out: another version of iboard may still run one.
fn prune(path: &Path, name: &str) {
    let Some(entries) = path.parent().and_then(|dir| std::fs::read_dir(dir).ok()) else {
        return;
    };
    let day = std::time::Duration::from_secs(24 * 3600);
    for entry in entries.flatten() {
        let other = entry.path();
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|at| at.elapsed().is_ok_and(|age| age > day));
        if other != path
            && stale
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{name}-"))
        {
            let _ = std::fs::remove_file(other);
        }
    }
}

fn write_executable(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    files::atomic_write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// FNV-1a, enough to tell two builds of a helper apart.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_embedded_helper_is_written_out_once_and_runs() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("helpers/fake");
        write_executable(&path, b"#!/bin/sh\necho ran\n").unwrap();
        let output = std::process::Command::new(&path).output().unwrap();
        assert_eq!(output.stdout, b"ran\n");
        assert_ne!(fnv1a(b"one build"), fnv1a(b"another build"));
    }
}
