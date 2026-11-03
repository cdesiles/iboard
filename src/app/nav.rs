//! Where the user is: the screens, leaving the editor, and moving the selection.

use super::*;

impl IboardApp {
    /// Opens an issue in the reader. Editing is a separate step, entered from
    /// the detail view.
    pub(super) fn open_issue(&mut self, path: PathBuf) {
        // An open NewIssueModal is closed: its notes are parked.
        if self.composer_open() {
            self.close_composer();
        }
        self.close_review();
        self.session.picked.clear();
        self.session.selected = Some(path.clone());
        self.session.screen = Screen::Detail(path.clone());
        self.session.detail_mode = DetailMode::Preview;
        // A review that finished while the issue was closed shows now, and so
        // does the conversation of an ask answered meanwhile.
        if self.session.review_runs.is_ready(&path) {
            self.reveal_review(&path);
        } else if self.session.answered.remove(&path) && !self.ask_open(&path) {
            self.open_ask(&path);
        }
    }

    /// Leaves the current screen for `to`. From the editor, unsaved edits are
    /// saved when Auto is on; otherwise, or when saving fails, the editor bar
    /// asks first and nothing moves until the user answers.
    pub(super) fn leave(&mut self, to: Leave) {
        let editing = matches!(self.session.screen, Screen::Detail(_))
            && self.session.detail_mode == DetailMode::Edit;
        if editing {
            if self.auto {
                self.save_pending();
            }
            if self.has_unsaved_edits() {
                self.session.leave_prompt = Some(to);
                return;
            }
        }
        self.go(to);
    }

    pub(super) fn go(&mut self, to: Leave) {
        self.session.leave_prompt = None;
        match to {
            Leave::Reader => self.session.detail_mode = DetailMode::Preview,
            Leave::Board => {
                self.session.detail_mode = DetailMode::Preview;
                self.session.screen = Screen::Board;
            }
            Leave::Composer => {
                self.session.detail_mode = DetailMode::Preview;
                self.session.screen = Screen::Board;
                self.show_composer();
            }
            Leave::Open(target) => {
                self.session.detail_mode = DetailMode::Preview;
                self.session.screen = Screen::Board;
                self.open_target(target);
            }
            Leave::Quit => {
                self.closing = true;
                self.send_close = true;
            }
        }
    }

    /// Ends the process once quitting has been agreed.
    ///
    /// On macOS, letting eframe close the window destroys the `NSWindow`, and
    /// AppKit's Touch Bar finder then races the teardown and aborts the process
    /// on Touch Bar Macs: a double `removeObserver` in
    /// `_NSTouchBarFinderObservation` raises `NSInternalInconsistencyException`
    /// (`SIGABRT`). Exiting here skips that teardown; unsaved edits were just
    /// saved or discarded, and writes are atomic, so there is nothing to flush.
    pub(super) fn quit(&mut self, _ctx: &egui::Context) {
        process::stop_all();
        #[cfg(not(target_os = "macos"))]
        _ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        #[cfg(target_os = "macos")]
        std::process::exit(0);
    }

    /// Applies the answer to the save prompt.
    pub(super) fn answer_leave(&mut self, choice: LeaveChoice) {
        let Some(to) = self.session.leave_prompt.take() else {
            return;
        };
        match choice {
            LeaveChoice::Save => {
                self.flush_autosave(true);
                // A failed save stays in the editor, showing why.
                if !self.has_unsaved_edits() {
                    self.go(to);
                }
            }
            LeaveChoice::Discard => {
                self.discard_edits();
                self.go(to);
            }
            LeaveChoice::KeepEditing => {}
        }
    }

    pub(super) fn show_preview(&mut self) {
        self.leave(Leave::Reader);
    }

    pub(super) fn back_to_board(&mut self) {
        self.leave(Leave::Board);
    }

