//! Locating the issues directory and turning its folders into kanban columns.
//!
//! Because layouts differ between projects, discovery is deliberately lenient:
//! any known column name (`todo`, `doing`, `done`, ...) is always shown, any
//! other sub-directory is shown only if it directly contains markdown files,
//! and markdown files sitting directly in the issues root form the first
//! column. A `iboard.toml` can override all of this.

use std::fs;
use std::path::{Path, PathBuf};

use crate::build_log;
use crate::config::Config;
use crate::files;
use crate::history;
use crate::issue::Issue;

/// A resolved project: where the issues live and how it was configured.
#[derive(Clone, Debug)]
pub struct Located {
    pub project_dir: PathBuf,
    pub root: PathBuf,
    pub config: Option<Config>,
}

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
#[derive(Clone, Debug)]
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

    /// An empty board: what [`build`] fills, and what the UI renders when a
    /// scan fails.
    pub fn empty(located: &Located) -> Self {
        let config = located.config.as_ref();
        let title = config.and_then(|c| c.title.clone());
        Self::new(
            &located.project_dir,
            &located.root,
            title,
            reader_sections(config),
        )
    }

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

/// Walks up from `start` looking for a configured project or an `issues/`
/// directory. Running from inside the issues folder (or any sub-folder) also works.
pub fn locate(start: &Path) -> Result<Located, String> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    match find_project(start) {
        Some(located) if located.config.is_some() && !located.root.is_dir() => Err(format!(
            "config {} points to `{}`, which is not a directory",
            located.project_dir.join("iboard.toml").display(),
            located.root.display()
        )),
        Some(located) => Ok(located),
        None => Err(format!(
            "no `issues/` directory found from `{}` (and no parent contains one)",
            start.display()
        )),
    }
}

/// The project found walking up from `start`: the first folder with an
/// `iboard.toml` (whose root may be missing) or an `issues/` inside, or that
/// is an `issues/` itself.
fn find_project(start: &Path) -> Option<Located> {
    start.ancestors().find_map(|dir| {
        if let Some(loaded) = crate::config::load(dir) {
            let root = match loaded.config.root.as_deref() {
                Some(relative) => dir.join(relative),
                None => dir.join("issues"),
            };
            return Some(Located {
                project_dir: dir.to_path_buf(),
                root,
                config: Some(loaded.config),
            });
        }

        let candidate = dir.join("issues");
        if candidate.is_dir() {
            return Some(Located {
                project_dir: dir.to_path_buf(),
                root: candidate,
                config: None,
            });
        }

        if dir.file_name().and_then(|n| n.to_str()) == Some("issues") && dir.is_dir() {
            let project_dir = dir.parent().unwrap_or(dir).to_path_buf();
            return Some(Located {
                project_dir,
                root: dir.to_path_buf(),
                config: None,
            });
        }
        None
    })
}

/// The columns a fresh project is created with: `todo → ready → doing → done`.
pub const INIT_COLUMNS: [&str; 4] = ["todo", "ready", "doing", "done"];

/// Creates the standard `issues/` layout for `start` and returns the resolved
/// project, ready to open. It resolves the project like [`locate`], so a config
/// file or an `issues/` directory found up the tree is used instead of a new
/// project being started below it; a path that does not exist yet is created as
/// a fresh project. Existing files and directories are never touched, so
/// running it again is safe.
pub fn init(start: &Path) -> Result<Located, String> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    let found = if start.is_dir() {
        find_project(start)
    } else {
        None
    };
    // Nothing to join: start a brand new project here.
    let located = found.unwrap_or_else(|| Located {
        project_dir: start.to_path_buf(),
        root: start.join("issues"),
        config: None,
    });
    create_columns(&located.root)?;
    Ok(located)
}

/// Creates the issues root and one directory per standard column. Each column
/// gets a `.gitkeep`, because git does not track empty directories and the
/// layout is meant to survive a clone.
fn create_columns(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root)
        .map_err(|err| format!("cannot create issues directory {}: {err}", root.display()))?;
    for name in INIT_COLUMNS {
        let dir = root.join(name);
        fs::create_dir_all(&dir)
            .map_err(|err| format!("cannot create column {}: {err}", dir.display()))?;
        let keep = dir.join(".gitkeep");
        if !keep.exists() {
            fs::File::create(&keep)
                .map_err(|err| format!("cannot create {}: {err}", keep.display()))?;
        }
    }
    Ok(())
}

