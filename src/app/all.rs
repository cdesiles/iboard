//! A group's board: every project of one `[[workspace.group]]` on one board,
//! opened from the project menu or, for the first group, at launch outside
//! any project.

use super::*;
use crate::discovery::Repo;
use crate::priority;

/// What the board shows: one project, or every project of a group.
#[derive(Clone)]
pub(super) enum Source {
    Project(Located),
    /// The group, and where its order across projects is kept.
    Group(Group, Option<PathBuf>),
}

/// A board `⌃⇥` and the project menu can switch to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Group(String),
    Project(PathBuf),
}

impl IboardApp {
    /// What the next scan reads.
    pub(super) fn source(&self) -> Source {
        match self.shown_group() {
            Some(group) => Source::Group(group.clone(), self.group_order_file()),
            None => Source::Project(self.located.clone()),
        }
    }

    /// The group whose board is shown, if any.
    pub(super) fn shown_group(&self) -> Option<&Group> {
        let name = self.group.as_deref()?;
        self.workspace.as_ref()?.group(name)
    }

    /// Whether the board shows a group's projects rather than one project.
    pub(super) fn showing_group(&self) -> bool {
        self.shown_group().is_some()
    }

    /// Where the shown group keeps its order across projects.
    pub(super) fn group_order_file(&self) -> Option<PathBuf> {
        let dir = self.order_dir.as_deref()?;
        Some(workspace::order::file(dir, self.group.as_deref()?))
    }

    /// Switches the board to the group `name`, dropping what belonged to the
    /// board shown before, as a project switch does.
    pub(super) fn open_group(&mut self, name: &str) {
        if self
            .workspace
            .as_ref()
            .and_then(|w| w.group(name))
            .is_none()
        {
            return;
        }
        self.group = Some(name.to_string());
        self.pending_title = Some(name.to_string());
        self.leave_project();
        self.refresh();
    }

    /// The board on screen, as `⌃⇥` and the project menu name it.
    pub(super) fn current_target(&self) -> Target {
        match self.shown_group() {
            Some(group) => Target::Group(group.name.clone()),
            None => Target::Project(recents::canonical(&self.located.project_dir)),
        }
    }

    pub(super) fn open_target(&mut self, target: Target) {
        match target {
            Target::Group(name) => self.open_group(&name),
            Target::Project(project) => self.open_project(project),
        }
    }

    /// The Design panel and Draft design: on with `[idesign]`, for a single
    /// project, whose design system they read.
    pub(super) fn design_reachable(&self) -> bool {
        self.prefs.idesign && !self.showing_group()
    }

    /// After a drop in the merged column `dir` that left it in `order`:
    /// renumbers only the projects whose own order changed, then keeps the
    /// interleaving. Returns the renames.
    pub(super) fn reorder_across(
        &mut self,
        dir: &Path,
        before: &[PathBuf],
        order: &[PathBuf],
    ) -> Result<Vec<(PathBuf, PathBuf)>, String> {
        let mut folders: Vec<&Path> = Vec::new();
        for parent in order.iter().filter_map(|path| path.parent()) {
            if !folders.contains(&parent) {
                folders.push(parent);
            }
        }
        let own = |list: &[PathBuf], folder: &Path| -> Vec<PathBuf> {
            let of = |path: &&PathBuf| path.parent() == Some(folder);
            list.iter().filter(of).cloned().collect()
        };
        let mut renames = Vec::new();
        for folder in folders {
            let after = own(order, folder);
            if own(before, folder) != after {
                renames.extend(priority::renumber(folder, &after)?);
            }
        }
        let renamed: Vec<PathBuf> = order
            .iter()
            .map(|path| match renames.iter().find(|(old, _)| old == path) {
                Some((_, new)) => new.clone(),
                None => path.clone(),
            })
            .collect();
        if let Some(file) = &self.group_order_file() {
            let mut stored = workspace::Order::load(Some(file));
            let keys = workspace::order::keys(&self.board, &renamed);
            stored.columns.insert(file_name(dir), keys);
            stored.save(file)?;
        }
        Ok(renames)
    }

    /// The boards `⌃⇥` goes through, in a steady order: each group, then
    /// the projects in the user's order.
    pub(super) fn project_cycle(&self) -> Vec<Target> {
        let groups = self
            .workspace
            .iter()
            .flat_map(|workspace| &workspace.groups)
            .map(|group| Target::Group(group.name.clone()));
        let projects = self.ordered_projects().into_iter().map(Target::Project);
        groups.chain(projects).collect()
    }

    /// `⌃⇥`: the next board of [`Self::project_cycle`], or with `forward`
    /// false the previous one, round the end.
    pub(super) fn switch_project(&mut self, forward: bool) {
        let cycle = self.project_cycle();
        let len = cycle.len();
        if len < 2 {
            return;
        }
        let current = self.current_target();
        let next = match cycle.iter().position(|board| *board == current) {
            Some(at) if forward => (at + 1) % len,
            Some(at) => (at + len - 1) % len,
            None => 0,
        };
        self.open_target(cycle[next].clone());
    }

    /// The project new issues go to on a group's board: the one the filter
    /// names, else the selected card's, else the first. `None` on a single project.
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

    /// `todo/` for a single project, `app/todo/` on a group's board:
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
