//! How an issue moves through the board: the column after the one it is in,
//! and how a Done issue leaves.

use std::path::Path;

use crate::kernel::board::Board;

/// How an issue leaves the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Delete,
    Archive,
}

/// Done issues are archived; every other column's issues are deleted.
pub fn exit(done: bool) -> Exit {
    if done { Exit::Archive } else { Exit::Delete }
}

/// The directory of the column after the one holding `path`, in board order.
/// The last column has no next one; neither has an issue off the board.
pub fn next_column<'a>(board: &'a Board, path: &Path) -> Option<&'a Path> {
    let index = board
        .columns
        .iter()
        .position(|column| column.issues.iter().any(|issue| issue.path == path))?;
    board
        .columns
        .get(index + 1)
        .map(|column| column.dir.as_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::board::{Column, ColumnKind};
    use crate::kernel::issue::Issue;

    fn issue(path: &str) -> Issue {
        Issue {
            path: path.into(),
            title: String::new(),
            labels: Vec::new(),
            summary: String::new(),
            markdown: String::new(),
            modified: None,
            done_at: None,
            built_in: None,
        }
    }

    fn column(dir: &str, paths: &[&str]) -> Column {
        Column {
            name: dir.to_string(),
            dir: dir.into(),
            kind: ColumnKind::of(Path::new(dir)),
            issues: paths.iter().map(|path| issue(path)).collect(),
        }
    }

    fn board(columns: Vec<Column>) -> Board {
        let mut board = Board::new(
            Path::new("/project"),
            Path::new("/project/issues"),
            None,
            Vec::new(),
        );
        board.columns = columns;
        board
    }

    #[test]
    fn a_done_issue_archives_and_the_others_delete() {
        assert_eq!(exit(true), Exit::Archive);
        assert_eq!(exit(false), Exit::Delete);
    }

    #[test]
    fn the_next_column_is_the_one_after_the_issue() {
        let board = board(vec![
            column("todo", &["a.md"]),
            column("ready", &["b.md"]),
            column("done", &["c.md"]),
        ]);
        assert_eq!(
            next_column(&board, Path::new("a.md")),
            Some(Path::new("ready"))
        );
        assert_eq!(
            next_column(&board, Path::new("b.md")),
            Some(Path::new("done"))
        );
        assert_eq!(next_column(&board, Path::new("c.md")), None);
        assert_eq!(next_column(&board, Path::new("missing.md")), None);
    }
}
