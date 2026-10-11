//! How ColumnFocus orders a column's issues, and groups them by label.

use std::path::Path;

use crate::discovery::{self, Column};
use crate::filter::Filter;
use crate::issue::Issue;
use crate::sheet;

/// How a column orders its cards in ColumnFocus. The choice is remembered per
/// column for the session; only [`Sort::Board`] offers drag reordering.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sort {
    /// The order the files are in on the board.
    Board,
    /// Most recently edited first.
    Modified,
    /// Most recently created first.
    Created,
    /// Title, A to Z.
    Title,
    /// The most open checklist items first.
    MostToDo,
    /// Most recently finished first; Done's default.
    Finished,
}

impl Sort {
    /// The five sorts every column offers; Done additionally opens with
    /// [`Sort::Finished`].
    pub const MENU: [Sort; 5] = [
        Sort::Board,
        Sort::Modified,
        Sort::Created,
        Sort::Title,
        Sort::MostToDo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Sort::Board => "Board order",
            Sort::Modified => "Recently modified",
            Sort::Created => "Recently created",
            Sort::Title => "Title A to Z",
            Sort::MostToDo => "Most to do",
            Sort::Finished => "Recently finished",
        }
    }
}

/// The default sort of a focused column, chosen by what people look for there.
pub fn default_sort(dir: &Path) -> Sort {
    if discovery::is_ready_column(dir) {
        Sort::Board
    } else if discovery::is_done_column(dir) {
        Sort::Finished
    } else {
        Sort::Modified
    }
}

/// The focused column's issues, filtered and ordered by `sort`.
pub fn focus_issues<'a>(column: &'a Column, filter: &Filter, sort: Sort) -> Vec<&'a Issue> {
    let mut issues: Vec<&Issue> = column
        .issues
        .iter()
        .filter(|issue| filter.matches(issue))
        .collect();
    match sort {
        Sort::Board => {}
        Sort::Modified => issues.sort_by_key(|issue| std::cmp::Reverse(issue.modified)),
        Sort::Created => {
            issues.sort_by_key(|issue| std::cmp::Reverse(issue.created.or(issue.modified)))
        }
        Sort::Title => issues.sort_by_cached_key(|issue| issue.title.to_lowercase()),
        Sort::MostToDo => issues.sort_by_key(|issue| {
            let (done, total) = sheet::checklist(&issue.markdown);
            std::cmp::Reverse(total.saturating_sub(done))
        }),
        Sort::Finished => issues.sort_by_key(|issue| {
            std::cmp::Reverse(issue.done_at.map(|at| at.timestamp()).unwrap_or(i64::MIN))
        }),
    }
    issues
}

/// Splits sorted issues into label sections, most-used label first with the
/// unlabelled ones last, for Group by label.
pub fn group_issues(issues: Vec<&Issue>) -> Vec<(Option<String>, Vec<&Issue>)> {
    let mut map: Vec<(String, Vec<&Issue>)> = Vec::new();
    let mut unlabelled: Vec<&Issue> = Vec::new();
    for issue in issues {
        // An issue with two labels shows under both.
        if issue.labels.is_empty() {
            unlabelled.push(issue);
        }
        for label in &issue.labels {
            match map.iter_mut().find(|(name, _)| name == label) {
                Some((_, group)) => group.push(issue),
                None => map.push((label.clone(), vec![issue])),
            }
        }
    }
    map.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let mut groups: Vec<(Option<String>, Vec<&Issue>)> = map
        .into_iter()
        .map(|(name, group)| (Some(name), group))
        .collect();
    if !unlabelled.is_empty() {
        groups.push((None, unlabelled));
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_column_opens_with_its_own_sort() {
        assert_eq!(default_sort(Path::new("/x/todo")), Sort::Modified);
        assert_eq!(default_sort(Path::new("/x/ready")), Sort::Board);
        assert_eq!(default_sort(Path::new("/x/doing")), Sort::Modified);
        assert_eq!(default_sort(Path::new("/x/done")), Sort::Finished);
    }

    /// A Todo column of `cards`: a title, and maybe a label.
    fn todo_column(cards: &[(&str, Option<&str>)]) -> (tempfile::TempDir, Column) {
        let scratch = tempfile::tempdir().unwrap();
        let todo = scratch.path().join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        for (index, (title, label)) in cards.iter().enumerate() {
            let body = match label {
                Some(label) => format!("# [{label}] {title}\n"),
                None => format!("# {title}\n\n- [ ] one\n- [x] two\n"),
            };
            std::fs::write(todo.join(format!("card-{index}.md")), body).unwrap();
        }
        let located = discovery::locate(scratch.path()).unwrap();
        let mut board = discovery::build(&located).unwrap();
        let column = board.columns.remove(0);
        (scratch, column)
    }

    #[test]
    fn sorts_order_the_cards() {
        let (_dir, column) = todo_column(&[("Zebra", None), ("Apple", None), ("Mango", None)]);
        let titles = |sort| {
            focus_issues(&column, &Filter::default(), sort)
                .into_iter()
                .map(|issue| issue.title.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(titles(Sort::Board), ["Zebra", "Apple", "Mango"]);
        assert_eq!(titles(Sort::Title), ["Apple", "Mango", "Zebra"]);
        assert_eq!(titles(Sort::MostToDo).len(), 3);
    }

    #[test]
    fn groups_go_by_label_count_with_the_unlabelled_last() {
        let (_dir, column) = todo_column(&[
            ("A", Some("bug")),
            ("B", Some("bug")),
            ("C", Some("feature")),
            ("D", None),
        ]);
        let groups = group_issues(focus_issues(&column, &Filter::default(), Sort::Board));
        let names: Vec<Option<&str>> = groups.iter().map(|(label, _)| label.as_deref()).collect();
        assert_eq!(names, [Some("bug"), Some("feature"), None]);
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[2].1.len(), 1);
    }
}
