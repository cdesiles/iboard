//! The groups as the groups dialog edits them. Nothing touches the disk until
//! [`Draft::validate`] hands back the groups to write.

use std::path::{Path, PathBuf};

use super::{Group, Workspace};

/// One group being edited.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftGroup {
    pub name: String,
    pub repos: Vec<PathBuf>,
    /// Added in this dialog: dropped on save while it has no name and no
    /// project.
    pub new: bool,
    /// Why the group cannot be saved, under its head.
    pub error: Option<String>,
    /// What the last add could not do, in the group's help line.
    pub notice: Option<String>,
    /// The name it had in the configuration, when it was there.
    pub origin: Option<String>,
}

/// What adding a folder to a group did.
#[derive(Debug, PartialEq, Eq)]
pub enum Added {
    Added,
    AlreadyThere,
    NoIssues,
}

#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub groups: Vec<DraftGroup>,
    original: Vec<Group>,
}

impl Draft {
    pub fn new(workspace: Option<&Workspace>) -> Self {
        let original: Vec<Group> = workspace.map(|w| w.groups.clone()).unwrap_or_default();
        let groups = original
            .iter()
            .map(|group| DraftGroup {
                name: group.name.clone(),
                repos: group.repos.clone(),
                origin: Some(group.name.clone()),
                ..DraftGroup::default()
            })
            .collect();
        Self { groups, original }
    }

    /// Appends a new, empty group and returns where it is.
    pub fn add_group(&mut self) -> usize {
        self.groups.push(DraftGroup {
            new: true,
            ..DraftGroup::default()
        });
        self.groups.len() - 1
    }

    /// Adds the project at `dir` to group `at`; `has_issues` says whether a
    /// folder holds an `issues/` folder.
    pub fn add_project(
        &mut self,
        at: usize,
        dir: &Path,
        has_issues: impl Fn(&Path) -> bool,
    ) -> Added {
        let Some(group) = self.groups.get_mut(at) else {
            return Added::AlreadyThere;
        };
        if group.repos.iter().any(|repo| repo == dir) {
            return Added::AlreadyThere;
        }
        if !has_issues(dir) {
            group.notice = Some(format!(
                "{} has no issues/ folder. Pick the project's root.",
                dir.display()
            ));
            return Added::NoIssues;
        }
        group.notice = None;
        group.repos.push(dir.to_path_buf());
        Added::Added
    }

    pub fn remove_project(&mut self, at: usize, index: usize) {
        if let Some(group) = self.groups.get_mut(at)
            && index < group.repos.len()
        {
            group.repos.remove(index);
        }
    }

    pub fn delete_group(&mut self, at: usize) {
        if at < self.groups.len() {
            self.groups.remove(at);
        }
    }

    /// The groups as they would be written.
    fn to_write(&self) -> Vec<Group> {
        self.groups
            .iter()
            .filter(|group| !(group.new && group.name.trim().is_empty() && group.repos.is_empty()))
            .map(|group| Group {
                name: group.name.trim().to_string(),
                repos: group.repos.clone(),
            })
            .collect()
    }

    /// Each group of the configuration still in the draft: its old name,
    /// then its name once saved.
    pub fn renames(&self) -> Vec<(String, String)> {
        self.groups
            .iter()
            .filter_map(|group| Some((group.origin.clone()?, group.name.trim().to_string())))
            .collect()
    }

    /// Whether saving would change the configuration.
    pub fn is_dirty(&self) -> bool {
        self.to_write() != self.original
    }

    /// The groups to write, or `None` after putting an error under each
    /// group that cannot be saved: a name must be there, and unique
    /// ignoring case.
    pub fn validate(&mut self) -> Option<Vec<Group>> {
        let names: Vec<String> = self
            .groups
            .iter()
            .map(|group| group.name.trim().to_lowercase())
            .collect();
        let mut valid = true;
        for (at, group) in self.groups.iter_mut().enumerate() {
            let name = group.name.trim();
            let dropped = group.new && name.is_empty() && group.repos.is_empty();
            group.error = if dropped {
                None
            } else if name.is_empty() {
                Some("Name the group, or delete it.".to_string())
            } else if names[..at].contains(&name.to_lowercase()) {
                Some(format!(
                    "Another group is already called {name}. Rename one of them."
                ))
            } else {
                None
            };
            valid &= group.error.is_none();
        }
        valid.then(|| self.to_write())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn workspace() -> Workspace {
        Workspace {
            groups: vec![Group {
                name: "Client".into(),
                repos: vec![PathBuf::from("/code/app")],
            }],
        }
    }

    #[test]
    fn a_fresh_draft_is_clean_and_saves_as_it_was() {
        let mut draft = Draft::new(Some(&workspace()));
        assert!(!draft.is_dirty());
        assert_eq!(draft.validate().unwrap(), workspace().groups);
        assert!(Draft::new(None).groups.is_empty());
    }

    #[test]
    fn a_new_group_without_name_or_project_is_dropped() {
        let mut draft = Draft::new(Some(&workspace()));
        draft.add_group();
        assert!(!draft.is_dirty(), "nothing would be written");
        assert_eq!(draft.validate().unwrap().len(), 1);
    }

    #[test]
    fn names_must_be_there_and_unique_ignoring_case() {
        let mut draft = Draft::new(Some(&workspace()));
        let at = draft.add_group();
        draft.groups[at].name = " client ".into();
        assert!(draft.validate().is_none());
        assert_eq!(
            draft.groups[at].error.as_deref(),
            Some("Another group is already called client. Rename one of them.")
        );
        draft.groups[0].name = "  ".into();
        draft.groups[at].name = "Personal".into();
        assert!(draft.validate().is_none());
        assert_eq!(
            draft.groups[0].error.as_deref(),
            Some("Name the group, or delete it.")
        );
        assert!(draft.groups[at].error.is_none());
        draft.groups[0].name = " Client ".into();
        let groups = draft.validate().unwrap();
        assert_eq!(groups[0].name, "Client", "trimmed");
    }

    #[test]
    fn projects_are_added_once_and_only_with_an_issues_folder() {
        let mut draft = Draft::new(Some(&workspace()));
        let yes = |_: &Path| true;
        let no = |_: &Path| false;
        assert_eq!(
            draft.add_project(0, Path::new("/code/app"), yes),
            Added::AlreadyThere
        );
        assert_eq!(
            draft.add_project(0, Path::new("/code/legacy"), no),
            Added::NoIssues
        );
        assert_eq!(
            draft.groups[0].notice.as_deref(),
            Some("/code/legacy has no issues/ folder. Pick the project's root.")
        );
        assert_eq!(
            draft.add_project(0, Path::new("/code/site"), yes),
            Added::Added
        );
        assert!(draft.groups[0].notice.is_none());
        assert!(draft.is_dirty());
        draft.remove_project(0, 1);
        assert!(!draft.is_dirty(), "back as it was");
    }

    #[test]
    fn renames_follow_the_groups_of_the_configuration() {
        let mut draft = Draft::new(Some(&workspace()));
        let at = draft.add_group();
        draft.groups[at].name = "New".into();
        draft.groups[0].name = " Customer ".into();
        assert_eq!(
            draft.renames(),
            [("Client".to_string(), "Customer".to_string())]
        );
        draft.delete_group(0);
        assert!(draft.renames().is_empty());
    }

    #[test]
    fn deleting_a_group_only_changes_the_draft() {
        let mut draft = Draft::new(Some(&workspace()));
        draft.delete_group(0);
        draft.delete_group(5);
        assert!(draft.is_dirty());
        assert!(draft.validate().unwrap().is_empty());
    }
}