/// The window title for a resolved project: the configured title, else the
/// project folder name, else `iboard`.
pub fn window_title(located: &Located) -> String {
    let title = located
        .config
        .as_ref()
        .and_then(|config| config.title.as_deref());
    project_name(title, &located.project_dir)
}

/// What a project is called: its configured title, else its folder's name.
fn project_name(title: Option<&str>, project_dir: &Path) -> String {
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

/// Scans the issues directory into columns.
pub fn build(located: &Located) -> Result<Board, String> {
    let root = &located.root;
    let config = located.config.as_ref();
    let mut warnings = Vec::new();
    let dirs = column_dirs(root, config, &mut warnings)?;
    let mut columns = Vec::new();
    columns.extend(root_column(root, config, &dirs, &mut warnings));
    for dir in &dirs {
        columns.push(folder_column(root, dir, &mut warnings));
    }
    Ok(Board {
        columns,
        warnings,
        ..Board::empty(located)
    })
}

/// The ordered sub-directory columns: the configured ones, else the known
/// columns and the folders holding markdown.
fn column_dirs(
    root: &Path,
    config: Option<&Config>,
    warnings: &mut Vec<String>,
) -> Result<Vec<PathBuf>, String> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(columns) = config.and_then(|c| c.columns.as_ref()) {
        for name in columns {
            let dir = root.join(name);
            if dir.is_dir() {
                dirs.push(dir);
            } else {
                warnings.push(format!(
                    "configured column `{name}` not found at {}",
                    dir.display()
                ));
            }
        }
    }

    if dirs.is_empty() {
        let entries = fs::read_dir(root)
            .map_err(|err| format!("cannot read issues directory {}: {err}", root.display()))?;
        let mut subs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && !is_hidden(path))
            .filter(|path| ColumnKind::of(path).is_some() || contains_markdown(path))
            .collect();
        subs.sort_by(|a, b| {
            column_rank(a).cmp(&column_rank(b)).then_with(|| {
                display_name(a)
                    .to_lowercase()
                    .cmp(&display_name(b).to_lowercase())
            })
        });
        dirs = subs;
    }
    Ok(dirs)
}

/// Files sitting directly in the issues root form the first column.
fn root_column(
    root: &Path,
    config: Option<&Config>,
    dirs: &[PathBuf],
    warnings: &mut Vec<String>,
) -> Option<Column> {
    let root_files = collect_markdown(root, warnings);
    if root_files.is_empty() {
        return None;
    }
    let has_todo_dir = dirs
        .iter()
        .any(|dir| ColumnKind::of(dir) == Some(ColumnKind::Todo));
    let name = config
        .and_then(|c| c.root_column_name.clone())
        .unwrap_or_else(|| {
            if has_todo_dir {
                ColumnKind::Backlog.name().to_string()
            } else {
                ColumnKind::Todo.name().to_string()
            }
        });
    Some(Column {
        name,
        dir: root.to_path_buf(),
        kind: None,
        issues: load_issues(root_files, warnings),
    })
}

/// The column of the sub-directory `dir`.
fn folder_column(root: &Path, dir: &Path, warnings: &mut Vec<String>) -> Column {
    let files = collect_markdown(dir, warnings);
    let mut issues = load_issues(files, warnings);
    let kind = ColumnKind::of(dir);
    match kind {
        Some(ColumnKind::Done) => {
            sort_recent_first(dir, &mut issues, true);
            for issue in &mut issues {
                let log = build_log::load(&build_log::path(root, &issue.path));
                issue.built_in = log.as_deref().and_then(build_log::took);
            }
        }
        Some(ColumnKind::Doing) => sort_recent_first(dir, &mut issues, false),
        _ => {}
    }
    Column {
        name: display_name(dir),
        dir: dir.to_path_buf(),
        kind,
        issues,
    }
}

