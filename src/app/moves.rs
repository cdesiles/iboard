//! Moving, reordering, deleting and archiving issue files, and undoing a delete.

use super::*;

impl IboardApp {
    /// Moves a dragged issue into another column's directory, keeping its file
    /// name apart from the Ready priority number (see
    /// [`discovery::moved_name`]). An existing file with the same name is never
    /// overwritten. Returns the issue's new path on success.
    pub(super) fn move_issue(&mut self, path: &Path, target_dir: &Path) -> Option<PathBuf> {
        let (Some(name), Some(from_dir)) = (path.file_name(), path.parent()) else {
            return None;
        };
        if from_dir == target_dir {
            return Some(path.to_path_buf());
        }
        let name = discovery::moved_name(&name.to_string_lossy(), from_dir, target_dir);
        let target = target_dir.join(&name);
        if target.exists() {
            self.error = Some(format!(
                "Couldn't move {}: {name} already exists in that column. Rename one of them first.",
                self.relative(path),
            ));
            return None;
        }
        match std::fs::rename(path, &target) {
            Ok(()) => {
                // Keep the reader/editor pointed at the issue's new home.
                if let Some(editor) = self.session.editor.as_mut()
                    && editor.path == path
                {
                    editor.path = target.clone();
                }
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
        if !orderable && paths.iter().all(|path| path.parent() == Some(target_dir)) {
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

    /// A drop on a column tab: the issue, or the multi-selection it is in,
    /// moves to the end of `to`.
    pub(super) fn move_issues(&mut self, path: &Path, to: &Path) {
        for path in self.selection_of(path) {
            if path.parent() != Some(to) {
                self.move_issue(&path, to);
            }
        }
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
        let anchor = anchor
            .and_then(|anchor| all.iter().position(|path| path == anchor))
            .and_then(|at| all[at..].iter().find(|path| !moved.contains(path)));
        let mut order: Vec<PathBuf> = all
            .iter()
            .filter(|path| !moved.contains(path))
            .cloned()
            .collect();
        let at = anchor
            .and_then(|anchor| order.iter().position(|path| path == anchor))
            .unwrap_or(order.len());
        order.splice(at..at, moved.iter().cloned());
        match discovery::renumber(dir, &order) {
            Ok(renames) => self.apply_renames(&renames),
            Err(err) => self.error = Some(err),
        }
        self.refresh();
    }

    /// Follows renamed files in everything that holds an issue: the
    /// selection, the reader and editor, and every panel open on one.
    pub(super) fn apply_renames(&mut self, renames: &[(PathBuf, PathBuf)]) {
        for (old, new) in renames {
            let follow = |path: &mut PathBuf| {
                if path == old {
                    *path = new.clone();
                }
            };
            self.session.review_runs.rename(old, new);
            if let Some(selected) = self.session.selected.as_mut() {
                follow(selected);
            }
            self.session.picked.iter_mut().for_each(follow);
            if let Screen::Detail(open) = &mut self.session.screen {
                follow(open);
            }
            if let Some(Confirm::Delete(paths)) = self.session.confirm.as_mut() {
                paths.iter_mut().for_each(follow);
            }
            if let Some((path, _)) = self.session.new_card.as_mut() {
                follow(path);
            }
            if let Some((_, path)) = self.session.pending_front.as_mut() {
                follow(path);
            }
            let paths = [
                self.session.editor.as_mut().map(|editor| &mut editor.path),
                self.session
                    .label_picker
                    .as_mut()
                    .map(|picker| &mut picker.path),
                self.session.commits.as_mut().map(|panel| &mut panel.path),
                self.session.review.as_mut().map(|review| &mut review.path),
                self.session.refine.as_mut().map(|refine| &mut refine.path),
                self.session.notes.as_mut().map(|notes| &mut notes.path),
                self.session
                    .enrichment
                    .as_mut()
                    .map(|enrichment| &mut enrichment.path),
            ];
            for path in paths.into_iter().flatten() {
                follow(path);
            }
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
        if paths.is_empty() {
            return;
        }
        if self.confirm_delete {
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
                    at: Instant::now(),
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

    /// `⌘Z` on the board: takes back the last delete made without asking,
    /// or the last split.
    pub(super) fn undo_last(&mut self) {
        match self
            .session
            .undo
            .take()
            .filter(|undo| undo.at().elapsed() < UNDO_FOR)
        {
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

    /// Whether `⌘Z` can still take something back.
    pub(super) fn can_undo(&self) -> bool {
        self.session
            .undo
            .as_ref()
            .is_some_and(|undo| undo.at().elapsed() < UNDO_FOR)
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
        if self
            .session
            .review
            .as_ref()
            .is_some_and(|review| review.path == path)
            || self
                .session
                .refine
                .as_ref()
                .is_some_and(|refine| refine.path == path)
        {
            self.close_review();
        }
        self.cancel_review_run(path);
        match self.trash.delete(path) {
            Ok(trashed) => {
                let images = self.trash_images(path);
                self.session.notice =
                    Some((format!("Deleted {}", file_name(path)), Instant::now()));
                self.let_go(path);
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
        } else if self.move_to_archive(path).is_some() {
            self.session.notice = Some((format!("Archived {}", file_name(path)), Instant::now()));
            self.refresh();
        }
    }

    /// "Archive all" in Done's header: moves every listed issue to
    /// `.archive/`, then reads the board once, and says how many went.
    pub(super) fn archive_all(&mut self, paths: &[PathBuf]) {
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
        if self
            .session
            .review
            .as_ref()
            .is_some_and(|review| review.path == path)
            || self
                .session
                .refine
                .as_ref()
                .is_some_and(|refine| refine.path == path)
        {
            self.close_review();
        }
        self.cancel_review_run(path);
        let archive = self.board.root.join(ARCHIVE_DIR);
        if let Err(err) = std::fs::create_dir_all(&archive) {
            self.error = Some(format!(
                "Couldn't create {}: {err}",
                self.relative(&archive)
            ));
            return None;
        }
        let target = archive.join(unique_name(&archive, &file_name(path)));
        if let Err(err) = std::fs::rename(path, &target) {
            self.error = Some(format!("Couldn't archive {}: {err}", self.relative(path)));
            return None;
        }
        self.let_go(path);
        Some(target)
    }

    /// Lets go of an issue that left the board: the selections, the editor
    /// and the reader.
    fn let_go(&mut self, path: &Path) {
        if self.session.selected.as_deref() == Some(path) {
            self.session.selected = None;
        }
        self.session.picked.retain(|picked| picked != path);
        if self.session.editor.as_ref().is_some_and(|e| e.path == path) {
            self.session.editor = None;
        }
        if matches!(&self.session.screen, Screen::Detail(open) if open == path) {
            self.session.detail_mode = DetailMode::Preview;
            self.session.screen = Screen::Board;
        }
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
