//! Moving, reordering, deleting and archiving issue files, and undoing a delete.

use super::{
    Confirm, Deleted, IboardApp, Instant, Path, PathBuf, Removal, Screen, Session, Undo,
    atomic_write, discovery, file_name, images, trash,
};
use crate::priority;

impl IboardApp {
    /// Moves a dragged issue into another column's directory, keeping its file
    /// name apart from the Ready priority number (see
    /// [`priority::moved_name`]). An existing file with the same name is never
    /// overwritten. On a workspace board the issue stays in its own project.
    /// Returns the issue's new path on success.
    pub(super) fn move_issue(&mut self, path: &Path, target_dir: &Path) -> Option<PathBuf> {
        let planned = self
            .board
            .dir_for(path, target_dir)
            .and_then(|dir| priority::plan_move(path, &dir));
        let target = match planned {
            Ok(target) if target == path => return Some(target),
            Ok(target) => target,
            Err(err) => {
                self.error = Some(format!("Couldn't move {}: {err}", self.relative(path)));
                return None;
            }
        };
        match std::fs::rename(path, &target) {
            Ok(()) => {
                self.apply_renames(&[(path.to_path_buf(), target.clone())]);
                self.refresh();
                Some(target)
            }
            Err(err) => {
                self.error = Some(format!("Couldn't move {}: {err}", self.relative(path)));
                None
            }
        }
    }

    /// Handles a card drop: move it between columns when needed, then, in a
    /// file-name ordered column (Todo, Ready), renumber the files so the drop
    /// position becomes the priority. `anchor` is the card the dropped one
    /// lands in front of, or `None` for the end.
    /// A picked card carries the whole multi-selection with it, in board
    /// order, landing together in front of `anchor`.
    pub(super) fn place_issue(&mut self, path: &Path, target_dir: &Path, anchor: Option<PathBuf>) {
        let paths = self.selection_of(path);
        let orderable = discovery::is_orderable_column(target_dir);
        if !orderable
            && paths
                .iter()
                .all(|path| self.board.in_column(path, target_dir))
        {
            return;
        }
        let moved: Vec<PathBuf> = paths
            .iter()
            .filter_map(|path| self.move_issue(path, target_dir))
            .collect();
        if orderable && !moved.is_empty() {
            self.reorder_column(target_dir, &moved, anchor.as_deref());
        }
    }

    /// Move to… in the Reader: `path` goes to the end of the column `dir`,
    /// and the Reader follows it there.
    pub(super) fn send_to(&mut self, path: &Path, dir: &Path) {
        self.session.picked.clear();
        self.place_issue(path, dir, None);
    }

    /// `⌘↩` on the board: moves the selected ticket to the next column, left
    /// to right. A ticket in the last column stays put.
    pub(super) fn move_selection_to_next(&mut self) {
        let Some(path) = self.selected_issue() else {
            return;
        };
        let Some(index) = self
            .board
            .columns
            .iter()
            .position(|column| column.issues.iter().any(|issue| issue.path == path))
        else {
            return;
        };
        let Some(target) = self
            .board
            .columns
            .get(index + 1)
            .map(|column| column.dir.clone())
        else {
            return;
        };
        self.place_issue(&path, &target, None);
    }

    /// Renumbers an ordered column so `moved` sits in front of `anchor`, keeping
    /// every other issue in its current order. An anchor among the moved
    /// gives way to the next issue that is not.
    pub(super) fn reorder_column(&mut self, dir: &Path, moved: &[PathBuf], anchor: Option<&Path>) {
        if moved.len() == 1 && anchor == Some(moved[0].as_path()) {
            return;
        }
        self.refresh();
        let Some(column) = self
            .board
            .columns
            .iter()
            .find(|column| column.dir.as_path() == dir)
        else {
            return;
        };
        let all: Vec<PathBuf> = column.issues.iter().map(|t| t.path.clone()).collect();
        let order = priority::reorder(&all, moved, anchor);
        let renamed = match self.showing_group() {
            true => self.reorder_across(dir, &all, &order),
            false => priority::renumber_each(&order),
        };
        match renamed {
            Ok(renames) => self.apply_renames(&renames),
            Err(err) => self.error = Some(err),
        }
        self.refresh();
    }

