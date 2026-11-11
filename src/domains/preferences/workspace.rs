//! Writes the workspace back to the user configuration: only the
//! `[[workspace.group]]` tables and `workspace.projects` change, every other
//! key, table and comment stays as written.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, value};

use super::settings;
use crate::domains::workspace::Group;
use crate::domains::workspace::config::contract;

/// What a save could not do.
#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The file changed on disk since it was read.
    Changed,
    Failed(String),
}

/// A fingerprint of `file` as it is now; a missing file has one too.
pub fn fingerprint(file: &Path) -> u64 {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Replaces the groups of `file` with `groups`, in their order. With
/// `expected`, refuses when the file no longer has that fingerprint.
pub fn save_groups(
    file: &Path,
    groups: &[Group],
    home: Option<&Path>,
    expected: Option<u64>,
) -> Result<(), SaveError> {
    edit(file, expected, |document| {
        write_groups(document, groups, home)
    })
}

/// Replaces `workspace.projects`, the user's own order of their projects.
pub fn save_projects(
    file: &Path,
    projects: &[PathBuf],
    home: Option<&Path>,
) -> Result<(), SaveError> {
    edit(file, None, |document| {
        let Some(workspace) = workspace_table(document) else {
            return;
        };
        let mut list = Array::new();
        for project in projects {
            list.push(contract(project, home));
        }
        workspace.insert("projects", value(list));
    })
}

/// Edits `file` as [`settings::edit`] does, unless it no longer has the
/// fingerprint `expected`, and drops a `[workspace]` left empty.
fn edit(
    file: &Path,
    expected: Option<u64>,
    change: impl FnOnce(&mut DocumentMut),
) -> Result<(), SaveError> {
    if expected.is_some_and(|expected| expected != fingerprint(file)) {
        return Err(SaveError::Changed);
    }
    settings::edit(file, |document| {
        change(document);
        if document
            .get("workspace")
            .and_then(Item::as_table)
            .is_some_and(Table::is_empty)
        {
            document.remove("workspace");
        }
    })
    .map_err(SaveError::Failed)
}

fn workspace_table(document: &mut DocumentMut) -> Option<&mut Table> {
    if !document.get("workspace").is_some_and(Item::is_table) {
        let mut table = Table::new();
        table.set_implicit(true);
        document.insert("workspace", Item::Table(table));
    }
    document.get_mut("workspace").and_then(Item::as_table_mut)
}

fn write_groups(document: &mut DocumentMut, groups: &[Group], home: Option<&Path>) {
    let Some(workspace) = workspace_table(document) else {
        return;
    };
    // A comment above the first group belongs to its table: keep it there.
    let lead = workspace
        .get("group")
        .and_then(Item::as_array_of_tables)
        .and_then(|tables| tables.get(0))
        .map(|table| table.decor().clone());
    if groups.is_empty() {
        workspace.remove("group");
    } else {
        let mut tables = ArrayOfTables::new();
        for group in groups {
            let mut table = Table::new();
            table.insert("name", value(group.name.as_str()));
            let mut repos = Array::new();
            for repo in &group.repos {
                repos.push(contract(repo, home));
            }
            table.insert("repos", value(repos));
            tables.push(table);
        }
        if let (Some(lead), Some(first)) = (lead, tables.get_mut(0)) {
            *first.decor_mut() = lead;
        }
        workspace.insert("group", Item::ArrayOfTables(tables));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn group(name: &str, repos: &[&str]) -> Group {
        Group {
            name: name.into(),
            repos: repos.iter().map(PathBuf::from).collect(),
        }
    }

    fn workspace(text: &str) -> crate::domains::workspace::WorkspaceSection {
        let document: toml::Table = toml::from_str(text).unwrap();
        document
            .get("workspace")
            .and_then(|value| value.clone().try_into().ok())
            .unwrap_or_default()
    }

    #[test]
    fn only_the_group_tables_change() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("iboard.toml");
        let before = "# My settings\n[ai]\ntool = \"claude\" # the agent\n\n\
                      [workspace]\nother = 1\n\n\
                      # Mine\n[[workspace.group]]\nname = \"Old\"\nrepos = [\"~/old\"]\n";
        std::fs::write(&file, before).unwrap();
        let home = Path::new("/Users/me");
        let groups = [
            group("Client", &["/Users/me/code/app", "/opt/site"]),
            group("Empty", &[]),
        ];
        save_groups(&file, &groups, Some(home), Some(fingerprint(&file))).unwrap();
        let after = std::fs::read_to_string(&file).unwrap();
        assert!(
            after.starts_with("# My settings\n[ai]\ntool = \"claude\" # the agent\n"),
            "{after}"
        );
        assert!(after.contains("other = 1"), "{after}");
        assert!(!after.contains("Old"), "{after}");
        assert!(
            after.contains("# Mine\n[[workspace.group]]\nname = \"Client\""),
            "{after}"
        );
        assert!(
            after.contains("repos = [\"~/code/app\", \"/opt/site\"]"),
            "{after}"
        );
        assert!(after.contains("name = \"Empty\"\nrepos = []"), "{after}");

        let names: Vec<String> = workspace(&after)
            .workspace()
            .unwrap()
            .groups
            .into_iter()
            .map(|group| group.name)
            .collect();
        assert_eq!(names, ["Client", "Empty"]);
    }

    #[test]
    fn a_file_changed_since_it_was_read_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("iboard.toml");
        std::fs::write(&file, "[ai]\n").unwrap();
        let read = fingerprint(&file);
        std::fs::write(&file, "[ai]\ntool = \"claude\"\n").unwrap();
        let groups = [group("A", &[])];
        assert_eq!(
            save_groups(&file, &groups, None, Some(read)),
            Err(SaveError::Changed)
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[ai]\ntool = \"claude\"\n"
        );
        save_groups(&file, &groups, None, None).unwrap();
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("name = \"A\"")
        );
    }

    #[test]
    fn a_broken_file_is_left_alone_and_its_line_reported() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("iboard.toml");
        let broken = "[ai]\ntool = \"claude\"\n[workspace\n";
        std::fs::write(&file, broken).unwrap();
        let err = save_groups(&file, &[group("A", &[])], None, None).unwrap_err();
        assert!(
            matches!(&err, SaveError::Failed(why) if why.contains("line 3")),
            "{err:?}"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), broken);
    }

    #[test]
    fn no_group_left_removes_the_tables_and_a_new_file_gets_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub/iboard.toml");
        save_groups(&file, &[group("A", &["/a"])], None, None).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[[workspace.group]]\nname = \"A\"\nrepos = [\"/a\"]\n"
        );
        save_groups(&file, &[], None, None).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
    }

    #[test]
    fn the_project_order_is_written_beside_the_groups() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("iboard.toml");
        let before = "# mine\n[ai]\ntool = \"claude\"\n\n\
                      [[workspace.group]]\nname = \"A\"\nrepos = [\"/a\"]\n";
        std::fs::write(&file, before).unwrap();
        let home = Path::new("/Users/me");
        let order = [PathBuf::from("/Users/me/b"), PathBuf::from("/a")];
        save_projects(&file, &order, Some(home)).unwrap();
        let after = std::fs::read_to_string(&file).unwrap();
        assert!(
            after.starts_with("# mine\n[ai]\ntool = \"claude\"\n"),
            "{after}"
        );
        assert!(
            after.contains("[workspace]\nprojects = [\"~/b\", \"/a\"]"),
            "{after}"
        );
        assert!(
            after.contains("[[workspace.group]]\nname = \"A\""),
            "{after}"
        );

        save_groups(&file, &[group("B", &[])], None, Some(fingerprint(&file))).unwrap();
        let expanded = workspace(&std::fs::read_to_string(&file).unwrap()).projects();
        assert_eq!(expanded.len(), 2, "saving the groups keeps the order");
        assert_eq!(expanded[1], PathBuf::from("/a"));
    }
}