/// The configured reader sections, or the standard four.
pub(crate) fn reader_sections(config: Option<&Config>) -> Vec<String> {
    config
        .and_then(|c| c.reader.as_ref())
        .and_then(|r| r.sections.clone())
        .unwrap_or_else(|| {
            crate::sheet::DEFAULT_SECTIONS
                .iter()
                .map(|s| s.to_string())
                .collect()
        })
}

/// Puts the most recently edited issues first (Doing and Done), and with
/// `show_date` keeps the date on each card. Issues with the same date (one
/// commit) go in reverse file-name order, so numbered issues stay newest
/// first too.
fn sort_recent_first(dir: &Path, issues: &mut [Issue], show_date: bool) {
    let paths: Vec<PathBuf> = issues.iter().map(|t| t.path.clone()).collect();
    let dates = history::done_dates(dir, &paths);
    issues.reverse();
    issues.sort_by_key(|t| std::cmp::Reverse(dates.get(&t.path).copied()));
    if show_date {
        for issue in issues.iter_mut() {
            issue.done_at = dates.get(&issue.path).copied();
        }
    }
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

fn load_issues(files: Vec<PathBuf>, warnings: &mut Vec<String>) -> Vec<Issue> {
    let mut issues = Vec::with_capacity(files.len());
    for path in files {
        match Issue::load(&path) {
            Ok(issue) => issues.push(issue),
            Err(err) => warnings.push(format!("cannot read {}: {err}", path.display())),
        }
    }
    issues
}

fn collect_markdown(dir: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            warnings.push(format!("cannot read {}: {err}", dir.display()));
            return Vec::new();
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && !is_hidden(path) && is_markdown(path))
        .collect();
    files.sort_by(|a, b| natural_cmp(&files::file_name(a), &files::file_name(b)));
    files
}

fn display_name(dir: &Path) -> String {
    let raw = dir.file_name().and_then(|n| n.to_str()).unwrap_or("Column");
    match ColumnKind::of(dir) {
        Some(kind) => kind.name().to_string(),
        None => title_case(raw),
    }
}

pub(crate) fn column_rank(dir: &Path) -> u8 {
    ColumnKind::of(dir).map_or(u8::MAX, |kind| kind as u8)
}

fn contains_markdown(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .any(|entry| entry.path().is_file() && is_markdown(&entry.path()))
        })
        .unwrap_or(false)
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
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

