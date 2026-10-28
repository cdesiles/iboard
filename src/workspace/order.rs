//! The order of Todo and Ready across the projects of the All projects
//! board. Each project keeps its own order in its file names (`001-`); this
//! file, beside the user configuration, only says how the projects' issues
//! interleave, so reordering across projects renames nothing.
//!
//! ```toml
//! [columns]
//! ready = ["site/ship.md", "app/export.md", "site/fix.md"]
//! ```

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::discovery::{self, Board};
use crate::files;
use crate::settings;

const FILE: &str = "all-projects-order.toml";

/// The stored interleaving, by column folder name (`ready`, `todo`), each a
/// list of [`key`]s.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Order {
    #[serde(default)]
    pub columns: BTreeMap<String, Vec<String>>,
}

/// Where the order lives: beside the user configuration.
pub fn file() -> Option<PathBuf> {
    settings::config_file().and_then(|config| Some(config.parent()?.join(FILE)))
}

impl Order {
    /// The order in `file`; empty when there is none or it cannot be read.
    pub fn load(file: Option<&Path>) -> Self {
        file.and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> Result<(), String> {
        let text = toml::to_string(self).map_err(|err| err.to_string())?;
        files::atomic_write(file, text)
            .map_err(|err| format!("Couldn't write {}: {err}", file.display()))
    }
}

/// An issue as the order names it: its project, then its file name without
/// the priority number, which changes when the project renumbers.
pub fn key(project: &str, path: &Path) -> String {
    let name = files::file_name(path);
    format!("{project}/{}", discovery::strip_priority(&name))
}

/// The issues of several projects, each list in its project's own order, as
/// one list in `stored`'s order. A project's own order always holds: an issue
/// `stored` puts before one its project ranks higher waits for it. Issues
/// `stored` does not name follow the issue before them in their project, or
/// come at the end.
pub fn merge(lanes: &[Vec<(String, PathBuf)>], stored: &[String]) -> Vec<PathBuf> {
    let known: HashSet<&str> = stored.iter().map(String::as_str).collect();
    let mut next = vec![0; lanes.len()];
    let mut merged = Vec::new();
    for wanted in stored {
        let found = lanes.iter().enumerate().find_map(|(lane, issues)| {
            let at = issues.iter().position(|(key, _)| key == wanted)?;
            (at >= next[lane]).then_some((lane, at))
        });
        let Some((lane, at)) = found else {
            continue;
        };
        let issues = &lanes[lane];
        merged.extend(issues[next[lane]..=at].iter().map(|(_, path)| path.clone()));
        next[lane] = at + 1;
        while let Some((key, path)) = issues.get(next[lane]) {
            if known.contains(key.as_str()) {
                break;
            }
            merged.push(path.clone());
            next[lane] += 1;
        }
    }
    for (lane, issues) in lanes.iter().enumerate() {
        merged.extend(issues[next[lane]..].iter().map(|(_, path)| path.clone()));
    }
    merged
}

/// Puts the All projects board's Todo and Ready in `order`'s interleaving;
/// a column it has nothing for keeps its order by priority, then project.
pub fn arrange(board: &mut Board, order: &Order) {
    for at in 0..board.columns.len() {
        let column = &board.columns[at];
        if !discovery::is_orderable_column(&column.dir) {
            continue;
        }
        let stored = order
            .columns
            .get(&files::file_name(&column.dir))
            .map_or(&[][..], Vec::as_slice);
        if stored.is_empty() {
            continue;
        }
        let lanes: Vec<Vec<(String, PathBuf)>> = board
            .repos
            .iter()
            .map(|repo| {
                column
                    .issues
                    .iter()
                    .filter(|issue| {
                        board.repo_of(&issue.path).map(|of| &of.name) == Some(&repo.name)
                    })
                    .map(|issue| (key(&repo.name, &issue.path), issue.path.clone()))
                    .collect()
            })
            .collect();
        let merged = merge(&lanes, stored);
        let column = &mut board.columns[at];
        let mut issues = std::mem::take(&mut column.issues);
        column.issues = merged
            .iter()
            .filter_map(|path| {
                let at = issues.iter().position(|issue| issue.path == *path)?;
                Some(issues.remove(at))
            })
            .collect();
        column.issues.append(&mut issues);
    }
}

/// The keys of the column `order`, to store after a drop on the All
/// projects board.
pub fn keys(board: &Board, order: &[PathBuf]) -> Vec<String> {
    order
        .iter()
        .filter_map(|path| Some(key(&board.repo_of(path)?.name, path)))
        .collect()
}

#[cfg(test)]
mod tests;
