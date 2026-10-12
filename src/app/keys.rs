//! What each frame does before drawing: keys, shortcuts and background work.

use super::*;

impl IboardApp {
    /// The work each frame does before drawing: keys and shortcuts, menu
    /// requests, and what runs in the background. Tests drive it with no
    /// window.
    pub(super) fn frame_logic(&mut self, ctx: &egui::Context) {
        // A project switch sets the title; apply it now that we have a context.
        if let Some(title) = self.pending_title.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        // Adopt an agent's answer as soon as it lands.
        self.poll_review();
        self.poll_split();
        self.poll_naming(ctx);
        self.poll_design_draft(ctx);
        self.poll_enrichment(ctx);
        let typing = ctx.memory(|m| m.focused().is_some());
        let command = |key: Key| ctx.input(|i| i.modifiers.command && i.key_pressed(key));

        // A quit picked from the macOS menu arrives as a flag rather than a
        // window close, so it can run the save prompt before we exit.
        #[cfg(target_os = "macos")]
        if iboard::menu::take_quit_request() {
            self.leave(Leave::Quit);
        }
        #[cfg(target_os = "macos")]
        if iboard::menu::take_settings_request() && self.settings.is_none() {
            self.open_settings(self.settings_tab);
        }

        // Closing the window is one more way of leaving the editor.
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.closing {
            self.leave(Leave::Quit);
            if !self.closing {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        if std::mem::take(&mut self.send_close) {
            self.quit(ctx);
        }

        // The delete confirmation owns Escape and Enter while it is up.
        let confirming = self.confirm.is_some();
        if confirming {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.confirm = None;
            } else if ctx.input(|i| i.key_pressed(Key::Enter))
                && let Some(Confirm::Delete(path)) = self.confirm.clone()
            {
                self.delete_issue(&path);
            }
        }

        // The settings dialog owns the keyboard while it is up; `⌘,` closes
        // it as it opened it.
        if self.settings.is_some() {
            if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Comma)) {
                self.settings = None;
            }
            self.background(ctx);
            return;
        }

