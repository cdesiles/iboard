//! The All projects board: every project of the user's workspace on one
//! board, opened from the project menu or at launch outside any project.

use super::*;
use crate::discovery::Repo;

/// What the board shows: one project, or every project of the workspace.
#[derive(Clone)]
pub(super) enum Source {
    Project(Located),
    All(Workspace),
}

impl IboardApp {
    /// What the next scan reads.
    pub(super) fn source(&self) -> Source {
        match (&self.workspace, self.all) {
            (Some(workspace), true) => Source::All(workspace.clone()),
            _ => Source::Project(self.located.clone()),
        }
    }

    /// Whether the board shows every project of the workspace.
    pub(super) fn showing_all(&self) -> bool {
        self.all && self.workspace.is_some()
    }

    /// Switches the board to every project of the workspace, dropping what
    /// belonged to the project shown before, as a project switch does.
    pub(super) fn open_all(&mut self) {
        if self.workspace.is_none() {
            return;
        }
        self.all = true;
        self.pending_title = Some(workspace::TITLE.to_string());
        self.leave_project();
        self.refresh();
    }

    /// The Design panel and Draft design: on with `[idesign]`, for a single
    /// project, whose design system they read.
    pub(super) fn design_reachable(&self) -> bool {
        self.idesign && !self.showing_all()
    }

    /// The project Stats measure: the board's own, or on the All projects
    /// board the one [`Self::target_repo`] picks.
    pub(super) fn stats_project(&self) -> (PathBuf, PathBuf) {
        match self.target_repo() {
            Some(repo) => (repo.project_dir.clone(), repo.root.clone()),
            None => (self.located.project_dir.clone(), self.located.root.clone()),
        }
    }

    /// The groups of the workspace, as the header names the board's place.
    pub(super) fn workspace_groups(&self) -> String {
        self.workspace
            .iter()
            .flat_map(|workspace| &workspace.groups)
            .map(|group| group.name.as_str())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// The project new issues and Stats go to on the All projects board: the
    /// one the filter names, else the first of the group it names, else the
    /// selected card's, else the first. `None` on a single project.
    pub(super) fn target_repo(&self) -> Option<&Repo> {
        let repos = &self.board.repos;
        let tokens = &self.filter_bar.filter.tokens;
        let named = |prefix: &str, of: fn(&Repo) -> &str| {
            tokens.iter().find_map(|token| {
                let name = token.strip_prefix(prefix)?;
                repos
                    .iter()
                    .find(|repo| labels::normalize(of(repo)).as_deref() == Some(name))
            })
        };
        named("repo:", |repo| &repo.name)
            .or_else(|| named("group:", |repo| &repo.group))
            .or_else(|| {
                let selected = self.selected_issue()?;
                self.board.repo_of(&selected)
            })
            .or_else(|| repos.first())
    }

    /// Where the composer writes on the column `dir`, without creating it:
    /// `None` on a single project, where it writes in `dir` itself.
    pub(super) fn compose_preview(&self, dir: &Path) -> Option<PathBuf> {
        let repo = self.target_repo()?;
        let kind = file_name(dir);
        let folder = repo.dirs.iter().find(|(of, _)| *of == kind);
        Some(folder.map_or_else(|| repo.root.join(&kind), |(_, folder)| folder.clone()))
    }

    /// Where the composer writes on the column `dir`, created if missing.
    pub(super) fn compose_dir(&self, dir: &Path) -> Result<PathBuf, String> {
        match self.target_repo() {
            Some(repo) => discovery::dir_in(repo, dir),
            None => Ok(dir.to_path_buf()),
        }
    }

    /// `todo/` for a single project, `app/todo/` on the All projects board:
    /// the folder a new issue lands in, as the composer shows it.
    pub(super) fn compose_folder(&self, dir: &Path) -> String {
        let (name, root) = match self.board.repo_of(dir) {
            Some(repo) => (format!("{}/", repo.name), repo.root.as_path()),
            None => (String::new(), self.board.root.as_path()),
        };
        match dir.strip_prefix(root) {
            Ok(relative) if !relative.as_os_str().is_empty() => {
                format!("{name}{}/", relative.display())
            }
            _ => name,
        }
    }
}