    /// The issues the board shows, column by column, after the filter.
    pub(super) fn visible_grid(&self) -> Vec<Vec<PathBuf>> {
        self.board
            .columns
            .iter()
            .map(|column| {
                column
                    .issues
                    .iter()
                    .filter(|issue| self.filter_bar.filter.matches(issue))
                    .map(|issue| issue.path.clone())
                    .collect()
            })
            .collect()
    }

    /// Moves the board selection one step with an arrow key.
    pub(super) fn move_selection(&mut self, key: Key) {
        let grid = self.visible_grid();
        let at = self.session.selected.as_ref().and_then(|selected| {
            grid.iter().enumerate().find_map(|(column, paths)| {
                paths
                    .iter()
                    .position(|path| path == selected)
                    .map(|row| (column, row))
            })
        });
        if let Some((column, row)) = step_selection(&grid, at, key) {
            self.session.selected = Some(grid[column][row].clone());
            self.session.scroll_to_selected = true;
        }
    }

    /// The selected issue, if it is still on the board.
    pub(super) fn selected_issue(&self) -> Option<PathBuf> {
        self.session
            .selected
            .clone()
            .filter(|path| self.board.find(path).is_some())
    }

    /// A plain click: the card becomes the only one selected, without opening.
    pub(super) fn select(&mut self, path: PathBuf) {
        self.session.picked.clear();
        self.session.selected = Some(path);
    }

    /// Shift-click: a picked card leaves the multi-selection, any other joins
    /// it, along with the card selected before.
    pub(super) fn pick(&mut self, path: PathBuf) {
        let picked = &mut self.session.picked;
        if let Some(at) = picked.iter().position(|p| *p == path) {
            picked.remove(at);
            self.session.selected = picked.last().cloned();
            return;
        }
        if picked.is_empty()
            && let Some(selected) = self.session.selected.take()
            && selected != path
        {
            picked.push(selected);
        }
        picked.push(path.clone());
        self.session.selected = Some(path);
    }

    /// `Space` on the board: `path` joins the multi-selection, or leaves it.
    pub(super) fn toggle_pick(&mut self, path: PathBuf) {
        let picked = &mut self.session.picked;
        match picked.iter().position(|p| *p == path) {
            Some(at) => {
                picked.remove(at);
            }
            None => picked.push(path),
        }
    }

    /// What an action on `path` applies to: the whole multi-selection when
    /// `path` is in it, in board order; `path` alone otherwise.
    pub(super) fn selection_of(&self, path: &Path) -> Vec<PathBuf> {
        if !self.session.picked.iter().any(|p| p == path) {
            return vec![path.to_path_buf()];
        }
        self.board
            .columns
            .iter()
            .flat_map(|column| &column.issues)
            .map(|issue| &issue.path)
            .filter(|p| self.session.picked.contains(p))
            .cloned()
            .collect()
    }
}

/// The next selected `(column, row)` after an arrow key on the board's visible
/// issues. With nothing selected, any arrow selects the first card. Up and down
/// stop at the column's ends; left and right skip empty columns and keep the
/// row when the next column is long enough.
pub(super) fn step_selection(
    grid: &[Vec<PathBuf>],
    at: Option<(usize, usize)>,
    key: Key,
) -> Option<(usize, usize)> {
    let Some((column, row)) = at else {
        return grid
            .iter()
            .position(|paths| !paths.is_empty())
            .map(|column| (column, 0));
    };
    let land = |column: usize| (column, row.min(grid[column].len() - 1));
    match key {
        Key::ArrowUp => Some((column, row.saturating_sub(1))),
        Key::ArrowDown => Some((column, (row + 1).min(grid[column].len() - 1))),
        Key::ArrowLeft => (0..column)
            .rev()
            .find(|&c| !grid[c].is_empty())
            .map(land)
            .or(at),
        Key::ArrowRight => (column + 1..grid.len())
            .find(|&c| !grid[c].is_empty())
            .map(land)
            .or(at),
        _ => at,
    }
}
