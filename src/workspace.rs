//! A workspace: the boards of several projects seen as one. The user
//! configuration lists the projects by group:
//!
//! ```toml
//! [[workspace.group]]
//! name = "Client"
//! repos = ["~/code/app", "~/code/site"]
//! ```
//!
//! Their columns merge by kind (every Doing in one Doing), and each issue
//! carries `repo:<folder>` and `group:<name>` labels, so the board's filter
//! narrows it to one project or one group.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::discovery::{self, Board, Column, Repo};
use crate::files;
use crate::issue::Issue;
use crate::labels;
use crate::settings::{self, UserConfig};

pub mod order;

pub use order::Order;

/// What the workspace board is called, in the project menu and the title.
pub const TITLE: &str = "All projects";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub repos: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub groups: Vec<Group>,
}

#[derive(Default, Deserialize)]
struct RawWorkspace {
    #[serde(default)]
    group: Vec<RawGroup>,
}

#[derive(Deserialize)]
struct RawGroup {
    name: String,
    #[serde(default)]
    repos: Vec<String>,
}

/// The `[[workspace.group]]` tables of the user configuration, in their
/// order; `None` when there is none.
pub fn load(config: &mut UserConfig) -> Option<Workspace> {
    let raw: RawWorkspace = config.section("workspace");
    let groups: Vec<Group> = raw
        .group
        .into_iter()
        .map(|group| Group {
            name: group.name,
            repos: group.repos.iter().map(|repo| expand(repo)).collect(),
        })
        .collect();
    (!groups.is_empty()).then_some(Workspace { groups })
}

fn expand(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), settings::home()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

/// Where the merged columns pretend to live. Nothing is ever written there:
/// a move resolves to the column of the card's own project.
pub fn virtual_root() -> PathBuf {
    settings::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("all-projects")
}

/// The merged board of every project in `workspace`, its columns under
/// `virtual_root`, Todo and Ready interleaved as `order` says. A project that
/// cannot be read is left out, with a warning.
pub fn build(workspace: &Workspace, virtual_root: &Path, order: &Order) -> Board {
    let mut board = Board {
        project_dir: virtual_root.to_path_buf(),
        root: virtual_root.to_path_buf(),
        title: Some(TITLE.to_string()),
        reader_sections: discovery::reader_sections(None),
        columns: Vec::new(),
        warnings: Vec::new(),
        repos: Vec::new(),
    };
    for group in &workspace.groups {
        for dir in &group.repos {
            match open(dir) {
                Ok(project) if board.repos.iter().any(|r| r.root == project.root) => {}
                Ok(project) => absorb(&mut board, project, &group.name),
                Err(err) => board
                    .warnings
                    .push(format!("{}: {err:#}, skipped", dir.display())),
            }
        }
    }
    board
        .columns
        .sort_by_key(|column| discovery::column_rank(&column.dir));
    let repos = board.repos.clone();
    for column in &mut board.columns {
        sort(column, &repos);
    }
    order::arrange(&mut board, order);
    board
}

/// The board of the project at `dir`, whose `issues/` must be its own: an
/// `issues/` found further up belongs to another project.
fn open(dir: &Path) -> Result<Board> {
    let Ok(dir) = std::fs::canonicalize(dir) else {
        bail!("not found");
    };
    match discovery::locate(&dir) {
        Ok(located) if located.project_dir == dir => discovery::build(&located),
        _ => bail!("no issues/ folder"),
    }
}

/// Adds `project`'s columns and issues to the merged `board`.
fn absorb(board: &mut Board, project: Board, group: &str) {
    let name = files::file_name(&project.project_dir);
    let tags: Vec<String> = [format!("repo:{name}"), format!("group:{group}")]
        .iter()
        .filter_map(|tag| labels::normalize(tag))
        .collect();
    let mut repo = Repo {
        name: name.clone(),
        group: group.to_string(),
        project_dir: project.project_dir.clone(),
        root: project.root.clone(),
        dirs: Vec::new(),
    };
    for column in project.columns {
        let kind = kind(&column, &project.root);
        let dir = board.root.join(&kind);
        repo.dirs.push((kind, column.dir.clone()));
        let at = match board.columns.iter().position(|merged| merged.dir == dir) {
            Some(at) => at,
            None => {
                board.columns.push(Column {
                    name: column.name.clone(),
                    dir,
                    issues: Vec::new(),
                });
                board.columns.len() - 1
            }
        };
        board.columns[at]
            .issues
            .extend(column.issues.into_iter().map(|mut issue| {
                issue.labels.extend(tags.iter().cloned());
                issue
            }));
    }
    board.repos.push(repo);
    let warnings = project.warnings.into_iter();
    board
        .warnings
        .extend(warnings.map(|warning| format!("{name}: {warning}")));
}

/// The merged column `column` joins: its canonical name (`wip/` is Doing),
/// the root column's name for loose files, else its folder's name.
fn kind(column: &Column, root: &Path) -> String {
    let name = if column.dir == root {
        column.name.clone()
    } else {
        discovery::canonical_name(&column.dir)
            .map(str::to_string)
            .unwrap_or_else(|| files::file_name(&column.dir))
    };
    name.to_lowercase()
}

/// Puts a merged column in order: by priority then project in Todo and
/// Ready, newest first in Doing and Done, as each project had it elsewhere.
fn sort(column: &mut Column, repos: &[Repo]) {
    let rank = |issue: &Issue| {
        repos
            .iter()
            .position(|repo| issue.path.starts_with(&repo.root))
            .unwrap_or(usize::MAX)
    };
    if discovery::is_orderable_column(&column.dir) {
        column.issues.sort_by_key(|issue| {
            let priority = discovery::priority(&files::file_name(&issue.path));
            (priority.unwrap_or(u32::MAX), rank(issue))
        });
    } else if discovery::is_doing_column(&column.dir) || discovery::is_done_column(&column.dir) {
        column.issues.sort_by(|a, b| {
            let done = b.done_at.cmp(&a.done_at);
            done.then_with(|| b.modified.cmp(&a.modified))
        });
    }
}

#[cfg(test)]
mod tests;