    /// Follows renamed files in everything that holds an issue (see
    /// [`Session::rename`]).
    pub(super) fn apply_renames(&mut self, renames: &[(PathBuf, PathBuf)]) {
        for (old, new) in renames {
            self.session.rename(old, new);
        }
    }

    /// Every delete lands here: the trash icons, the Delete key and button,
    /// and the Delete drop zone. It asks first unless Settings › General says
    /// not to; then the file goes at once and `⌘Z` can bring it back. Done
    /// issues are archived, never deleted.
    /// A picked issue takes the whole multi-selection with it.
    pub(super) fn request_delete(&mut self, path: PathBuf) {
        let paths: Vec<PathBuf> = self
            .selection_of(&path)
            .into_iter()
            .filter(|path| !self.is_done_issue(path))
            .collect();
        if paths.is_empty() || self.asks_to_stop(Removal::Delete, &paths) {
            return;
        }
        if self.prefs.confirm_delete {
            self.session.confirm = Some(Confirm::Delete(paths));
            return;
        }
        let deleted: Vec<Deleted> = paths
            .into_iter()
            .filter_map(|path| {
                let content = std::fs::read(&path).unwrap_or_default();
                let (trashed, images) = self.delete_issue(&path)?;
                Some(Deleted {
                    path,
                    trashed,
                    content,
                    images,
                })
            })
            .collect();
        if !deleted.is_empty() {
            self.session.notice = Some((self.trashed_notice(&deleted), Instant::now()));
            self.session.undo = Some(Undo::Deleted(deleted));
        }
    }

    /// "Moved todo/a.md to the Trash", or how many went.
    pub(super) fn trashed_notice(&self, deleted: &[Deleted]) -> String {
        match deleted {
            [one] => format!("Moved {} to the Trash", self.relative(&one.path)),
            many => format!("Moved {} issues to the Trash", many.len()),
        }
    }

    /// The confirmed delete: every issue the dialog listed.
    pub(super) fn delete_issues(&mut self, paths: &[PathBuf]) {
        for path in paths {
            self.delete_issue(path);
        }
    }

    /// Before removing issues an agent works on, an ask or a build, asks
    /// to stop it. True when it asked.
    fn asks_to_stop(&mut self, removal: Removal, paths: &[PathBuf]) -> bool {
        let working = paths
            .iter()
            .any(|path| self.asking(path) || self.building_since(path).is_some());
        if working {
            self.session.confirm = Some(Confirm::Stop(removal, paths.to_vec()));
        }
        working
    }

    /// Removal agreed with agents at work: letting go of the issues stops
    /// them.
    pub(super) fn remove_anyway(&mut self, removal: Removal, paths: &[PathBuf]) {
        self.session.confirm = None;
        match removal {
            Removal::Delete => self.delete_issues(paths),
            Removal::Archive => self.archive_now(paths),
        }
    }

    /// `⌘Z` on the board: takes back the last delete made without asking,
    /// or the last split.
    pub(super) fn undo_last(&mut self) {
        match self.session.undo.take() {
            Some(Undo::Deleted(deleted)) => deleted
                .into_iter()
                .for_each(|deleted| self.undo_delete(deleted)),
            Some(Undo::Split(split)) => self.undo_split(split),
            None => {}
        }
    }

    /// Puts an issue deleted without asking back where it was.
    fn undo_delete(&mut self, deleted: Deleted) {
        if deleted.path.exists() {
            self.error = Some(format!(
                "Couldn't restore {}: a file with that name exists now.",
                self.relative(&deleted.path)
            ));
            return;
        }
        let restored = match &deleted.trashed {
            Some(trashed) if trash::restore(trashed, &deleted.path).is_ok() => Ok(()),
            _ => atomic_write(&deleted.path, &deleted.content),
        };
        match restored {
            Ok(()) => {
                if let Some(images) = deleted.images {
                    self.restore_images(images);
                }
                self.session.notice = Some((
                    format!("Restored {}", self.relative(&deleted.path)),
                    Instant::now(),
                ));
                self.refresh();
                self.session.selected = Some(deleted.path);
                self.session.scroll_to_selected = true;
            }
            Err(err) => {
                self.error = Some(format!(
                    "Couldn't restore {}: {err}",
                    self.relative(&deleted.path)
                ));
            }
        }
    }

