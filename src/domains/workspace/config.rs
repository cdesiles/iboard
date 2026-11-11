//! A group's project paths as the user configuration writes them, and whether
//! a folder holds the issues a group's project needs.

use std::path::Path;

/// `path` as the user would write it: under `home`, it starts with `~/`.
pub fn contract(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

/// Whether `dir` holds the issues a group's project needs, where its board
/// finds them: the `root` its `iboard.toml` sets, else its `issues/` folder.
pub fn has_issues_dir(dir: &Path) -> bool {
    super::locate(dir).is_ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn paths_under_home_round_trip_with_a_tilde() {
        let home = Path::new("/Users/me");
        assert_eq!(
            contract(Path::new("/Users/me/code/app"), Some(home)),
            "~/code/app"
        );
        assert_eq!(contract(Path::new("/Users/me"), Some(home)), "/Users/me");
        assert_eq!(
            contract(Path::new("/Users/meow/x"), Some(home)),
            "/Users/meow/x"
        );
        assert_eq!(contract(Path::new("/opt/x"), None), "/opt/x");
    }

    #[test]
    fn a_project_has_its_issues_where_its_board_finds_them() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        std::fs::create_dir_all(dir.join("custom/tickets/todo")).unwrap();
        std::fs::write(dir.join("custom/iboard.toml"), "root = \"tickets\"\n").unwrap();
        std::fs::create_dir_all(dir.join("plain/issues/todo")).unwrap();
        std::fs::create_dir_all(dir.join("plain/src")).unwrap();
        assert!(has_issues_dir(&dir.join("custom")), "its configured root");
        assert!(has_issues_dir(&dir.join("plain")));
        assert!(
            !has_issues_dir(&dir.join("plain/src")),
            "the issues/ of the project above"
        );
        assert!(!has_issues_dir(&dir.join("gone")));
    }
}
