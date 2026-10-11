//! The editor: autosave, a file changed on disk meanwhile, and its page.

use super::*;

impl IboardApp {
    /// Notices external changes to the file being edited.
    pub(super) fn reconcile_editor(&mut self) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let Some(issue) = self.board.find(&editor.path) else {
            return;
        };
        let disk = &issue.markdown;
        if disk == &editor.baseline {
            return;
        }
        if editor.is_dirty() {
            // Someone (or something) wrote while we have unsaved work.
            editor.conflict = Some(disk.clone());
        } else {
            // No local edits: quietly adopt the newest content.
            editor.text = disk.clone();
            editor.baseline = disk.clone();
            editor.conflict = None;
        }
    }

    /// Writes the buffer if it is dirty, renaming the file when its name was
    /// edited too. With `force`, also overwrites a detected conflict ("Keep
    /// mine").
    pub(super) fn flush_autosave(&mut self, force: bool) {
        let mut renamed_to: Option<PathBuf> = None;
        let saved = {
            let Some(editor) = self.editor.as_mut() else {
                return;
            };
            if !editor.is_dirty() {
                return;
            }
            if editor.conflict.is_some() && !force {
                return;
            }

            // Work out the target path from the (possibly edited) file name.
            if editor.name.trim().is_empty() {
                editor.error = Some("the file name cannot be empty".to_string());
                return;
            }
            let desired = normalize_file_name(&editor.name);
            let target = editor.path.with_file_name(&desired);
            if target != editor.path && target.exists() {
                editor.error = Some(format!(
                    "Couldn't rename to {desired}: that file already exists. Pick another name."
                ));
                return;
            }

            // Save the content under the current name, then move the file if the
            // name changed. A refused rename leaves the saved content in place.
            if let Err(err) = atomic_write(&editor.path, &editor.text) {
                editor.error = Some(format!("Couldn't save {}: {err}", file_name(&editor.path)));
                return;
            }
            if target != editor.path {
                match std::fs::rename(&editor.path, &target) {
                    Ok(()) => {
                        editor.path = target.clone();
                        editor.baseline_name = desired.clone();
                        editor.name = desired.clone();
                        renamed_to = Some(target);
                    }
                    Err(err) => {
                        editor.error = Some(format!("Couldn't rename to {desired}: {err}"));
                        editor.baseline = editor.text.clone();
                        editor.last_edit = None;
                        editor.saved_flash = Some(Instant::now());
                        return;
                    }
                }
            }

            editor.baseline = editor.text.clone();
            if force {
                editor.conflict = None;
            }
            editor.last_edit = None;
            editor.saved_flash = Some(Instant::now());
            editor.error = None;
            true
        };
        if let Some(target) = renamed_to {
            // Keep the reader and the selection on the issue's new file name.
            self.selected = Some(target.clone());
            if let Screen::Detail(open) = &mut self.screen
                && open != &target
            {
                *open = target;
            }
        }
        if saved {
            self.refresh();
        }
    }

    pub(super) fn reload_editor_from_disk(&mut self) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let Some(issue) = self.board.find(&editor.path) else {
            return;
        };
        editor.text = issue.markdown.clone();
        editor.baseline = issue.markdown.clone();
        editor.conflict = None;
        editor.last_edit = None;
        editor.error = None;
    }

    /// Drops unsaved edits (content and file name) and returns to the reader.
    pub(super) fn discard_edits(&mut self) {
        if let Some(editor) = self.editor.as_mut() {
            editor.text = editor.baseline.clone();
            editor.name = editor.baseline_name.clone();
            editor.conflict = None;
            editor.last_edit = None;
            editor.error = None;
        }
        self.detail_mode = DetailMode::Preview;
    }

    pub(super) fn start_editing(&mut self, path: &Path) {
        if !self.can_edit(path) {
            return;
        }
        if self
            .editor
            .as_ref()
            .map(|e| e.path == path)
            .unwrap_or(false)
        {
            self.detail_mode = DetailMode::Edit;
            return;
        }
        // Persist any pending edits of another issue before switching.
        self.save_pending();

        let Some(issue) = self.board.find(path) else {
            return;
        };
        let content = issue.markdown.clone();
        self.editor = Some(EditorState::new(path.to_path_buf(), content));
        self.screen = Screen::Detail(path.to_path_buf());
        self.detail_mode = DetailMode::Edit;
    }

    /// Saves unsaved edits unless they conflict with the file on disk.
    pub(super) fn save_pending(&mut self) {
        let should_save = self
            .editor
            .as_ref()
            .map(|e| e.is_dirty() && e.conflict.is_none())
            .unwrap_or(false);
        if should_save {
            self.flush_autosave(false);
        }
    }

    pub(super) fn has_unsaved_edits(&self) -> bool {
        self.editor.as_ref().is_some_and(EditorState::is_dirty)
    }

    /// Writes an issue the reader changed via a checkbox.
    pub(super) fn write_issue(&mut self, path: &Path, contents: &str) {
        match atomic_write(path, contents) {
            Ok(()) => self.refresh(),
            Err(err) => {
                self.error = Some(format!("Couldn't save {}: {err}", self.relative(path)));
            }
        }
    }

    /// Whether `path` can be edited: Doing and Done tickets are the agent's
    /// and the finished ones, so they open read-only.
    pub(super) fn can_edit(&self, path: &Path) -> bool {
        self.board
            .column_of(path)
            .is_some_and(|column| discovery::is_editable_column(&column.dir))
    }

    /// Saved / Unsaved / Save failed for the issue being edited.
    pub(super) fn save_pill(&self, ui: &mut egui::Ui, path: &Path) {
        let p = theme::palette(ui);
        let Some(editor) = self.editor.as_ref().filter(|e| e.path == path) else {
            return;
        };
        let (word, color) = save_state(editor, p);
        widgets::pill(ui, word, color);
    }

    /// Save in the Editor header (`⌘S` too): writes the draft and, when it
    /// went through, shows the issue again.
    pub(super) fn save_and_preview(&mut self) {
        self.flush_autosave(true);
        if self.editor.as_ref().is_some_and(|e| e.error.is_none()) {
            self.detail_mode = DetailMode::Preview;
        }
    }

    /// Opens the editor with `## <section>` appended, the caret under it.
    /// The draft stays unsaved until the user saves or Auto does.
    pub(super) fn add_section(&mut self, path: &Path, section: &str) {
        self.start_editing(path);
        if let Some(editor) = self.editor.as_mut().filter(|editor| editor.path == path) {
            editor.text = sheet::append_section(&editor.text, section);
            self.focus_editor = true;
        }
    }

    /// The editor: the same page as the reader, in plain text, framed in
    /// `accent`. Above it: the file name, save state, Discard and Save.
    pub(super) fn detail_edit(&mut self, ui: &mut egui::Ui, path: &Path) {
        let p = theme::palette(ui);
        let focus_editor = std::mem::take(&mut self.focus_editor);
        let folder = path
            .parent()
            .map(|dir| self.relative(dir))
            .unwrap_or_default();
        let mut reload = false;
        let mut keep = false;
        let mut answer: Option<LeaveChoice> = None;
        let prompting = self.leave_prompt.is_some();

        page_column(ui, |ui| {
            let Some(editor) = self.editor.as_mut() else {
                ui.label("Nothing is being edited.");
                return;
            };

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(RichText::new(folder).font(theme::meta()).color(p.ink_muted));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut editor.name)
                        .font(theme::meta())
                        .text_color(p.ink)
                        .hint_text("issue.md")
                        .char_limit(files::MAX_FILE_NAME_BYTES)
                        .margin(Margin::symmetric(theme::SPACE_2 as i8, 4))
                        .desired_width(240.0),
                );
                if response.changed() {
                    editor.last_edit = Some(Instant::now());
                    ui.ctx().request_repaint_after(AUTOSAVE_AFTER);
                }
                // Save and Discard live in the header; the bar keeps the
                // file and its save state.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (word, color) = save_state(editor, p);
                    widgets::pill(ui, word, color);
                });
            });

            if prompting {
                ui.add_space(theme::SPACE_3);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    ui.label(
                        RichText::new(format!("Save changes to {}?", editor.baseline_name))
                            .font(theme::ui_strong())
                            .color(p.ink),
                    );
                    if widgets::button(ui, ButtonKind::Primary, None, "Save", None).clicked() {
                        answer = Some(LeaveChoice::Save);
                    }
                    if widgets::button(ui, ButtonKind::Secondary, None, "Discard", None)
                        .on_hover_text("Drop your unsaved changes and leave")
                        .clicked()
                    {
                        answer = Some(LeaveChoice::Discard);
                    }
                    if widgets::button(ui, ButtonKind::Secondary, None, "Keep editing", Some("esc"))
                        .clicked()
                    {
                        answer = Some(LeaveChoice::KeepEditing);
                    }
                });
            }
            if let Some(error) = &editor.error {
                ui.add_space(theme::SPACE_2);
                ui.label(RichText::new(error).color(p.danger));
            }
            if editor.conflict.is_some() {
                ui.add_space(theme::SPACE_2);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} changed on disk since you started editing.",
                            editor.baseline_name
                        ))
                        .color(p.danger),
                    );
                    reload = widgets::button(ui, ButtonKind::Secondary, None, "Reload", None)
                        .on_hover_text("Drop your edits and load the file from disk")
                        .clicked();
                    keep = widgets::button(ui, ButtonKind::Secondary, None, "Keep mine", None)
                        .on_hover_text("Overwrite the file on disk with your version")
                        .clicked();
                });
            }
            ui.add_space(theme::SPACE_3);

            let rows = ((ui.available_height() - 2.0 * theme::SPACE_5) / 20.0).max(8.0) as usize;
            Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(2.0, p.accent))
                .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                .inner_margin(Margin::symmetric(20, theme::SPACE_5 as i8))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("editor_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let id = egui::Id::new("editor_body");
                            let mut output = egui::TextEdit::multiline(&mut editor.text)
                                .id(id)
                                .font(theme::edit_body())
                                .code_editor()
                                .frame(Frame::NONE)
                                .margin(Margin::ZERO)
                                .desired_width(f32::INFINITY)
                                .desired_rows(rows)
                                .show(ui);
                            if focus_editor {
                                // A freshly created issue: caret after the title.
                                let end = CCursor::new(editor.text.chars().count());
                                output
                                    .state
                                    .cursor
                                    .set_char_range(Some(CCursorRange::one(end)));
                                output.state.store(ui.ctx(), id);
                                output.response.request_focus();
                            }
                            if output.response.changed() {
                                editor.last_edit = Some(Instant::now());
                                ui.ctx().request_repaint_after(AUTOSAVE_AFTER);
                            }
                        });
                });
        });

        if reload {
            self.reload_editor_from_disk();
        }
        if keep {
            self.flush_autosave(true);
        }
        if let Some(choice) = answer {
            self.answer_leave(choice);
        }
    }
}

/// The editor's save state as a pill word and its colour.
pub(super) fn save_state(editor: &EditorState, p: &Palette) -> (&'static str, Color32) {
    if editor.error.is_some() {
        ("Save failed", p.danger)
    } else if editor.conflict.is_some() {
        ("Changed on disk", p.danger)
    } else if editor.is_dirty() {
        ("Unsaved", p.status_doing)
    } else {
        ("Saved", p.status_done)
    }
}
