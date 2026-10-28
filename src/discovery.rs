//! Locating the issues directory and turning its folders into kanban columns.
//!
//! Because layouts differ between projects, discovery is deliberately lenient:
//! any known column name (`todo`, `doing`, `done`, ...) is always shown, any
//! other sub-directory is shown only if it directly contains markdown files,
//! and markdown files sitting directly in the issues root form the first
//! column. A `iboard.toml` can override all of this.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

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

pub struct Column {
    pub name: String,
    /// The directory holding this column's issues. Dropping a card here moves
    /// its markdown file into this directory.
    pub dir: PathBuf,
    pub issues: Vec<Issue>,
}

impl Column {
    /// Whether the "+" / new-issue affordance is offered here. New issues
    /// start life in the todo column.
    pub fn accepts_new_issues(&self) -> bool {
        self.name.eq_ignore_ascii_case("todo")
    }

    /// Whether this is the Done column.
    pub fn is_done(&self) -> bool {
        is_done_column(&self.dir)
    }
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

    /// An empty board, used when a scan fails so the UI can still render.
    pub fn empty(located: &Located) -> Self {
        Self {
            project_dir: located.project_dir.clone(),
            root: located.root.clone(),
            title: located.config.as_ref().and_then(|c| c.title.clone()),
            reader_sections: reader_sections(located.config.as_ref()),
            columns: Vec::new(),
            warnings: Vec::new(),
        }
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

    /// The project the issue at `path` belongs to: where its agents run.
    pub fn project_of(&self, _path: &Path) -> &Path {
        &self.project_dir
    }

    /// The issues folder the issue at `path` belongs to: where its sidecars
    /// (reviews, comments, build logs, images) live.
    pub fn root_of(&self, _path: &Path) -> &Path {
        &self.root
    }
}

/// Walks up from `start` looking for a configured project or an `issues/`
/// directory. Running from inside the issues folder (or any sub-folder) also works.
pub fn locate(start: &Path) -> Result<Located> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    let origin = start.to_path_buf();
    let mut dir = origin.clone();

    loop {
        if let Some(loaded) = crate::config::load(&dir) {
            let root = match loaded.config.root.as_deref() {
                Some(relative) => dir.join(relative),
                None => dir.join("issues"),
            };
            if !root.is_dir() {
                bail!(
                    "config {} points to `{}`, which is not a directory",
                    loaded.path.display(),
                    root.display()
                );
            }
            return Ok(Located {
                project_dir: dir,
                root,
                config: Some(loaded.config),
            });
        }

        let candidate = dir.join("issues");
        if candidate.is_dir() {
            return Ok(Located {
                project_dir: dir,
                root: candidate,
                config: None,
            });
        }

        if dir.file_name().and_then(|n| n.to_str()) == Some("issues") && dir.is_dir() {
            let project_dir = dir.parent().unwrap_or(&dir).to_path_buf();
            return Ok(Located {
                project_dir,
                root: dir,
                config: None,
            });
        }

        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => bail!(
                "no `issues/` directory found from `{}` (and no parent contains one)",
                origin.display()
            ),
        }
    }
}

/// The columns a fresh project is created with: `todo → ready → doing → done`.
pub const INIT_COLUMNS: [&str; 4] = ["todo", "ready", "doing", "done"];