    /// A press, of a key or the pointer, is the next action: what `⌘Z` could
    /// take back goes with it, before this frame's action sets its own.
    pub(super) fn forget_undo_on_action(&mut self, ctx: &egui::Context) {
        let acted = ctx.input(|i| {
            i.events.iter().any(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => !(*key == egui::Key::Z && modifiers.command),
                egui::Event::PointerButton { pressed: true, .. } => true,
                _ => false,
            })
        });
        if acted {
            self.session.undo = None;
        }
    }

    /// Deletes an issue's file and its images to `self.trash` and lets the
    /// board catch up. Returns where the Trash put the file (`None` when it
    /// can't say) and the images, or `None` when the delete failed.
    pub(super) fn delete_issue(
        &mut self,
        path: &Path,
    ) -> Option<(Option<PathBuf>, Option<images::TrashedImages>)> {
        self.session.confirm = None;
        self.session.undo = None;
        match self.trash.delete(path) {
            Ok(trashed) => {
                let images = self.trash_images(path);
                self.session.notice =
                    Some((format!("Deleted {}", file_name(path)), Instant::now()));
                self.session.forget(path);
                self.refresh();
                Some((trashed, images))
            }
            Err(err) => {
                self.error = Some(format!("Couldn't delete {}: {err}", self.relative(path)));
                None
            }
        }
    }

    /// Moves a done issue out of the board into `issues/.archive`, which
    /// discovery ignores. Never overwrites an archived file.
    /// A picked issue takes the done issues of the multi-selection with it.
    pub(super) fn archive_issue(&mut self, path: &Path) {
        let paths = self.selection_of(path);
        if paths.len() > 1 {
            let done: Vec<PathBuf> = paths
                .into_iter()
                .filter(|path| self.is_done_issue(path))
                .collect();
            self.archive_all(&done);
        } else if !self.asks_to_stop(Removal::Archive, &paths)
            && self.move_to_archive(path).is_some()
        {
            self.session.notice = Some((format!("Archived {}", file_name(path)), Instant::now()));
            self.refresh();
        }
    }

    /// "Archive all" in Done's header: moves every listed issue to
    /// `.archive/`, then reads the board once, and says how many went.
    pub(super) fn archive_all(&mut self, paths: &[PathBuf]) {
        if !self.asks_to_stop(Removal::Archive, paths) {
            self.archive_now(paths);
        }
    }

    fn archive_now(&mut self, paths: &[PathBuf]) {
        let archived = paths
            .iter()
            .filter(|path| self.move_to_archive(path).is_some())
            .count();
        self.refresh();
        if archived > 0 {
            let noun = if archived == 1 { "issue" } else { "issues" };
            self.session.notice = Some((format!("Archived {archived} {noun}"), Instant::now()));
        }
    }

    /// Moves `path` into `.archive/` and lets go of it wherever it is open,
    /// without reading the board again. Returns where it went; `None`, with
    /// the reason in `self.error`, when it could not.
    pub(super) fn move_to_archive(&mut self, path: &Path) -> Option<PathBuf> {
        let target = priority::archive_path(self.board.root_of(path), path);
        let archive = target.parent().unwrap_or(&target);
        if let Err(err) = std::fs::create_dir_all(archive) {
            self.error = Some(format!("Couldn't create {}: {err}", self.relative(archive)));
            return None;
        }
        if let Err(err) = std::fs::rename(path, &target) {
            self.error = Some(format!("Couldn't archive {}: {err}", self.relative(path)));
            return None;
        }
        self.session.forget(path);
        Some(target)
    }

    /// Moves a just-composed ticket to position 1, once the pointer has settled.
    /// Renumbering changes the cards' ids, so doing it while a click is still
    /// down would steal that click from the card underneath it.
    pub(super) fn flush_pending_front(&mut self, ctx: &egui::Context) {
        let Some((dir, path)) = self.session.pending_front.clone() else {
            return;
        };
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        self.session.pending_front = None;
        if self.board.find(&path).is_none() {
            return;
        }
        let anchor = self
            .board
            .todo_column()
            .and_then(|column| column.issues.first().map(|issue| issue.path.clone()))
            .filter(|first| first != &path);
        let Some(anchor) = anchor else {
            return;
        };
        self.reorder_column(&dir, &[path], Some(&anchor));
    }
}

