//! The files and assets that make up the system on disk.

use std::path::Path;

use super::model::{Asset, DesignFile, DesignSystem};

impl DesignSystem {
    /// Every file under `design/`, `rel`ative to the root, sorted by path.
    pub fn files(&self) -> Vec<DesignFile> {
        let mut out = Vec::new();
        collect_files(&self.root, &self.root, &mut out);
        out.sort_by(|a, b| a.rel.cmp(&b.rel));
        out
    }

    /// The assets the system ships or declares: files under the project's
    /// `assets/` and the design's own `assets/`, plus the names in
    /// `design-system.json`'s `assetGroups` that are not files.
    pub fn assets(&self) -> Vec<Asset> {
        let mut out = Vec::new();
        if let Some(parent) = self.root.parent() {
            collect_assets(&parent.join("assets"), "project", &mut out);
        }
        collect_assets(&self.root.join("assets"), "design", &mut out);
        for (group, names) in &self.asset_groups {
            for name in names {
                if !out.iter().any(|asset| &asset.name == name) {
                    out.push(Asset {
                        name: name.clone(),
                        group: group.clone(),
                        path: None,
                        bytes: 0,
                    });
                }
            }
        }
        out.sort_by(|a, b| {
            (a.group.as_str(), a.name.as_str()).cmp(&(b.group.as_str(), b.name.as_str()))
        });
        out
    }
}

/// Recursively collect the files under `dir`, relative to `root`, skipping
/// hidden entries.
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<DesignFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(DesignFile {
                rel: rel.to_string_lossy().replace('\\', "/"),
                bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            });
        }
    }
}

/// Recursively collect the files under an `assets/` folder, grouped by the
/// first folder below it (`fonts`, `images`).
fn collect_assets(dir: &Path, source: &str, out: &mut Vec<Asset>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            let group = format!("{source}/{name}");
            collect_grouped(&path, &group, out);
        } else {
            out.push(Asset {
                name: name.to_string(),
                group: source.to_string(),
                path: Some(path.clone()),
                bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            });
        }
    }
}

fn collect_grouped(dir: &Path, group: &str, out: &mut Vec<Asset>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_grouped(&path, group, out);
        } else {
            out.push(Asset {
                name: name.to_string(),
                group: group.to_string(),
                path: Some(path.clone()),
                bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            });
        }
    }
}
