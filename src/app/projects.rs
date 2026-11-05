//! The user's projects in their own order, and the ProjectMenu that lists
//! them under the board header's project name.

use super::{
    IboardApp, Instant, Path, PathBuf, Rect, UserConfig, all, display_path, file_name, groups,
    recents, settings, workspace,
};
use crate::project_menu::{self, Action, Entries, GroupRow, Pick, ProjectRow};
use crate::workspace::config;

impl IboardApp {
    /// Every project the menu lists, in the user's order: the stored order,
    /// then recent projects and group projects it does not name yet. Folders
    /// that are gone drop out.
    pub(super) fn ordered_projects(&self) -> Vec<PathBuf> {
        let grouped = self
            .workspace
            .iter()
            .flat_map(|workspace| &workspace.groups)
            .flat_map(|group| &group.repos)
            .map(|repo| recents::canonical(repo));
        let mut list: Vec<PathBuf> = Vec::new();
        let candidates = self.projects.iter().chain(&self.recents).cloned();
        for project in candidates.chain(grouped) {
            if !list.contains(&project) && project.is_dir() {
                list.push(project);
            }
        }
        list
    }

    /// A project opened for the first time goes to the top of the order.
    pub(super) fn add_to_order(&mut self, project: &Path) {
        let project = recents::canonical(project);
        if self.projects.contains(&project) {
            return;
        }
        let mut order = self.ordered_projects();
        order.retain(|known| *known != project);
        order.insert(0, project);
        self.save_project_order(order, false);
    }

    /// Keeps `order` and writes it to the user configuration's
    /// `workspace.projects`, saying so when `tell`.
    fn save_project_order(&mut self, order: Vec<PathBuf>, tell: bool) {
        self.projects = order;
        let Some(file) = self.config_file.clone() else {
            return;
        };
        match config::save_projects(&file, &self.projects, settings::home().as_deref()) {
            Ok(()) => {
                self.config_stamp = groups::modified(&file);
                if tell {
                    let notice = format!("Saved your project order to {}", display_path(&file));
                    self.session.notice = Some((notice, Instant::now()));
                }
            }
            Err(config::SaveError::Changed) => {}
            Err(config::SaveError::Failed(error)) => self.error = Some(error),
        }
    }

    pub(super) fn reload_project_order(&mut self, user: &mut UserConfig) {
        self.projects = workspace::load_projects(user)
            .iter()
            .map(|project| recents::canonical(project))
            .collect();
    }

    fn menu_entries(&self) -> Entries {
        let groups = self
            .workspace
            .iter()
            .flat_map(|workspace| &workspace.groups)
            .map(|group| GroupRow {
                name: group.name.clone(),
                count: group.count(),
            })
            .collect();
        let projects = self
            .ordered_projects()
            .into_iter()
            .map(|path| ProjectRow {
                name: file_name(&path),
                detail: display_path(&path),
                path,
            })
            .collect();
        let (current_group, current_project) = match self.current_target() {
            all::Target::Group(name) => (Some(name), None),
            all::Target::Project(path) => (None, Some(path)),
        };
        Entries {
            groups,
            projects,
            current_group,
            current_project,
        }
    }

    pub(super) fn toggle_project_menu(&mut self, ctx: &egui::Context) {
        if self.project_menu.take().is_some() {
            return;
        }
        ctx.memory_mut(|m| {
            if let Some(focused) = m.focused() {
                m.surrender_focus(focused);
            }
        });
        self.project_menu = Some(project_menu::Menu::new(&self.menu_entries()));
    }

    /// The menu owns the keyboard while it is open.
    pub(super) fn project_menu_keys(&mut self, ctx: &egui::Context) {
        let entries = self.menu_entries();
        let action = self
            .project_menu
            .as_mut()
            .and_then(|menu| menu.keys(ctx, &entries));
        if let Some(action) = action {
            self.project_menu_action(action);
        }
    }

    pub(super) fn show_project_menu(&mut self, ctx: &egui::Context, anchor: Rect) {
        let entries = self.menu_entries();
        let action = self
            .project_menu
            .as_mut()
            .and_then(|menu| menu.show(ctx, anchor, &entries));
        if let Some(action) = action {
            self.project_menu_action(action);
        }
    }

    fn project_menu_action(&mut self, action: Action) {
        match action {
            Action::SaveOrder(order) => {
                self.save_project_order(order, true);
                return;
            }
            Action::Close => {}
            Action::Open(Pick::Group(name)) => self.open_group(&name),
            Action::Open(Pick::Project(path)) => self.open_project(path),
            Action::EditGroups => self.open_groups(false),
            Action::Add => {
                let picked = (self.folder_picker)();
                for project in picked.iter().rev() {
                    if config::has_issues_dir(project) {
                        self.add_to_order(project);
                    }
                }
                match picked.first() {
                    Some(first) => self.open_project(first.clone()),
                    None => return,
                }
            }
        }
        self.project_menu = None;
    }
}