/// Natural ordering so `2-foo` sorts before `10-bar`.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let mut an = String::new();
                while let Some(c) = ai.peek().copied() {
                    if !c.is_ascii_digit() {
                        break;
                    }
                    an.push(c);
                    ai.next();
                }
                let mut bn = String::new();
                while let Some(c) = bi.peek().copied() {
                    if !c.is_ascii_digit() {
                        break;
                    }
                    bn.push(c);
                    bi.next();
                }
                let av: u128 = an.parse().unwrap_or(0);
                let bv: u128 = bn.parse().unwrap_or(0);
                match av.cmp(&bv) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            (Some(ac), Some(bc)) => match ac.to_ascii_lowercase().cmp(&bc.to_ascii_lowercase()) {
                Ordering::Equal => {
                    ai.next();
                    bi.next();
                }
                other => return other,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(board: &Board) -> Vec<String> {
        board
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect()
    }

    #[test]
    fn locate_works_from_the_project_the_issues_a_column_or_a_file() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        fs::create_dir_all(dir.join("issues/todo")).unwrap();
        let file = dir.join("issues/todo/a.md");
        fs::write(&file, "# A\n").unwrap();
        for start in [
            dir.to_path_buf(),
            dir.join("issues"),
            dir.join("issues/todo"),
            file,
        ] {
            let located = locate(&start).unwrap();
            assert_eq!(located.root, dir.join("issues"), "from {}", start.display());
            assert_eq!(located.project_dir, dir);
        }
    }

    #[test]
    fn a_configured_root_is_followed_and_must_exist() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        fs::create_dir_all(dir.join("tickets/todo")).unwrap();
        fs::write(
            dir.join("iboard.toml"),
            "root = \"tickets\"\ntitle = \"Moon\"\n",
        )
        .unwrap();
        let located = locate(dir).unwrap();
        assert_eq!(located.root, dir.join("tickets"));
        assert_eq!(build(&located).unwrap().title.as_deref(), Some("Moon"));

        fs::write(dir.join("iboard.toml"), "root = \"nowhere\"\n").unwrap();
        let err = locate(dir).unwrap_err().to_string();
        assert!(err.contains("nowhere"), "{err}");
    }

    #[test]
    fn a_folder_with_no_issues_above_it_is_an_error() {
        let scratch = tempfile::tempdir().unwrap();
        assert!(locate(scratch.path()).is_err());
    }

    #[test]
    fn configured_columns_keep_their_order_and_name_the_missing_ones() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        for column in ["done", "todo"] {
            fs::create_dir_all(dir.join("issues").join(column)).unwrap();
        }
        fs::write(
            dir.join("iboard.toml"),
            "columns = [\"done\", \"wip\", \"todo\"]\n",
        )
        .unwrap();
        let board = build(&locate(dir).unwrap()).unwrap();
        assert_eq!(names(&board), ["Done", "Todo"]);
        assert_eq!(board.warnings.len(), 1);
        assert!(board.warnings[0].contains("wip"), "{:?}", board.warnings);
    }

    #[test]
    fn files_in_the_issues_root_are_todo_or_the_backlog() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        fs::create_dir_all(dir.join("issues/doing")).unwrap();
        fs::write(dir.join("issues/loose.md"), "# Loose\n").unwrap();
        let board = build(&locate(dir).unwrap()).unwrap();
        assert_eq!(names(&board), ["Todo", "Doing"]);
        assert!(board.columns[0].accepts_new_issues());

        fs::create_dir_all(dir.join("issues/todo")).unwrap();
        let board = build(&locate(dir).unwrap()).unwrap();
        assert_eq!(names(&board), ["Backlog", "Todo", "Doing"]);
        assert!(!board.columns[0].accepts_new_issues());
        assert!(board.columns[1].accepts_new_issues());
    }

    #[test]
    fn root_files_take_new_issues_even_in_a_folder_named_like_a_column() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        fs::create_dir_all(dir.join("backlog/done")).unwrap();
        fs::write(dir.join("backlog/loose.md"), "# Loose\n").unwrap();
        fs::write(dir.join("iboard.toml"), "root = \"backlog\"\n").unwrap();
        let board = build(&locate(dir).unwrap()).unwrap();
        assert_eq!(names(&board), ["Todo", "Done"]);
        assert_eq!(board.columns[0].kind, None);
        assert!(board.columns[0].accepts_new_issues());
    }

    #[test]
    fn hidden_folders_and_folders_without_issues_are_not_columns() {
        let scratch = tempfile::tempdir().unwrap();
        let issues = scratch.path().join("issues");
        fs::create_dir_all(issues.join("todo")).unwrap();
        fs::create_dir_all(issues.join(".archive")).unwrap();
        fs::write(issues.join(".archive/old.md"), "# Old\n").unwrap();
        fs::create_dir_all(issues.join("assets")).unwrap();
        fs::write(issues.join("assets/shot.png"), "png").unwrap();
        fs::create_dir_all(issues.join("spikes")).unwrap();
        fs::write(issues.join("spikes/idea.md"), "# Idea\n").unwrap();
        let board = build(&locate(scratch.path()).unwrap()).unwrap();
        assert_eq!(names(&board), ["Todo", "Spikes"]);
    }

    #[test]
    fn natural_order_is_numeric() {
        let mut names = vec!["10-b.md", "2-a.md", "1-c.md"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["1-c.md", "2-a.md", "10-b.md"]);
    }

    #[test]
    fn canonical_titles() {
        assert_eq!(display_name(Path::new("/x/in-progress")), "Doing");
        assert_eq!(display_name(Path::new("/x/scratch-notes")), "Scratch Notes");
    }

    #[test]
    fn ready_sits_between_todo_and_doing() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        for name in ["done", "doing", "ready", "todo"] {
            fs::create_dir_all(dir.join("issues").join(name)).unwrap();
        }
        let board = build(&locate(dir).unwrap()).unwrap();
        let names: Vec<&str> = board.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Todo", "Ready", "Doing", "Done"]);
    }

    /// Doing is ordered by last edit too, but its cards carry no date.
    #[test]
    fn doing_issues_are_most_recent_first_without_a_date() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let doing = dir.join("issues/doing");
        fs::create_dir_all(&doing).unwrap();
        for name in ["a.md", "b.md"] {
            fs::write(doing.join(name), "# T\n").unwrap();
        }
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(86_400);
        fs::File::options()
            .write(true)
            .open(doing.join("a.md"))
            .unwrap()
            .set_modified(later)
            .unwrap();

        let board = build(&locate(dir).unwrap()).unwrap();
        let column = board.columns.iter().find(|c| c.name == "Doing").unwrap();
        let names: Vec<String> = column.issues.iter().map(|t| t.file_name()).collect();
        assert_eq!(names, ["a.md", "b.md"]);
        assert!(column.issues.iter().all(|t| t.done_at.is_none()));
    }

    /// Outside git, done issues are dated by their file times.
    #[test]
    fn done_issues_are_dated_and_most_recent_first() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let done = dir.join("issues/done");
        let todo = dir.join("issues/todo");
        fs::create_dir_all(&done).unwrap();
        fs::create_dir_all(&todo).unwrap();
        for name in ["01-old.md", "02-mid.md", "03-new.md"] {
            fs::write(done.join(name), "# T\n").unwrap();
        }
        fs::write(todo.join("a.md"), "# A\n").unwrap();
        // Edited after the others: first despite its number.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(86_400);
        fs::File::options()
            .write(true)
            .open(done.join("01-old.md"))
            .unwrap()
            .set_modified(later)
            .unwrap();

        let board = build(&locate(dir).unwrap()).unwrap();
        let column = |name: &str| board.columns.iter().find(|c| c.name == name).unwrap();
        let names: Vec<String> = column("Done")
            .issues
            .iter()
            .map(|t| t.file_name())
            .collect();
        assert_eq!(names, ["01-old.md", "03-new.md", "02-mid.md"]);
        assert!(column("Done").issues.iter().all(|t| t.done_at.is_some()));
        assert!(
            column("Todo").issues[0].done_at.is_none(),
            "only Done is dated"
        );
    }

    #[test]
    fn orderable_columns() {
        assert!(is_orderable_column(Path::new("/x/todo")));
        assert!(is_orderable_column(Path::new("/x/ready")));
        assert!(!is_orderable_column(Path::new("/x/doing")));
        assert!(!is_orderable_column(Path::new("/x/done")));
    }

    #[test]
    fn init_creates_the_standard_columns_and_is_idempotent() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();

        let located = init(dir).unwrap();
        assert_eq!(located.project_dir, dir);
        assert_eq!(located.root, dir.join("issues"));
        for name in INIT_COLUMNS {
            let column = dir.join("issues").join(name);
            assert!(column.is_dir(), "{name} exists");
            assert!(column.join(".gitkeep").is_file(), "{name} kept in git");
        }
        let board = build(&located).unwrap();
        let names: Vec<&str> = board.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Todo", "Ready", "Doing", "Done"]);

        // A second run keeps the layout and never fails.
        let again = init(dir).unwrap();
        assert_eq!(again.root, located.root);
    }

    #[test]
    fn init_respects_a_configured_root() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("iboard.toml"), "root = \"tasks\"\n").unwrap();

        let located = init(dir).unwrap();
        assert_eq!(located.root, dir.join("tasks"));
        assert!(dir.join("tasks/todo/.gitkeep").is_file());
        assert!(!dir.join("issues").exists());
    }

    #[test]
    fn init_from_a_subfolder_uses_the_enclosing_project() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let nested = dir.join("src/deep");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(dir.join("issues")).unwrap();

        let located = init(&nested).unwrap();
        assert_eq!(located.project_dir, dir);
        assert_eq!(located.root, dir.join("issues"));
        assert!(!nested.join("issues").exists());
    }
}
