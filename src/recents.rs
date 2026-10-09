//! The projects opened recently, listed when the path in the header is clicked.
//!
//! The list is a small cache file, one absolute path per line, most recent
//! first: `$XDG_CACHE_HOME/iboard/recent-projects`, or
//! `$HOME/.cache/iboard/recent-projects` when `XDG_CACHE_HOME` is
//! unset. Folders that have since disappeared are dropped when reading, so a
//! stale entry never turns into a dead menu row.

use std::path::{Path, PathBuf};

use crate::{files, settings};

/// How many projects are kept; older ones fall off the end.
const MAX_RECENT: usize = 10;

const CACHE_FILE: &str = "recent-projects";

/// Where the list is stored, or `None` when neither `XDG_CACHE_HOME` nor
/// `HOME` is set.
pub fn cache_file() -> Option<PathBuf> {
    settings::cache_dir().map(|dir| dir.join(CACHE_FILE))
}

/// The remembered projects that still exist, most recent first.
pub fn read(file: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .collect()
}

/// Writes the list atomically (hidden temp file + rename). Failures are
/// ignored: a cache that cannot be written must never break the app.
pub fn write(file: &Path, projects: &[PathBuf]) {
    let Some(parent) = file.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let mut body = projects
        .iter()
        .map(|project| project.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    body.push('\n');
    let _ = files::atomic_write(file, body);
}

/// Moves `project` to the front of `list`, removing any earlier copy, and
/// drops the oldest entries beyond [`MAX_RECENT`].
pub fn remember(mut list: Vec<PathBuf>, project: &Path) -> Vec<PathBuf> {
    let project = canonical(project);
    list.retain(|entry| canonical(entry) != project);
    list.insert(0, project);
    list.truncate(MAX_RECENT);
    list
}

/// The absolute path with symlinks resolved when it exists, else the path as
/// given. Used so the same project is never listed twice under two spellings.
pub fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "iboard-recents-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        canonical(&dir)
    }

    #[test]
    fn remembering_moves_a_project_to_the_front_without_duplicates() {
        let a = scratch_dir("a");
        let b = scratch_dir("b");
        let c = scratch_dir("c");
        let file = scratch_dir("file").join(CACHE_FILE);

        let mut list = remember(Vec::new(), &a);
        list = remember(list, &b);
        list = remember(list, &c);
        list = remember(list, &a); // reopened
        write(&file, &list);

        assert_eq!(read(&file), vec![a, c, b], "newest first, no duplicate `a`");

        std::fs::remove_dir_all(file.parent().unwrap()).ok();
    }

    #[test]
    fn the_list_is_capped_and_drops_the_oldest() {
        let dir = scratch_dir("cap");
        let file = dir.join(CACHE_FILE);
        let mut list = Vec::new();
        let projects: Vec<PathBuf> = (0..MAX_RECENT + 2)
            .map(|i| {
                let project = dir.join(format!("project-{i}"));
                std::fs::create_dir_all(&project).unwrap();
                canonical(&project)
            })
            .collect();

        for project in &projects {
            list = remember(list, project);
        }
        write(&file, &list);
        let read_back = read(&file);

        assert_eq!(read_back.len(), MAX_RECENT);
        assert_eq!(read_back.first(), projects.last());
        assert!(!read_back.contains(&projects[0]), "the oldest fell off");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reading_skips_projects_that_are_gone() {
        let dir = scratch_dir("stale");
        let file = dir.join(CACHE_FILE);
        let gone = dir.join("deleted");
        std::fs::write(&file, format!("{}\n{}\n\n", gone.display(), dir.display())).unwrap();

        assert_eq!(read(&file), vec![canonical(&dir)]);

        std::fs::remove_dir_all(&dir).ok();
    }
}