        // The LabelPicker closes first; what is left in its field is dropped.
        if !confirming
            && self.label_picker.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            self.label_picker = None;
        }
        // `L` adds a label to the issue the Reader shows (not in the Editor,
        // where labels are edited in the title).
        if !confirming
            && !typing
            && self.detail_mode == DetailMode::Preview
            && let Screen::Detail(path) = self.screen.clone()
            && self.board.find(&path).is_some()
            && ctx.input(|i| i.key_pressed(Key::L) && i.modifiers.is_none())
        {
            self.toggle_label_picker(path);
        }

        // In the filter, `esc` leaves the chips, then clears, then leaves the
        // field. egui drops focus on `esc` before we see it, so the field
        // asks for it back unless this is the last step.
        if !confirming
            && matches!(self.screen, Screen::Board)
            && self.filter_open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            if self.chip_cursor.take().is_some() {
                self.focus_filter = true;
            } else if !self.filter.is_empty() {
                self.filter.clear();
                self.focus_filter = true;
            } else {
                ctx.memory_mut(|m| m.surrender_focus(filter_id()));
                self.filter_open = false;
            }
        }

        // Escape steps back one level, but a review consumes it first: it
        // cancels a run, or closes a finished panel.
        if !confirming && ctx.input(|i| i.key_pressed(Key::Escape)) {
            if let Some(notes) = self.notes.as_mut().filter(|notes| notes.draft.is_some()) {
                notes.draft = None;
            } else if self.review.is_some() {
                self.cancel_review();
            } else {
                match self.screen.clone() {
                    _ if self.leave_prompt.is_some() => self.answer_leave(LeaveChoice::KeepEditing),
                    Screen::Board if self.composer.is_some() => self.composer_escape(),
                    Screen::Board if !self.filter.is_empty() => self.filter.clear(),
                    Screen::Board if self.focused_column.is_some() => self.unfocus_column(),
                    Screen::Board => self.selected = None,
                    Screen::Detail(_) if self.detail_mode == DetailMode::Edit => {
                        self.show_preview();
                    }
                    Screen::Detail(_) => self.back_to_board(),
                }
            }
        }

        if command(Key::N) {
            self.open_composer();
        }
        if command(Key::F) && matches!(self.screen, Screen::Board) {
            self.focus_filter = true;
        }
        // `⌘1`–`⌘4` show a column full screen (ColumnFocus); again goes back.
        if matches!(self.screen, Screen::Board) && !typing {
            for (at, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4]
                .into_iter()
                .enumerate()
            {
                if command(key)
                    && let Some(column) = self.board.columns.get(at)
                {
                    let dir = column.dir.clone();
                    if self.focused_column.as_ref() == Some(&dir) {
                        self.unfocus_column();
                    } else {
                        self.focus_column(dir);
                    }
                }
            }
        }
        if command(Key::R) && !ctx.input(|i| i.modifiers.shift) {
            self.refresh();
        }
        // `⌘Z` takes back a delete made without asking, or a split; in a
        // text field it stays the field's own undo.
        if command(Key::Z) && !typing && matches!(self.screen, Screen::Board) {
            self.undo_last();
        }
        // `⌘,` opens Settings on the last tab used; `⌘⇧R` reviews the open
        // issue or draft.
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Comma)) {
            self.open_settings(self.settings_tab);
        }
        if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::R))
            && let Screen::Detail(path) = self.screen.clone()
        {
            if self.review_is_running(&path) {
                self.cancel_review();
            } else {
                let draft = self.detail_mode == DetailMode::Edit;
                self.start_review(&path, draft);
            }
        }
        if command(Key::OpenBracket) {
            match self.screen {
                Screen::Detail(_) if self.detail_mode == DetailMode::Edit => self.show_preview(),
                Screen::Detail(_) => self.back_to_board(),
                Screen::Board => {}
            }
        }

        // `⌘S` is the header's Save: it forces a save (overwriting a
        // conflict) and, when it went through, shows the issue again.
        // On the board it adds the composer's issue, as `↩` does.
        if command(Key::S) {
            if matches!(self.screen, Screen::Board) && self.composer.is_some() {
                self.submit_composer(false);
            } else if self.detail_mode == DetailMode::Edit {
                self.save_and_preview();
            } else {
                self.flush_autosave(true);
            }
        }
        // Saved some other way while the prompt was up: carry on leaving.
        if let Some(to) = self.leave_prompt
            && !self.has_unsaved_edits()
        {
            self.go(to);
        }

        // On the board, the arrow keys select a card; `↩` opens it, `E` edits.
        if matches!(self.screen, Screen::Board) && !typing && !confirming {
            let no_modifiers = ctx.input(|i| i.modifiers.is_none());
            for key in [
                Key::ArrowUp,
                Key::ArrowDown,
                Key::ArrowLeft,
                Key::ArrowRight,
            ] {
                if no_modifiers && ctx.input(|i| i.key_pressed(key)) {
                    self.move_selection(key);
                }
            }
            if let Some(path) = self.selected_issue() {
                let done = self
                    .board
                    .find(&path)
                    .is_some_and(|issue| issue.done_at.is_some());
                let command_enter = self.composer.is_none()
                    && ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Enter));
                if no_modifiers && ctx.input(|i| i.key_pressed(Key::Enter)) {
                    self.open_issue(path);
                } else if command_enter {
                    self.move_selection_to_next();
                } else if no_modifiers && ctx.input(|i| i.key_pressed(Key::E)) {
                    self.start_editing(&path);
                } else if done && no_modifiers && ctx.input(|i| i.key_pressed(Key::A)) {
                    self.archive_issue(&path);
                } else if no_modifiers && ctx.input(|i| i.key_pressed(Key::L)) {
                    self.toggle_label_picker(path);
                }
            }
        }

        // `Del` (or Backspace) asks to delete the selected card, or the issue
        // the reader is showing. In the editor the key belongs to the text.
        if !confirming && !typing && self.confirm.is_none() {
            let delete_key = ctx.input(|i| {
                (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
                    && i.modifiers.is_none()
            });
            if delete_key {
                match self.screen.clone() {
                    Screen::Board => {
                        if let Some(path) = self.selected_issue() {
                            self.request_delete(path);
                        }
                    }
                    Screen::Detail(path) if self.detail_mode == DetailMode::Preview => {
                        self.request_delete(path);
                    }
                    _ => {}
                }
            }
        }

        if self.detail_mode == DetailMode::Preview
            && let Screen::Detail(path) = self.screen.clone()
        {
            // `E`, or a double-click on the page (not the header), edits.
            let editable = self.can_edit(&path);
            let edit_key = editable
                && !typing
                && ctx.input(|i| i.key_pressed(Key::E) && i.modifiers.is_none());
            let double_clicked_page = editable
                && !self.writing_note()
                && ctx.input(|i| {
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                        && i.pointer
                            .interact_pos()
                            .map(|pos| self.preview_rect.contains(pos))
                            .unwrap_or(false)
                });
            if edit_key || double_clicked_page {
                self.start_editing(&path);
            }
        }

        self.background(ctx);
    }

    /// The work each frame does whatever the keyboard said: autosave, the
    /// periodic refresh, and waking up for a running review.
    pub(super) fn background(&mut self, ctx: &egui::Context) {
        // Debounced autosave, while Auto is on.
        let due = self.auto
            && self
                .editor
                .as_ref()
                .map(|e| {
                    e.is_dirty()
                        && e.conflict.is_none()
                        && e.last_edit
                            .map(|t| t.elapsed() >= AUTOSAVE_AFTER)
                            .unwrap_or(false)
                })
                .unwrap_or(false);
        if due {
            self.flush_autosave(false);
        }

        // Auto reads the board again every RESCAN_EVERY, off the UI thread.
        if self.auto {
            if self.scan.is_none() && self.last_refresh.elapsed() >= RESCAN_EVERY {
                self.last_refresh = Instant::now();
                let located = self.located.clone();
                let rescan = Job::spawn(move |_| scan(&located));
                self.scan = Some((rescan, self.board_generation));
            }
            ctx.request_repaint_after(RESCAN_EVERY);
        }
        if let Some((rescan, generation)) = &self.scan {
            match rescan.answer() {
                Some(scanned) => {
                    let current = *generation == self.board_generation;
                    self.scan = None;
                    if current {
                        self.adopt(scanned);
                    }
                }
                None => ctx.request_repaint_after(job::POLL_EVERY),
            }
        }
        // Poll a running review a few times a second.
        if self.review.as_ref().is_some_and(|review| {
            matches!(
                review.status,
                ReviewStatus::Running { .. } | ReviewStatus::Splitting { .. }
            )
        }) {
            ctx.request_repaint_after(job::POLL_EVERY);
        }
    }
}