/// Creates the standard `issues/` layout for `start` and returns the resolved
/// project, ready to open. It resolves the project like [`locate`], so a config
/// file or an `issues/` directory found up the tree is used instead of a new
/// project being started below it; a path that does not exist yet is created as
/// a fresh project. Existing files and directories are never touched, so
/// running it again is safe.
pub fn init(start: &Path) -> Result<Located> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start).to_path_buf()
    } else {
        start.to_path_buf()
    };

    if start.is_dir() {
        let mut dir = start.clone();
        loop {
            if let Some(loaded) = crate::config::load(&dir) {
                let root = match loaded.config.root.as_deref() {
                    Some(relative) => dir.join(relative),
                    None => dir.join("issues"),
                };
                create_columns(&root)?;
                return Ok(Located {
                    project_dir: dir,
                    root,
                    config: Some(loaded.config),
                });
            }

            let candidate = dir.join("issues");
            if candidate.is_dir() {
                create_columns(&candidate)?;
                return Ok(Located {
                    project_dir: dir,
                    root: candidate,
                    config: None,
                });
            }

            if dir.file_name().and_then(|n| n.to_str()) == Some("issues") {
                let root = dir.clone();
                create_columns(&root)?;
                return Ok(Located {
                    project_dir: dir.parent().unwrap_or(&dir).to_path_buf(),
                    root,
                    config: None,
                });
            }

            match dir.parent() {
                Some(parent) if parent != dir => dir = parent.to_path_buf(),
                _ => break,
            }
        }
    }

    // Nothing to join: start a brand new project here.
    let project_dir = start;
    let root = project_dir.join("issues");
    create_columns(&root)?;
    Ok(Located {
        project_dir,
        root,
        config: None,
    })
}