impl Session {
    /// Every place that holds an issue's path: the selection and picks, the
    /// detail screen, the delete confirmation, the new card, pending front,
    /// and every panel open on one.
    pub(super) fn issue_paths_mut(&mut self) -> impl Iterator<Item = &mut PathBuf> {
        let open = match &mut self.screen {
            Screen::Detail(open, _) => Some(open),
            _ => None,
        };
        let confirm = match &mut self.confirm {
            Some(Confirm::Delete(paths)) => paths.as_mut_slice(),
            _ => &mut [],
        };
        [
            self.selected.as_mut(),
            open,
            self.new_card.as_mut().map(|(path, _)| path),
            self.pending_front.as_mut().map(|(_, path)| path),
            self.editor.as_mut().map(|editor| &mut editor.path),
            self.label_picker.as_mut().map(|picker| &mut picker.path),
            self.commits.as_mut().map(|panel| &mut panel.path),
            self.review.as_mut().map(|review| &mut review.path),
            self.refine.as_mut().map(|refine| &mut refine.path),
            self.notes.as_mut().map(|notes| &mut notes.path),
            self.enrichment
                .as_mut()
                .map(|enrichment| &mut enrichment.path),
            self.ask_draft.as_mut().map(|draft| &mut draft.path),
            self.agent_log.as_mut().map(|log| &mut log.path),
        ]
        .into_iter()
        .flatten()
        .chain(&mut self.picked)
        .chain(confirm)
    }

    /// Follows an issue renamed from `old` to `new` everywhere it is held,
    /// and in the reviews, asks and builds at work on it.
    pub(super) fn rename(&mut self, old: &Path, new: &Path) {
        self.review_runs.rename(old, new);
        if let Some(ask) = self.asks.remove(old) {
            self.asks.insert(new.to_path_buf(), ask);
        }
        if let Some(build) = self.build.as_mut() {
            build.rename(old, new);
        }
        if let Some(watch) = self.watch.as_mut() {
            watch.rename(old, new);
        }
        for path in self.issue_paths_mut().filter(|path| *path == old) {
            *path = new.to_path_buf();
        }
    }

    /// Lets go of an issue that left the board: closes everything open on
    /// it and stops its review run, its ask and its build agent, so none
    /// writes it back. An enrichment at work on it runs on.
    pub(super) fn forget(&mut self, path: &Path) {
        self.review_runs.forget(path);
        self.asks.remove(path);
        if let Some(build) = self.build.as_mut() {
            build.forget(path);
        }
        if self
            .review
            .as_ref()
            .is_some_and(|review| review.path == path)
            || self
                .refine
                .as_ref()
                .is_some_and(|refine| refine.path == path)
        {
            self.review = None;
            self.refine = None;
        }
        if matches!(&self.screen, Screen::Detail(open, _) if open == path) {
            self.screen = Screen::Board;
        }
        if let Some(Confirm::Delete(paths)) = self.confirm.as_mut() {
            paths.retain(|held| held != path);
            if paths.is_empty() {
                self.confirm = None;
            }
        }
        self.picked.retain(|picked| picked != path);
        self.selected.take_if(|selected| selected == path);
        self.new_card.take_if(|(held, _)| held == path);
        self.pending_front.take_if(|(_, held)| held == path);
        self.editor.take_if(|editor| editor.path == path);
        self.label_picker.take_if(|picker| picker.path == path);
        self.commits.take_if(|panel| panel.path == path);
        self.notes.take_if(|notes| notes.path == path);
        self.ask_draft.take_if(|draft| draft.path == path);
        self.agent_log.take_if(|log| log.path == path);
    }
}
