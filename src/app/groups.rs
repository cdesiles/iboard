//! The groups dialog from the app's side: opening it, writing what it saves
//! back to the user configuration, and keeping the board on screen when the
//! groups change, from the dialog or from an edit outside iboard.

use super::*;
use crate::groups_ui;
use crate::workspace::config::{self, SaveError};

/// How often the user configuration is looked at for an outside edit.
const CONFIG_CHECK_EVERY: Duration = Duration::from_secs(1);

impl IboardApp {
    /// Opens the groups dialog on the configuration as it is now; with
    /// `new_group`, straight into a new group.
    pub(super) fn open_groups(&mut self, new_group: bool) {
        let fingerprint = self.config_file.as_deref().map_or(0, config::fingerprint);
        let dialog = groups_ui::Dialog::new(self.workspace.as_ref(), fingerprint, new_group)
            .with_picker(self.folder_picker);
        self.groups_dialog = Some(dialog);
    }

    pub(super) fn groups_window(&mut self, ui: &egui::Ui) {
        let projects = self.ordered_projects();
        let Some(dialog) = self.groups_dialog.as_mut() else {
            return;
        };
        let note = self
            .config_file
            .as_deref()
            .map_or_else(|| "No configuration file".to_string(), display_path);
        match dialog.show(ui, &projects, &note) {
            groups_ui::Action::Keep => {}
            groups_ui::Action::Close => self.groups_dialog = None,
            groups_ui::Action::Reload => {
                self.reload_groups();
                self.open_groups(false);
            }
            groups_ui::Action::Save(groups) => self.save_groups(groups, false),
            groups_ui::Action::Overwrite(groups) => self.save_groups(groups, true),
        }
    }

    fn save_groups(&mut self, groups: Vec<Group>, overwrite: bool) {
        let Some(dialog) = self.groups_dialog.as_mut() else {
            return;
        };
        let Some(file) = self.config_file.clone() else {
            dialog.save_error =
                Some("Couldn't save: there is no user configuration (HOME is not set).".into());
            return;
        };
        let expected = (!overwrite).then_some(dialog.fingerprint);
        match config::save(&file, &groups, settings::home().as_deref(), expected) {
            Ok(()) => {
                let renames = dialog.draft.renames();
                self.groups_dialog = None;
                let count = match groups.len() {
                    1 => "1 group".to_string(),
                    n => format!("{n} groups"),
                };
                self.config_stamp = modified(&file);
                self.apply_groups(groups, &renames);
                let notice = format!("Saved {count} to {}", display_path(&file));
                self.session.notice = Some((notice, Instant::now()));
            }
            Err(SaveError::Changed) => {
                dialog.conflict = true;
                dialog.save_error = None;
            }
            Err(SaveError::Failed(error)) => dialog.save_error = Some(error),
        }
    }

    /// Reads the groups from the configuration again, keeping the group on
    /// screen when it is still there. A configuration with problems, saved
    /// halfway through an edit, leaves the workspace as it is.
    fn reload_groups(&mut self) {
        let mut user = UserConfig::load(self.config_file.as_deref());
        let groups = workspace::load(&mut user)
            .map(|w| w.groups)
            .unwrap_or_default();
        self.report_config_problems(&user.problems);
        if !user.problems.is_empty() {
            return;
        }
        self.reload_project_order(&mut user);
        let same: Vec<(String, String)> = groups
            .iter()
            .map(|group| (group.name.clone(), group.name.clone()))
            .collect();
        self.apply_groups(groups, &same);
    }

    /// Puts `groups` in place of the workspace's. The group on screen stays,
    /// under the name `renames` gives it; gone, the board goes to the first
    /// group left, else to the most recent project.
    pub(super) fn apply_groups(&mut self, groups: Vec<Group>, renames: &[(String, String)]) {
        self.workspace = (!groups.is_empty()).then_some(Workspace { groups });
        let Some(shown) = self.group.clone() else {
            return;
        };
        let renamed = renames
            .iter()
            .find(|(old, _)| *old == shown)
            .map(|(_, new)| new.clone())
            .filter(|name| {
                self.workspace
                    .as_ref()
                    .is_some_and(|w| w.group(name).is_some())
            });
        if let Some(name) = renamed {
            self.group = Some(name.clone());
            if name != shown {
                self.pending_title = Some(name);
            }
            self.refresh();
            self.drop_stale_tokens();
            return;
        }
        let first = self.workspace.as_ref().and_then(|w| w.groups.first());
        if let Some(first) = first.map(|group| group.name.clone()) {
            self.open_group(&first);
        } else if let Some(recent) = self.recents.first().cloned() {
            self.open_project(recent);
        } else {
            self.group = None;
            self.refresh();
        }
    }

    /// Drops the filter's `repo:` and `group:` tokens that no longer name a
    /// project or the group of the board.
    fn drop_stale_tokens(&mut self) {
        let repos: Vec<String> = self
            .board
            .repos
            .iter()
            .filter_map(|repo| labels::normalize(&repo.name))
            .collect();
        let group = self.group.as_deref().and_then(labels::normalize);
        self.filter_bar.filter.tokens.retain(|token| {
            if let Some(name) = token.strip_prefix("repo:") {
                repos.iter().any(|repo| repo == name)
            } else if let Some(name) = token.strip_prefix("group:") {
                group.as_deref() == Some(name)
            } else {
                true
            }
        });
    }

    /// Reloads the groups when the configuration changed outside iboard.
    pub(super) fn watch_config(&mut self) {
        if self.config_checked.elapsed() < CONFIG_CHECK_EVERY {
            return;
        }
        self.config_checked = Instant::now();
        let Some(file) = self.config_file.as_deref() else {
            return;
        };
        let stamp = modified(file);
        if stamp != self.config_stamp {
            self.config_stamp = stamp;
            self.reload_groups();
        }
    }

    /// Shows `problems`, what the user configuration holds that iboard could
    /// not use, in the error strip and in Settings, instead of the last ones.
    pub(super) fn report_config_problems(&mut self, problems: &[String]) {
        let shown = self
            .config_problem
            .take()
            .map(|problem| format!("Settings: {problem}"));
        if self.error == shown {
            self.error = None;
        }
        if !problems.is_empty() {
            let problem = problems.join(" ");
            self.error = Some(format!("Settings: {problem}"));
            self.config_problem = Some(problem);
        }
    }
}

pub(super) fn modified(file: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(file)
        .and_then(|meta| meta.modified())
        .ok()
}
