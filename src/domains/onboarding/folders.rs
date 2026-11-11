//! The four standard column folders a board expects, the ones a project
//! lacks, and the small writes that set them up: the `.gitkeep` files that
//! keep them in git, the `.gitignore` entry that keeps a new board out of it,
//! and the folder name.

use std::fs;
use std::path::{Path, PathBuf};

use crate::domains::board::config;
use crate::kernel::board::ColumnKind;
use crate::platform::files;

/// The longest folder name the dialog accepts.
pub const MAX_NAME_CHARS: usize = 20;

/// One of the standard columns: its kind, its title, and the folder that
/// backs it here (the canonical name when it is missing).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standard {
    pub kind: ColumnKind,
    pub name: &'static str,
    pub folder: String,
    pub present: bool,
}

/// The standard columns, in board order, and whether each is present.
pub fn standard(root: &Path) -> [Standard; 4] {
    let folders = read_folders(root);
    [
        ("todo", ColumnKind::Todo, "Todo"),
        ("ready", ColumnKind::Ready, "Ready"),
        ("doing", ColumnKind::Doing, "Doing"),
        ("done", ColumnKind::Done, "Done"),
    ]
    .map(|(folder, kind, name)| {
        let found = folders
            .iter()
            .filter(|dir| ColumnKind::of(dir) == Some(kind))
            .min_by_key(|dir| (files::file_name(dir) != folder, files::file_name(dir)));
        Standard {
            kind,
            name,
            folder: found
                .map(|dir| files::file_name(dir))
                .unwrap_or_else(|| folder.to_string()),
            present: found.is_some(),
        }
    })
}

/// The standard columns `root` lacks, in board order.
pub fn missing(root: &Path) -> Vec<Standard> {
    standard(root)
        .into_iter()
        .filter(|folder| !folder.present)
        .collect()
}

/// The problem that stops `name` from being a folder name, if any.
pub fn name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("A folder name is not empty.");
    }
    if name == "." || name == ".." {
        return Some("A folder name is not `.` or `..`.");
    }
    if name.contains('/') {
        return Some("A folder name has no slash.");
    }
    if name.starts_with('.') {
        return Some("A folder name does not start with a dot.");
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Some("A folder name has at most 20 characters.");
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Some("A folder name holds letters, digits, `-`, `_` and inner dots.");
    }
    None
}

/// Creates `folders` in `root`, each with its `.gitkeep`, and says what could
/// not be created. A failure on one folder does not stop the others.
pub fn create(root: &Path, folders: &[Standard]) -> Result<(), String> {
    if !root.is_dir() {
        fs::create_dir_all(root)
            .map_err(|err| format!("Couldn't create {}/: {err}", files::file_name(root)))?;
    }
    let mut errors = Vec::new();
    for folder in folders {
        let dir = root.join(&folder.folder);
        if let Err(err) = create_folder(&dir) {
            errors.push(format!("Couldn't create {}/: {err}", folder.folder));
        }
    }
    match errors.is_empty() {
        true => Ok(()),
        false => Err(errors.join(" ")),
    }
}

/// Creates the board folder and its four canonical columns, as `init` does.
pub fn create_all(root: &Path) -> Result<(), String> {
    let folders = [
        ("todo", ColumnKind::Todo, "Todo"),
        ("ready", ColumnKind::Ready, "Ready"),
        ("doing", ColumnKind::Doing, "Doing"),
        ("done", ColumnKind::Done, "Done"),
    ]
    .map(|(folder, kind, name)| Standard {
        kind,
        name,
        folder: folder.to_string(),
        present: false,
    });
    create(root, &folders)
}

fn create_folder(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let keep = dir.join(".gitkeep");
    if !keep.exists() {
        fs::File::create(keep)?;
    }
    Ok(())
}

fn read_folders(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir() && !is_hidden(path))
                .collect()
        })
        .unwrap_or_default()
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