/// Creates the issues root and one directory per standard column. Each column
/// gets a `.gitkeep`, because git does not track empty directories and the
/// layout is meant to survive a clone.
fn create_columns(root: &Path) -> Result<()> {
    fs::create_dir_all(root)
        .with_context(|| format!("cannot create issues directory {}", root.display()))?;
    for name in INIT_COLUMNS {
        let dir = root.join(name);
        fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create column {}", dir.display()))?;
        let keep = dir.join(".gitkeep");
        if !keep.exists() {
            fs::File::create(&keep).with_context(|| format!("cannot create {}", keep.display()))?;
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
pub fn build(located: &Located) -> Result<Board> {
    let root = &located.root;
    let config = located.config.as_ref();
    let mut warnings = Vec::new();

    // 1. Establish the ordered list of sub-directory columns.
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
            .with_context(|| format!("cannot read issues directory {}", root.display()))?;
        let mut subs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && !is_hidden(path))
            .filter(|path| is_known_column(path) || contains_markdown(path))
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

    // 2. Files sitting directly in the issues root form the first column.
    let mut columns = Vec::new();
    let root_files = collect_markdown(root, &mut warnings);
    if !root_files.is_empty() {
        let has_todo_dir = dirs.iter().any(|dir| column_rank(dir) == 1);
        let name = config
            .and_then(|c| c.root_column_name.clone())
            .unwrap_or_else(|| {
                if has_todo_dir {
                    "Backlog".to_string()
                } else {
                    "Todo".to_string()
                }
            });
        columns.push(Column {
            name,
            dir: root.clone(),
            issues: load_issues(root_files, &mut warnings),
        });
    }

    // 3. One column per sub-directory.
    for dir in &dirs {
        let files = collect_markdown(dir, &mut warnings);
        let mut issues = load_issues(files, &mut warnings);
        match canonical_name(dir) {
            Some("Done") => {
                sort_recent_first(dir, &mut issues, true);
                for issue in &mut issues {
                    let log = build_log::load(&build_log::path(root, &issue.path));
                    issue.built_in = log.as_deref().and_then(build_log::took);
                }
            }
            Some("Doing") => sort_recent_first(dir, &mut issues, false),
            _ => {}
        }
        columns.push(Column {
            name: display_name(dir),
            dir: dir.clone(),
            issues,
        });
    }

    Ok(Board {
        project_dir: located.project_dir.clone(),
        root: root.clone(),
        title: config.and_then(|c| c.title.clone()),
        reader_sections: reader_sections(config),
        columns,
        warnings,
    })
}

/// The configured reader sections, or the standard four.
fn reader_sections(config: Option<&Config>) -> Vec<String> {
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
    canonical_name(dir) == Some("Ready")
}

/// Whether `dir` is the Doing column, where the AI build's agent works.
pub fn is_doing_column(dir: &Path) -> bool {
    canonical_name(dir) == Some("Doing")
}

/// Whether `dir` is the Done column.
pub fn is_done_column(dir: &Path) -> bool {
    canonical_name(dir) == Some("Done")
}

/// Whether a column's order is its file-name order, so dropping a card at a
/// position can renumber the files. Todo and Ready; Doing and Done are sorted
/// by date instead.
pub fn is_orderable_column(dir: &Path) -> bool {
    matches!(canonical_name(dir), Some("Todo" | "Ready"))
}

/// Whether a column's issues can be edited on the board. Todo and Ready are
/// the working columns; Doing and Done are the agent's and the finished ones.
pub fn is_editable_column(dir: &Path) -> bool {
    !matches!(canonical_name(dir), Some("Doing" | "Review" | "Done"))
}

/// The width of the priority prefix in Ready: `001-make-colour-green.md`.
const PRIORITY_DIGITS: usize = 3;

/// The file name without its Ready priority prefix, if it has one.
pub fn strip_priority(name: &str) -> &str {
    let bytes = name.as_bytes();
    let numbered = bytes.len() > PRIORITY_DIGITS
        && bytes[..PRIORITY_DIGITS].iter().all(u8::is_ascii_digit)
        && bytes[PRIORITY_DIGITS] == b'-';
    if numbered {
        &name[PRIORITY_DIGITS + 1..]
    } else {
        name
    }
}

/// The name an issue takes when moved from one folder into another: entering
/// Ready numbers it after the last issue there, leaving Ready drops the
/// number. Other moves keep the name.
pub fn moved_name(name: &str, from_dir: &Path, to_dir: &Path) -> String {
    if is_ready_column(to_dir) {
        let last = fs::read_dir(to_dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| priority(&entry.file_name().to_string_lossy()))
            .max()
            .unwrap_or(0);
        format!(
            "{:0width$}-{}",
            last + 1,
            strip_priority(name),
            width = PRIORITY_DIGITS
        )
    } else if is_ready_column(from_dir) {
        strip_priority(name).to_string()
    } else {
        name.to_string()
    }
}

/// The priority number of a Ready file name: `001-x.md` → 1.
fn priority(name: &str) -> Option<u32> {
    let rest = strip_priority(name);
    if rest.len() == name.len() {
        return None;
    }
    name[..PRIORITY_DIGITS].parse().ok()
}

/// The `001-x.md` name for `name` at a one-based position, dropping any prefix
/// it already has so renumbering is stable.
pub fn numbered_name(position: usize, name: &str) -> String {
    format!(
        "{:0width$}-{}",
        position,
        strip_priority(name),
        width = PRIORITY_DIGITS
    )
}

/// Renames the files of an ordered column so their `001-` prefixes match
/// `order`. Renames go through hidden temp names in two phases, so a swap
/// (`001-a` ↔ `002-b`) cannot clobber a file. Returns every `(old, new)` pair
/// that changed.
pub fn renumber(dir: &Path, order: &[PathBuf]) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let changes: Vec<(PathBuf, PathBuf)> = order
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            let target = dir.join(numbered_name(index + 1, &files::file_name(path)));
            (target != *path).then(|| (path.clone(), target))
        })
        .collect();
    if changes.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 1: every changing file moves to a unique hidden temp name.
    let mut staged: Vec<(PathBuf, PathBuf, PathBuf)> = Vec::new();
    for (index, (original, target)) in changes.iter().enumerate() {
        let temp = dir.join(format!(".renumber-{}-{index}.tmp", std::process::id()));
        if let Err(err) = std::fs::rename(original, &temp) {
            restore(&staged);
            return Err(format!(
                "Couldn't renumber {}: {err}",
                files::file_name(original)
            ));
        }
        staged.push((temp, original.clone(), target.clone()));
    }

    // Phase 2: temps take their final names. On failure, put back what moved.
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (index, (temp, original, target)) in staged.iter().enumerate() {
        if let Err(err) = std::fs::rename(temp, target) {
            restore(&staged[index..]);
            for (final_name, original) in &done {
                let _ = std::fs::rename(final_name, original);
            }
            return Err(format!(
                "Couldn't renumber {}: {err}",
                files::file_name(original)
            ));
        }
        done.push((target.clone(), original.clone()));
    }
    Ok(changes)
}

