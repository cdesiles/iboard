//! Onboarding: what a project needs before it is a board. It creates the
//! folder layout a board lives in, where the project's `iboard.toml` says it
//! is, and never touches what is already there.

use std::path::Path;

use crate::domains::board::discovery::{self, Located};

pub mod folders;

/// Creates the board folder and its standard columns for `start`, and returns
/// the resolved project, ready to open. It resolves the project the way the
/// board's discovery does, so a config file or a folder found up the tree is
/// used instead of a new project being started below it; a path that does not
/// exist yet is created as a fresh project. Existing files and directories are
/// never touched, so running it again is safe.
pub fn init(start: &Path) -> Result<Located, String> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    let found = if start.is_dir() {
        discovery::find_project(start)
    } else {
        None
    };
    // Nothing to join: start a brand new project here.
    let located = found.unwrap_or_else(|| Located {
        project_dir: start.to_path_buf(),
        root: start.join("issues"),
        config: None,
    });
    folders::create_all(&located.root)?;
    Ok(located)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::domains::board::discovery::build;
    use crate::kernel::board::INIT_COLUMNS;
    use std::fs;

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
