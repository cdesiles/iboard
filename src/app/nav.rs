//! Where the user is: the screens, leaving the editor, and moving the selection.

use super::*;

impl IboardApp {
    /// Opens an issue in the reader. Editing is a separate step, entered from
    /// the detail view.
    pub(super) fn open_issue(&mut self, path: PathBuf) {
        // A composer with text is an unsaved issue: it waits on the board
        // until the user adds or closes it. Only an empty one goes.
        if self.composer.as_ref().is_some_and(Composer::is_empty) {
            self.composer = None;
        }
        self.cancel_review();
        self.selected = Some(path.clone());
        self.screen = Screen::Detail(path);
        self.detail_mode = DetailMode::Preview;
    }

    /// Leaves the current screen for `to`. From the editor, unsaved edits are
    /// saved when Auto is on; otherwise, or when saving fails, the editor bar
    /// asks first and nothing moves until the user answers.
    pub(super) fn leave(&mut self, to: Leave) {
        let editing =
            matches!(self.screen, Screen::Detail(_)) && self.detail_mode == DetailMode::Edit;
        if editing {
            if self.auto {
                self.save_pending();
            }
            if self.has_unsaved_edits() {
                self.leave_prompt = Some(to);
                return;
            }
        }
        self.go(to);
    }

    pub(super) fn go(&mut self, to: Leave) {
        self.leave_prompt = None;
        match to {
            Leave::Reader => self.detail_mode = DetailMode::Preview,
            Leave::Board => {
                self.detail_mode = DetailMode::Preview;
                self.screen = Screen::Board;
            }
            Leave::Composer => {
                self.detail_mode = DetailMode::Preview;
                self.screen = Screen::Board;
                self.show_composer();
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
        let Some(to) = self.leave_prompt.take() else {
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
                    .filter(|issue| self.filter.matches(issue))
                    .map(|issue| issue.path.clone())
                    .collect()
            })
            .collect()
    }

    /// Moves the board selection one step with an arrow key.
    pub(super) fn move_selection(&mut self, key: Key) {
        if self.focused_column.is_some() {
            self.move_focus_selection(key);
            return;
        }
        let grid = self.visible_grid();
        let at = self.selected.as_ref().and_then(|selected| {
            grid.iter().enumerate().find_map(|(column, paths)| {
                paths
                    .iter()
                    .position(|path| path == selected)
                    .map(|row| (column, row))
            })
        });
        if let Some((column, row)) = step_selection(&grid, at, key) {
            self.selected = Some(grid[column][row].clone());
            self.scroll_to_selected = true;
        }
    }

    /// Moves the selection inside a focused column: `↑ ↓` step through its
    /// sorted cards, `← →` switch to the neighbouring column.
    pub(super) fn move_focus_selection(&mut self, key: Key) {
        match key {
            Key::ArrowLeft => self.focus_neighbor(-1),
            Key::ArrowRight => self.focus_neighbor(1),
            _ => {
                let Some(dir) = self.focused_column.clone() else {
                    return;
                };
                let Some(column) = self.board.columns.iter().find(|column| column.dir == dir)
                else {
                    return;
                };
                let paths: Vec<PathBuf> =
                    focus_issues(column, &self.filter, self.column_sort(&dir))
                        .into_iter()
                        .map(|issue| issue.path.clone())
                        .collect();
                if paths.is_empty() {
                    return;
                }
                let at = self
                    .selected
                    .as_ref()
                    .and_then(|selected| paths.iter().position(|path| path == selected));
                let next = match key {
                    Key::ArrowUp => at.map(|at| at.saturating_sub(1)).or(Some(0)),
                    _ => match at {
                        Some(at) if at + 1 < paths.len() => Some(at + 1),
                        Some(at) => Some(at),
                        None => Some(0),
                    },
                };
                if let Some(next) = next {
                    self.selected = Some(paths[next].clone());
                    self.scroll_to_selected = true;
                }
            }
        }
    }

    /// Shows the column `delta` tabs away from the focused one, wrapping.
    pub(super) fn focus_neighbor(&mut self, delta: isize) {
        let Some(dir) = self.focused_column.clone() else {
            return;
        };
        let count = self.board.columns.len();
        if count == 0 {
            return;
        }
        let Some(index) = self
            .board
            .columns
            .iter()
            .position(|column| column.dir == dir)
        else {
            return;
        };
        let next = (index as isize + delta).rem_euclid(count as isize) as usize;
        self.focused_column = Some(self.board.columns[next].dir.clone());
    }

    /// The selected issue, if it is still on the board.
    pub(super) fn selected_issue(&self) -> Option<PathBuf> {
        self.selected
            .clone()
            .filter(|path| self.board.find(path).is_some())
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