/// The entry that keeps `root` out of git: root-anchored and folder-only.
pub fn ignore_entry(project_dir: &Path, root: &Path) -> Option<String> {
    let relative = root.strip_prefix(project_dir).ok()?;
    let name = relative.to_string_lossy().replace('\\', "/");
    (!name.is_empty()).then(|| format!("/{name}/"))
}

/// Adds the entry that keeps `root` out of git to the project's `.gitignore`,
/// creating the file when it is absent and never touching another line.
pub fn add_gitignore(project_dir: &Path, root: &Path) -> Result<(), String> {
    let Some(entry) = ignore_entry(project_dir, root) else {
        return Ok(());
    };
    let file = project_dir.join(".gitignore");
    let mut text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("Couldn't read {}: {err}", file.display())),
    };
    if text.lines().any(|line| line.trim() == entry) {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&entry);
    text.push('\n');
    files::write_creating(&file, text).map_err(|err| match err {
        files::WriteError::Folder(dir, err) => format!("Couldn't create {}: {err}", dir.display()),
        files::WriteError::File(err) => format!("Couldn't write {}: {err}", file.display()),
    })
}

/// Writes `root` to the project's `iboard.toml`, keeping every other key.
pub fn write_root(project_dir: &Path, name: &str) -> Result<(), String> {
    config::write_root(project_dir, name)
}

/// The folder name the error strip offers to create: the project's configured
/// root, else `issues`.
pub fn root_name(project_dir: &Path) -> String {
    config::load(project_dir)
        .and_then(|config| config.root)
        .unwrap_or_else(|| "issues".to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_folder_counts_by_its_kind_not_its_name() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        for name in ["todo", "ready", "wip", "completed"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        let standard = standard(root);
        assert!(standard.iter().all(|folder| folder.present));
        assert_eq!(standard[2].folder, "wip");
        assert_eq!(standard[3].folder, "completed");
        assert!(missing(root).is_empty(), "wip is Doing, completed is Done");
    }

    #[test]
    fn the_missing_ones_are_created_empty_with_a_gitkeep() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        fs::create_dir_all(root.join("todo")).unwrap();
        let missing = missing(root);
        let names: Vec<&str> = missing
            .iter()
            .map(|folder| folder.folder.as_str())
            .collect();
        assert_eq!(names, ["ready", "doing", "done"]);
        create(root, &missing).unwrap();
        for name in ["ready", "doing", "done"] {
            assert!(root.join(name).join(".gitkeep").is_file(), "{name}");
        }
        assert!(
            !root.join("todo/.gitkeep").exists(),
            "an existing folder is left alone"
        );
    }

    #[test]
    fn a_name_is_one_short_path_segment() {
        for name in ["issues", "tickets", "my-notes", "v1.2", "A_b"] {
            assert_eq!(name_problem(name), None, "{name}");
        }
        for name in ["", ".", "..", "a/b", ".hidden", "with space"] {
            assert!(name_problem(name).is_some(), "{name}");
        }
        assert!(name_problem(&"a".repeat(21)).is_some());
    }

    #[test]
    fn the_gitignore_entry_is_root_anchored_and_never_repeated() {
        let scratch = tempfile::tempdir().unwrap();
        let project = scratch.path();
        let root = project.join("notes/tickets");
        fs::create_dir_all(&root).unwrap();
        assert_eq!(
            ignore_entry(project, &root).as_deref(),
            Some("/notes/tickets/")
        );
        add_gitignore(project, &root).unwrap();
        add_gitignore(project, &root).unwrap();
        assert_eq!(
            fs::read_to_string(project.join(".gitignore")).unwrap(),
            "/notes/tickets/\n"
        );

        fs::write(project.join(".gitignore"), "target\n").unwrap();
        add_gitignore(project, &root).unwrap();
        assert_eq!(
            fs::read_to_string(project.join(".gitignore")).unwrap(),
            "target\n/notes/tickets/\n"
        );
    }
}
