//! The board's vocabulary: the columns an issue moves through, the issues in
//! them, and the projects a workspace board merges.
//!
//! Scanning an issues folder into these columns lives in the Board domain.

use std::fs;
use std::path::{Path, PathBuf};

use super::issue::Issue;
use crate::files;

/// What a known column folder is for, in board order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Backlog,
    Todo,
    Ready,
    Doing,
    Review,
    Later,
    Blocked,
    Done,
}

impl ColumnKind {
    /// The kind of the folder `dir`, under any of its names: `wip/` is Doing.
    pub fn of(dir: &Path) -> Option<Self> {
        let name = dir.file_name()?.to_str()?;
        match name.to_ascii_lowercase().as_str() {
            "backlog" | "icebox" => Some(Self::Backlog),
            "todo" | "to-do" | "to_do" => Some(Self::Todo),
            "ready" => Some(Self::Ready),
            "doing" | "in-progress" | "in_progress" | "inprogress" | "wip" => Some(Self::Doing),
            "review" => Some(Self::Review),
            "later" => Some(Self::Later),
            "blocked" => Some(Self::Blocked),
            "done" | "completed" => Some(Self::Done),
            _ => None,
        }
    }

    /// The title of its column.
    pub fn name(self) -> &'static str {
        match self {
            Self::Backlog => "Backlog",
            Self::Todo => "Todo",
            Self::Ready => "Ready",
            Self::Doing => "Doing",
            Self::Review => "Review",
            Self::Later => "Later",
            Self::Blocked => "Blocked",
            Self::Done => "Done",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub name: String,
    /// The directory holding this column's issues. Dropping a card here moves
    /// its markdown file into this directory.
    pub dir: PathBuf,
    /// Its folder's kind: none for an unknown folder or the files at the
    /// issues root.
    pub kind: Option<ColumnKind>,
    pub issues: Vec<Issue>,
}

impl Column {
    /// Whether the "+" / new-issue affordance is offered here. New issues
    /// start life in Todo, or in the root files column when it is named so.
    pub fn accepts_new_issues(&self) -> bool {
        match self.kind {
            Some(kind) => kind == ColumnKind::Todo,
            None => self.name.eq_ignore_ascii_case(ColumnKind::Todo.name()),
        }
    }

    /// Whether this is the Done column.
    pub fn is_done(&self) -> bool {
        is_done_column(&self.dir)
    }
}

/// One project of a workspace board.
#[derive(Clone, Debug, PartialEq)]
pub struct Repo {
    /// Its folder's name, the `repo:` label of its issues.
    pub name: String,
    /// The workspace group it is listed in, the `group:` label.
    pub group: String,
    pub project_dir: PathBuf,
    pub root: PathBuf,
    /// Its own column folders, the issues root included, each with the
    /// merged column it joins (the folder name of that column).
    pub dirs: Vec<(String, PathBuf)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub project_dir: PathBuf,
    pub root: PathBuf,
    pub title: Option<String>,
    /// The `##` sections the reader asks for, from `[reader] sections`.
    pub reader_sections: Vec<String>,
    pub columns: Vec<Column>,
    /// Non-fatal problems encountered while scanning (unreadable files, ...).
    pub warnings: Vec<String>,
    /// The projects of a workspace board, whose columns merge theirs; empty
    /// for a single project.
    pub repos: Vec<Repo>,
}

impl Board {
    /// The column holding the issue at `path`.
    pub fn column_of(&self, path: &Path) -> Option<&Column> {
        self.columns
            .iter()
            .find(|column| column.issues.iter().any(|issue| issue.path == path))
    }

    /// The column new issues are written to.
    pub fn todo_column(&self) -> Option<&Column> {
        self.columns
            .iter()
            .find(|column| column.accepts_new_issues())
    }

    /// What the project is called: its configured title, else its folder's
    /// name.
    pub fn name(&self) -> String {
        project_name(self.title.as_deref(), &self.project_dir)
    }

    /// A board with no columns yet.
    pub fn new(
        project_dir: &Path,
        root: &Path,
        title: Option<String>,
        reader_sections: Vec<String>,
    ) -> Self {
        Self {
            project_dir: project_dir.to_path_buf(),
            root: root.to_path_buf(),
            title,
            reader_sections,
            columns: Vec::new(),
            warnings: Vec::new(),
            repos: Vec::new(),
        }
    }

    /// The issue at `path`.
    pub fn find(&self, path: &Path) -> Option<&Issue> {
        self.columns
            .iter()
            .flat_map(|column| column.issues.iter())
            .find(|issue| issue.path == path)
    }

    pub fn issue_count(&self) -> usize {
        self.columns.iter().map(|c| c.issues.len()).sum()
    }

