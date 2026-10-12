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
                if let Some(editor) = self.editor.as_mut()
                    && editor.path == path
                {
                    editor.path = target.clone();
                }
                if let Screen::Detail(open) = &mut self.screen
                    && open == path
                {
                    *open = target.clone();
                }
                if self.selected.as_deref() == Some(path) {
                    self.selected = Some(target.clone());
                }
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
    pub(super) fn place_issue(&mut self, path: &Path, target_dir: &Path, anchor: Option<PathBuf>) {
        let same_dir = path.parent() == Some(target_dir);
        let orderable = discovery::is_orderable_column(target_dir);
        if same_dir && !orderable {
            return;
        }
        let moved = if same_dir {
            path.to_path_buf()
        } else {
            match self.move_issue(path, target_dir) {
                Some(moved) => moved,
                None => return,
            }
        };
        if orderable {
            self.reorder_column(target_dir, &moved, anchor.as_deref());
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
    /// every other issue in its current order. Returns `moved`'s path after the
    /// renumbering, which may have changed its prefix.
    pub(super) fn reorder_column(
        &mut self,
        dir: &Path,
        moved: &Path,
        anchor: Option<&Path>,
    ) -> Option<PathBuf> {
        if anchor == Some(moved) {
            return Some(moved.to_path_buf());
        }
        self.refresh();
        let column = self
            .board
            .columns
            .iter()
            .find(|column| column.dir.as_path() == dir)?;
        let mut order: Vec<PathBuf> = column.issues.iter().map(|t| t.path.clone()).collect();
        order.retain(|path| path != moved);
        match anchor.and_then(|anchor| order.iter().position(|path| path == anchor)) {
            Some(index) => order.insert(index, moved.to_path_buf()),
            None => order.push(moved.to_path_buf()),
        }
        let result = match discovery::renumber(dir, &order) {
            Ok(renames) => {
                let renamed = renames
                    .iter()
                    .find(|(old, _)| old == moved)
                    .map(|(_, new)| new.clone());
                self.apply_renames(&renames);
                renamed
            }
            Err(err) => {
                self.error = Some(err);
                None
            }
        };
        self.refresh();
        Some(result.unwrap_or_else(|| moved.to_path_buf()))
    }

    /// Follows renamed files in the selection, the reader and the editor.
    pub(super) fn apply_renames(&mut self, renames: &[(PathBuf, PathBuf)]) {
        for (old, new) in renames {
            if self.selected.as_deref() == Some(old.as_path()) {
                self.selected = Some(new.clone());
            }
            if let Some(editor) = self.editor.as_mut()
                && editor.path == *old
            {
                editor.path = new.clone();
            }
            if let Screen::Detail(open) = &mut self.screen
                && open == old
            {
                *open = new.clone();
            }
        }
    }

    /// Every delete lands here: the trash icons, the Delete key and button,
    /// and the Delete drop zone. It asks first unless Settings › General says
    /// not to; then the file goes at once and `⌘Z` can bring it back.
    pub(super) fn request_delete(&mut self, path: PathBuf) {
        if self.confirm_delete {
            self.confirm = Some(Confirm::Delete(path));
        } else {
            let content = std::fs::read(&path).unwrap_or_default();
            if let Some(trashed) = self.delete_issue(&path) {
                self.notice = Some((
                    format!("Moved {} to the Trash", self.relative(&path)),
                    Instant::now(),
                ));
                self.undo = Some(Undo::Deleted(Deleted {
                    path,
                    trashed,
                    content,
                    at: Instant::now(),
                }));
            }
        }
    }

    /// `⌘Z` on the board: takes back the last delete made without asking,
    /// or the last split.
    pub(super) fn undo_last(&mut self) {
        match self
            .undo
            .take()
            .filter(|undo| undo.at().elapsed() < UNDO_FOR)
        {
            Some(Undo::Deleted(deleted)) => self.undo_delete(deleted),
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
                self.notice = Some((
                    format!("Restored {}", self.relative(&deleted.path)),
                    Instant::now(),
                ));
                self.refresh();
                self.selected = Some(deleted.path);
                self.scroll_to_selected = true;
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
        self.undo
            .as_ref()
            .is_some_and(|undo| undo.at().elapsed() < UNDO_FOR)
    }

    /// Deletes an issue's file to `self.trash` and lets the board catch
    /// up. Returns where the Trash put it (`Some(None)` when it can't say),
    /// or `None` when the delete failed.
    pub(super) fn delete_issue(&mut self, path: &Path) -> Option<Option<PathBuf>> {
        self.confirm = None;
        self.undo = None;
        if self
            .review
            .as_ref()
            .is_some_and(|review| review.path == path)
        {
            self.cancel_review();
        }
        match self.trash.delete(path) {
            Ok(trashed) => {
                self.notice = Some((format!("Deleted {}", file_name(path)), Instant::now()));
                if self.selected.as_deref() == Some(path) {
                    self.selected = None;
                }
                if self.editor.as_ref().is_some_and(|e| e.path == path) {
                    self.editor = None;
                }
                if matches!(&self.screen, Screen::Detail(open) if open == path) {
                    self.detail_mode = DetailMode::Preview;
                    self.screen = Screen::Board;
                }
                self.refresh();
                Some(trashed)
            }
            Err(err) => {
                self.error = Some(format!("Couldn't delete {}: {err}", self.relative(path)));
                None
            }
        }
    }

    /// Moves a done issue out of the board into `issues/.archive`, which
    /// discovery ignores. Never overwrites an archived file.
    pub(super) fn archive_issue(&mut self, path: &Path) {
        if self.move_to_archive(path).is_some() {
            self.notice = Some((format!("Archived {}", file_name(path)), Instant::now()));
            self.refresh();
        }
    }

    /// "Archive all" at the foot of Done: moves every listed issue to
    /// `.archive/`, then reads the board once, and says how many went.
    pub(super) fn archive_all(&mut self, paths: &[PathBuf]) {
        let archived = paths
            .iter()
            .filter(|path| self.move_to_archive(path).is_some())
            .count();
        self.refresh();
        if archived > 0 {
            let noun = if archived == 1 { "issue" } else { "issues" };
            self.notice = Some((format!("Archived {archived} {noun}"), Instant::now()));
        }
    }

    /// Moves `path` into `.archive/` and lets go of it wherever it is open,
    /// without reading the board again. Returns where it went; `None`, with
    /// the reason in `self.error`, when it could not.
    pub(super) fn move_to_archive(&mut self, path: &Path) -> Option<PathBuf> {
        if self
            .review
            .as_ref()
            .is_some_and(|review| review.path == path)
        {
            self.cancel_review();
        }
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
        if self.selected.as_deref() == Some(path) {
            self.selected = None;
        }
        if self.editor.as_ref().is_some_and(|e| e.path == path) {
            self.editor = None;
        }
        if matches!(&self.screen, Screen::Detail(open) if open == path) {
            self.detail_mode = DetailMode::Preview;
            self.screen = Screen::Board;
        }
        Some(target)
    }

    /// Moves a just-composed ticket to position 1, once the pointer has settled.
    /// Renumbering changes the cards' ids, so doing it while a click is still
    /// down would steal that click from the card underneath it.
    pub(super) fn flush_pending_front(&mut self, ctx: &egui::Context) {
        let Some((dir, path)) = self.pending_front.clone() else {
            return;
        };
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        self.pending_front = None;
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
        let Some(new_path) = self.reorder_column(&dir, &path, Some(&anchor)) else {
            return;
        };
        if self
            .new_card
            .as_ref()
            .is_some_and(|(card, _)| card == &path)
        {
            self.new_card = Some((new_path, Instant::now()));
        }
    }
}