/// Best-effort undo of the staging phase.
fn restore(staged: &[(PathBuf, PathBuf, PathBuf)]) {
    for (temp, original, _) in staged {
        let _ = std::fs::rename(temp, original);
    }
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

/// Maps a directory name to its canonical title and sort rank.
fn canonical(name: &str) -> Option<(&'static str, u8)> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "backlog" | "icebox" => Some(("Backlog", 0)),
        "todo" | "to-do" | "to_do" => Some(("Todo", 1)),
        "ready" => Some(("Ready", 2)),
        "doing" | "in-progress" | "in_progress" | "inprogress" | "wip" => Some(("Doing", 3)),
        "review" => Some(("Review", 4)),
        "later" => Some(("Later", 5)),
        "blocked" => Some(("Blocked", 6)),
        "done" | "completed" => Some(("Done", 7)),
        _ => None,
    }
}

fn display_name(dir: &Path) -> String {
    let raw = dir.file_name().and_then(|n| n.to_str()).unwrap_or("Column");
    match canonical(raw) {
        Some((name, _)) => name.to_string(),
        None => title_case(raw),
    }
}

fn is_known_column(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|n| n.to_str())
        .map(|name| canonical(name).is_some())
        .unwrap_or(false)
}

fn column_rank(dir: &Path) -> u8 {
    dir.file_name()
        .and_then(|n| n.to_str())
        .and_then(canonical)
        .map(|(_, rank)| rank)
        .unwrap_or(u8::MAX)
}

fn canonical_name(dir: &Path) -> Option<&'static str> {
    dir.file_name()
        .and_then(|n| n.to_str())
        .and_then(canonical)
        .map(|(name, _)| name)
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

        fs::create_dir_all(dir.join("issues/todo")).unwrap();
        let board = build(&locate(dir).unwrap()).unwrap();
        assert_eq!(names(&board), ["Backlog", "Todo", "Doing"]);
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

    #[test]
    fn entering_ready_numbers_an_issue_and_leaving_drops_the_number() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let (todo, ready, doing) = (dir.join("todo"), dir.join("ready"), dir.join("doing"));
        for d in [&todo, &ready, &doing] {
            fs::create_dir_all(d).unwrap();
        }
        assert_eq!(
            moved_name("make-green.md", &todo, &ready),
            "001-make-green.md"
        );
        fs::write(ready.join("001-a.md"), "").unwrap();
        fs::write(ready.join("007-b.md"), "").unwrap();
        fs::write(ready.join("2026-05-29-dated.md"), "").unwrap();
        // After the last number, not in the first gap: priority is kept.
        assert_eq!(
            moved_name("make-green.md", &todo, &ready),
            "008-make-green.md"
        );
        // A renumbered issue loses its old priority, other numbers stay.
        assert_eq!(moved_name("003-x.md", &doing, &ready), "008-x.md");
        assert_eq!(moved_name("02-ai.md", &todo, &ready), "008-02-ai.md");

        assert_eq!(moved_name("008-02-ai.md", &ready, &doing), "02-ai.md");
        assert_eq!(moved_name("008-x.md", &ready, &todo), "x.md");
        assert_eq!(moved_name("unnumbered.md", &ready, &doing), "unnumbered.md");
        assert_eq!(moved_name("001-x.md", &todo, &doing), "001-x.md");
        assert_eq!(strip_priority("2026-05-29-dated.md"), "2026-05-29-dated.md");
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
    fn orderable_columns_and_numbered_names() {
        assert_eq!(numbered_name(1, "hello.md"), "001-hello.md");
        assert_eq!(numbered_name(2, "002-byebye.md"), "002-byebye.md");
        assert_eq!(numbered_name(10, "x.md"), "010-x.md");
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