    /// The workspace project holding `path`; `None` on a single project.
    pub fn repo_of(&self, path: &Path) -> Option<&Repo> {
        self.repos
            .iter()
            .filter(|repo| path.starts_with(&repo.root))
            .max_by_key(|repo| repo.root.components().count())
    }

    /// The project the issue at `path` belongs to: where its agents run.
    pub fn project_of(&self, path: &Path) -> &Path {
        self.repo_of(path)
            .map_or(&self.project_dir, |repo| &repo.project_dir)
    }

    /// The issues folder the issue at `path` belongs to: where its sidecars
    /// (reviews, comments, build logs, images) live.
    pub fn root_of(&self, path: &Path) -> &Path {
        self.repo_of(path).map_or(&self.root, |repo| &repo.root)
    }

    /// Where the issue at `path` goes when dropped on the column `dir`: `dir`
    /// itself on a single project, else that column's folder in the issue's
    /// own project (see [`Self::dir_in`]).
    pub fn dir_for(&self, path: &Path, dir: &Path) -> Result<PathBuf, String> {
        match self.repo_of(path) {
            Some(repo) => dir_in(repo, dir),
            None => Ok(dir.to_path_buf()),
        }
    }

    /// Whether the issue at `path` already sits in the column `dir`.
    pub fn in_column(&self, path: &Path, dir: &Path) -> bool {
        match self.repo_of(path) {
            Some(repo) => {
                let kind = files::file_name(dir);
                repo.dirs
                    .iter()
                    .any(|(of, folder)| *of == kind && path.parent() == Some(folder.as_path()))
            }
            None => path.parent() == Some(dir),
        }
    }
}

/// The folder of `repo` behind the merged column `dir`. A missing Todo,
/// Ready, Doing or Done is created; another column the project lacks is an
/// error.
pub fn dir_in(repo: &Repo, dir: &Path) -> Result<PathBuf, String> {
    let kind = files::file_name(dir);
    if let Some((_, folder)) = repo.dirs.iter().find(|(of, _)| *of == kind) {
        return Ok(folder.clone());
    }
    if !INIT_COLUMNS.contains(&kind.as_str()) {
        let column = display_name(dir);
        return Err(format!("{} has no {column} column", repo.name));
    }
    let folder = repo.root.join(&kind);
    fs::create_dir_all(&folder)
        .map_err(|err| format!("Couldn't create {}: {err}", folder.display()))?;
    Ok(folder)
}

pub(crate) fn display_name(dir: &Path) -> String {
    let raw = dir.file_name().and_then(|n| n.to_str()).unwrap_or("Column");
    match ColumnKind::of(dir) {
        Some(kind) => kind.name().to_string(),
        None => title_case(raw),
    }
}

fn title_case(raw: &str) -> String {
    raw.split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The columns a fresh project is created with: `todo → ready → doing → done`.
pub const INIT_COLUMNS: [&str; 4] = ["todo", "ready", "doing", "done"];

/// What a project is called: its configured title, else its folder's name.
pub(crate) fn project_name(title: Option<&str>, project_dir: &Path) -> String {
    title
        .map(str::to_string)
        .or_else(|| {
            project_dir
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "iboard".to_string())
}

/// Whether `dir` is the Ready column, whose issues carry a priority prefix.
pub fn is_ready_column(dir: &Path) -> bool {
    ColumnKind::of(dir) == Some(ColumnKind::Ready)
}

/// Whether `dir` is the Doing column, where the AI build's agent works.
pub fn is_doing_column(dir: &Path) -> bool {
    ColumnKind::of(dir) == Some(ColumnKind::Doing)
}

/// Whether `dir` is the Done column.
pub fn is_done_column(dir: &Path) -> bool {
    ColumnKind::of(dir) == Some(ColumnKind::Done)
}

/// Whether a column's order is its file-name order, so dropping a card at a
/// position can renumber the files. Todo and Ready; Doing and Done are sorted
/// by date instead.
pub fn is_orderable_column(dir: &Path) -> bool {
    matches!(
        ColumnKind::of(dir),
        Some(ColumnKind::Todo | ColumnKind::Ready)
    )
}

/// Whether a column's issues can be edited on the board. Todo and Ready are
/// the working columns; Doing and Done are the agent's and the finished ones.
pub fn is_editable_column(dir: &Path) -> bool {
    !matches!(
        ColumnKind::of(dir),
        Some(ColumnKind::Doing | ColumnKind::Review | ColumnKind::Done)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_titles() {
        assert_eq!(display_name(Path::new("/x/in-progress")), "Doing");
        assert_eq!(display_name(Path::new("/x/scratch-notes")), "Scratch Notes");
    }

    #[test]
    fn orderable_columns() {
        assert!(is_orderable_column(Path::new("/x/todo")));
        assert!(is_orderable_column(Path::new("/x/ready")));
        assert!(!is_orderable_column(Path::new("/x/doing")));
        assert!(!is_orderable_column(Path::new("/x/done")));
    }
}
