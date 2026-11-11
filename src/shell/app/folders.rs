//! FoldersDialog from the app's side: offering a single project's missing
//! standard folders when its board opens, creating them on request, and
//! remembering the projects that said Not now.

use super::{IboardApp, Path, discovery, display_path, files, groups, onboarding, recents};
use crate::preferences::workspace as user_config;
use crate::shell::folders_ui::{self, Action};

impl IboardApp {
    /// Offers the missing standard folders when a single project's board
    /// opens, unless this launch already asked or the project said Not now.
    pub(super) fn offer_folders(&mut self) {
        self.folders_dialog = None;
        if self.welcoming() || self.group.is_some() || !self.located.root.is_dir() {
            return;
        }
        let project = recents::canonical(&self.located.project_dir);
        if self.folders_offered.contains(&project)
            || self
                .folders_quiet
                .iter()
                .any(|quiet| recents::canonical(quiet) == project)
            || onboarding::folders::missing(&self.located.root).is_empty()
        {
            return;
        }
        self.folders_offered.insert(project);
        self.open_folders_dialog();
    }

    /// The way back from Not now: forget the dismissal and open the dialog.
    pub(super) fn set_up_folders(&mut self) {
        if !self.missing_standard_folders() {
            return;
        }
        let project = recents::canonical(&self.located.project_dir);
        self.folders_offered.insert(project.clone());
        self.remember_folder_setup(&project, false);
        self.open_folders_dialog();
    }

    /// Whether the single project on the board lacks a standard folder.
    pub(super) fn missing_standard_folders(&self) -> bool {
        self.group.is_none()
            && self.located.root.is_dir()
            && !onboarding::folders::missing(&self.located.root).is_empty()
    }

    fn open_folders_dialog(&mut self) {
        let located = &self.located;
        let configured = located
            .config
            .as_ref()
            .and_then(|config| config.root.as_ref())
            .is_some();
        let name = located
            .config
            .as_ref()
            .and_then(|config| config.root.clone())
            .unwrap_or_else(|| "issues".to_string());
        self.folders_dialog = Some(folders_ui::Dialog::new(
            located.project_dir.clone(),
            self.board.name(),
            display_path(&located.project_dir),
            name,
            located.root.clone(),
            configured,
        ));
    }

    pub(super) fn folders_window(&mut self, ui: &egui::Ui) {
        let action = self
            .folders_dialog
            .as_mut()
            .and_then(|dialog| dialog.show(ui));
        match action {
            Some(Action::Close) => self.dismiss_folders(),
            Some(Action::Create { name, ignore }) => self.create_folders(&name, ignore),
            None => {}
        }
    }

    /// Not now, or `esc`: the board stays as it is and this project is not
    /// asked again.
    fn dismiss_folders(&mut self) {
        let Some(dialog) = self.folders_dialog.take() else {
            return;
        };
        let project = recents::canonical(dialog.project());
        self.remember_folder_setup(&project, true);
    }

    fn create_folders(&mut self, name: &str, ignore: bool) {
        match self.apply_folder_setup(name, ignore) {
            Ok(()) => {
                self.folders_dialog = None;
                self.reload_after_folders();
            }
            Err(error) => {
                if let Some(dialog) = &mut self.folders_dialog {
                    dialog.error = Some(error);
                }
            }
        }
    }

    /// Writes the name, then the ignore entry when asked, then the folders:
    /// each step before the one that would make the rest hard to undo.
    fn apply_folder_setup(&self, name: &str, ignore: bool) -> Result<(), String> {
        let project = &self.located.project_dir;
        let current = self
            .located
            .config
            .as_ref()
            .and_then(|config| config.root.clone())
            .unwrap_or_else(|| "issues".to_string());
        if name != current
            && let Some(problem) = onboarding::folders::name_problem(name)
        {
            return Err(problem.to_string());
        }
        let root = match name == current {
            true => self.located.root.clone(),
            false => project.join(name),
        };
        let missing = onboarding::folders::missing(&root);
        if name != current {
            onboarding::folders::write_root(project, name)?;
        }
        if ignore && !root.is_dir() {
            onboarding::folders::add_gitignore(project, &root)?;
        }
        onboarding::folders::create(&root, &missing)
    }

    /// Reads the board again where it now lives, without asking the same
    /// project anything.
    fn reload_after_folders(&mut self) {
        let project = self.located.project_dir.clone();
        let Ok(located) = discovery::locate(&project) else {
            self.error = Some(format!(
                "Couldn't open the board in {} again",
                display_path(&project)
            ));
            return;
        };
        self.pending_title = Some(discovery::window_title(&located));
        self.located = located;
        self.refresh();
    }

    /// Keeps or clears `project` in the user configuration's quiet folders.
    fn remember_folder_setup(&mut self, project: &Path, quiet: bool) {
        let known = self
            .folders_quiet
            .iter()
            .position(|entry| recents::canonical(entry) == project);
        match (quiet, known) {
            (true, None) => self.folders_quiet.push(project.to_path_buf()),
            (false, Some(at)) => {
                self.folders_quiet.remove(at);
            }
            _ => return,
        }
        let Some(file) = self.config_file.clone() else {
            return;
        };
        let home = files::home();
        match user_config::save_quiet_folders(&file, &self.folders_quiet, home.as_deref()) {
            Ok(()) => self.config_stamp = groups::modified(&file),
            Err(user_config::SaveError::Changed) => {}
            Err(user_config::SaveError::Failed(error)) => self.error = Some(error),
        }
    }
}
