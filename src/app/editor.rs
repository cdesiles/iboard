//! The editor: autosave, a file changed on disk meanwhile, and its page.

use super::*;

/// How long after the last keystroke an edit is written to disk.
pub(super) const AUTOSAVE_AFTER: Duration = Duration::from_millis(800);

/// The in-progress edit of one issue.
pub(super) struct EditorState {
    pub(super) path: PathBuf,
    /// Current buffer, edited by the user.
    pub(super) text: String,
    /// Content as last seen on disk (loaded or saved), for dirty detection.
    baseline: String,
    /// File name as edited by the user.
    pub(super) name: String,
    /// File name as last seen on disk (loaded or saved).
    baseline_name: String,
    /// Newer on-disk content that arrived while we have unsaved edits.
    conflict: Option<String>,
    last_edit: Option<Instant>,
    pub(super) error: Option<String>,
}

impl EditorState {
    pub(super) fn new(path: PathBuf, content: String) -> Self {
        let name = file_name(&path);
        Self {
            path,
            baseline: content.clone(),
            text: content,
            baseline_name: name.clone(),
            name,
            conflict: None,
            last_edit: None,
            error: None,
        }
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.text != self.baseline || self.name != self.baseline_name
    }

    /// The buffer or the name just changed: autosave waits for a pause.
    pub(super) fn mark_edited(&mut self) {
        self.last_edit = Some(Instant::now());
    }

    /// The buffer becomes `text`, an unsaved edit: an agent's proposal.
    pub(super) fn replace(&mut self, text: String) {
        self.text = text;
        self.mark_edited();
    }

    /// Unsaved edits, no conflict, and no keystroke for `AUTOSAVE_AFTER`.
    pub(super) fn autosave_due(&self) -> bool {
        self.is_dirty()
            && self.conflict.is_none()
            && self
                .last_edit
                .is_some_and(|edit| edit.elapsed() >= AUTOSAVE_AFTER)
    }

    /// What the file now holds on disk: taken in when nothing is unsaved,
    /// kept as a conflict otherwise.
    fn adopt(&mut self, disk: &str) {
        if disk == self.baseline {
            return;
        }
        if self.is_dirty() {
            // Someone (or something) wrote while we have unsaved work.
            self.conflict = Some(disk.to_string());
        } else {
            // No local edits: quietly adopt the newest content.
            self.text = disk.to_string();
            self.baseline = disk.to_string();
            self.conflict = None;
        }
    }

    /// Writes the buffer if it is dirty, renaming the file when its name was
    /// edited too. With `force`, also overwrites a detected conflict ("Keep
    /// mine"). False when there was nothing to write, or when it failed,
    /// `error` saying why.
    fn write(&mut self, force: bool) -> bool {
        if !self.is_dirty() {
            return false;
        }
        if self.conflict.is_some() && !force {
            return false;
        }

        // Work out the target path from the (possibly edited) file name.
        if self.name.trim().is_empty() {
            self.error = Some("the file name cannot be empty".to_string());
            return false;
        }
        let desired = normalize_file_name(&self.name);
        let target = self.path.with_file_name(&desired);
        if target != self.path && target.exists() {
            self.error = Some(format!(
                "Couldn't rename to {desired}: that file already exists. Pick another name."
            ));
            return false;
        }

        // Save the content under the current name, then move the file if the
        // name changed. A refused rename leaves the saved content in place.
        if let Err(err) = atomic_write(&self.path, &self.text) {
            self.error = Some(format!("Couldn't save {}: {err}", file_name(&self.path)));
            return false;
        }
        if target != self.path {
            match std::fs::rename(&self.path, &target) {
                Ok(()) => {
                    self.path = target;
                    self.baseline_name = desired.clone();
                    self.name = desired;
                }
                Err(err) => {
                    self.error = Some(format!("Couldn't rename to {desired}: {err}"));
                    self.mark_saved();
                    return false;
                }
            }
        }

        self.mark_saved();
        if force {
            self.conflict = None;
        }
        self.error = None;
        true
    }

    /// The buffer is what the file holds.
    pub(super) fn mark_saved(&mut self) {
        self.baseline = self.text.clone();
        self.last_edit = None;
    }

    /// Drops the edits for what the file holds on disk ("Reload").
    fn reload(&mut self, disk: &str) {
        self.text = disk.to_string();
        self.baseline = disk.to_string();
        self.conflict = None;
        self.last_edit = None;
        self.error = None;
    }

    /// Drops unsaved edits, to the content and the file name.
    fn discard(&mut self) {
        self.text = self.baseline.clone();
        self.name = self.baseline_name.clone();
        self.conflict = None;
        self.last_edit = None;
        self.error = None;
    }
}

impl IboardApp {
    /// Notices external changes to the file being edited.
    pub(super) fn reconcile_editor(&mut self) {
        let Some(editor) = self.session.editor.as_mut() else {
            return;
        };
        let Some(issue) = self.board.find(&editor.path) else {
            return;
        };
        editor.adopt(&issue.markdown);
    }

    /// Writes the buffer if it is dirty, renaming the file and its images
    /// when its name was edited too. With `force`, also overwrites a
    /// detected conflict ("Keep mine").
    pub(super) fn flush_autosave(&mut self, force: bool) {
        let Some(editor) = self.session.editor.as_mut() else {
            return;
        };
        let old = editor.path.clone();
        if !editor.write(force) {
            return;
        }
        let new = editor.path.clone();
        if new != old {
            self.rename_images(&old, &new);
            self.apply_renames(&[(old, new)]);
        }
        self.refresh();
    }

    pub(super) fn reload_editor_from_disk(&mut self) {
        let Some(editor) = self.session.editor.as_mut() else {
            return;
        };
        let Some(issue) = self.board.find(&editor.path) else {
            return;
        };
        editor.reload(&issue.markdown);
    }

    /// Drops unsaved edits (content and file name) and returns to the reader.
    pub(super) fn discard_edits(&mut self) {
        if let Some(editor) = self.session.editor.as_mut() {
            editor.discard();
        }
        self.session.detail_mode = DetailMode::Preview;
    }

    pub(super) fn start_editing(&mut self, path: &Path) {
        if self.can_edit(path) {
            self.open_editor(path);
        }
    }

    /// The editor on `path`, whatever its column: what applying an agent's
    /// proposal needs, the save being the accept.
    pub(super) fn open_editor(&mut self, path: &Path) {
        if self
            .session
            .editor
            .as_ref()
            .map(|e| e.path == path)
            .unwrap_or(false)
        {
            self.session.detail_mode = DetailMode::Edit;
            return;
        }
        // Persist any pending edits of another issue before switching.
        self.save_pending();

        let Some(issue) = self.board.find(path) else {
            return;
        };
        let content = issue.markdown.clone();
        self.session.editor = Some(EditorState::new(path.to_path_buf(), content));
        self.session.screen = Screen::Detail(path.to_path_buf());
        self.session.detail_mode = DetailMode::Edit;
    }

    /// Saves unsaved edits unless they conflict with the file on disk.
    pub(super) fn save_pending(&mut self) {
        self.flush_autosave(false);
    }

    pub(super) fn has_unsaved_edits(&self) -> bool {
        self.session
            .editor
            .as_ref()
            .is_some_and(EditorState::is_dirty)
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
        let Some(editor) = self.session.editor.as_ref().filter(|e| e.path == path) else {
            return;
        };
        let (word, color) = save_state(editor, p);
        widgets::pill(ui, word, color);
    }

    /// Save in the Editor header (`⌘S` too): writes the draft and, when it
    /// went through, shows the issue again.
    pub(super) fn save_and_preview(&mut self) {
        self.flush_autosave(true);
        if self
            .session
            .editor
            .as_ref()
            .is_some_and(|e| e.error.is_none())
        {
            self.session.detail_mode = DetailMode::Preview;
        }
    }

    /// Opens the editor with `## <section>` appended, the caret under it.
    /// The draft stays unsaved until the user saves or Auto does.
    pub(super) fn add_section(&mut self, path: &Path, section: &str) {
        self.start_editing(path);
        if let Some(editor) = self
            .session
            .editor
            .as_mut()
            .filter(|editor| editor.path == path)
        {
            editor.text = sheet::append_section(&editor.text, section);
            self.session.focus_editor = true;
        }
    }

    /// The editor: the same page as the reader, in plain text, framed in
    /// `accent`. Above it: the file name, save state, Discard and Save.
    pub(super) fn detail_edit(&mut self, ui: &mut egui::Ui, path: &Path) {
        let focus_editor = std::mem::take(&mut self.session.focus_editor);
        let folder = path
            .parent()
            .map(|dir| self.relative(dir))
            .unwrap_or_default();
        let prompting = self.session.leave_prompt.is_some();
        let mut conflict = (false, false);
        let mut answer = None;
        page_column(ui, |ui| {
            let Some(editor) = self.session.editor.as_mut() else {
                ui.label("Nothing is being edited.");
                return;
            };
            editor_bar(ui, editor, &folder);
            if prompting {
                ui.add_space(theme::SPACE_3);
                answer = leave_prompt_row(ui, editor);
            }
            if let Some(error) = &editor.error {
                ui.add_space(theme::SPACE_2);
                ui.label(RichText::new(error).color(theme::palette(ui).danger));
            }
            if editor.conflict.is_some() {
                ui.add_space(theme::SPACE_2);
                conflict = conflict_row(ui, editor);
            }
            ui.add_space(theme::SPACE_3);
            editor_text(ui, editor, focus_editor);
        });
        let (reload, keep) = conflict;
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

/// The editor's bar: the folder and the file name, editable, with the save
/// state on the right. Save and Discard live in the header.
fn editor_bar(ui: &mut egui::Ui, editor: &mut EditorState, folder: &str) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new(folder).font(theme::meta()).color(p.ink_muted));
        let response = widgets::text_entry(
            ui,
            egui::TextEdit::singleline(&mut editor.name)
                .font(theme::meta())
                .text_color(p.ink)
                .hint_text("issue.md")
                .char_limit(files::MAX_FILE_NAME_BYTES)
                .margin(Margin::symmetric(theme::SPACE_2 as i8, 4))
                .desired_width(240.0),
        );
        if response.changed() {
            editor.mark_edited();
            ui.ctx().request_repaint_after(AUTOSAVE_AFTER);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (word, color) = save_state(editor, p);
            widgets::pill(ui, word, color);
        });
    });
}

/// "Save changes to x.md?" while leaving with unsaved edits: Save, Discard
/// or Keep editing.
fn leave_prompt_row(ui: &mut egui::Ui, editor: &EditorState) -> Option<LeaveChoice> {
    let p = theme::palette(ui);
    let mut answer = None;
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
        if widgets::button(ui, ButtonKind::Secondary, None, "Keep editing", Some("esc")).clicked() {
            answer = Some(LeaveChoice::KeepEditing);
        }
    });
    answer
}

/// The file changed on disk while it was edited: Reload or Keep mine.
fn conflict_row(ui: &mut egui::Ui, editor: &EditorState) -> (bool, bool) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{} changed on disk since you started editing.",
                editor.baseline_name
            ))
            .color(p.danger),
        );
        let reload = widgets::button(ui, ButtonKind::Secondary, None, "Reload", None)
            .on_hover_text("Drop your edits and load the file from disk")
            .clicked();
        let keep = widgets::button(ui, ButtonKind::Secondary, None, "Keep mine", None)
            .on_hover_text("Overwrite the file on disk with your version")
            .clicked();
        (reload, keep)
    })
    .inner
}

/// The editor's text field.
pub(super) const EDITOR_ID: &str = "editor_body";

/// The text itself, in a `focus`-edged sheet with its halo, filling the window. A freshly
/// created issue gets the caret after its title.
fn editor_text(ui: &mut egui::Ui, editor: &mut EditorState, focus: bool) {
    let p = theme::palette(ui);
    let rows = ((ui.available_height() - 2.0 * theme::SPACE_5) / 20.0).max(8.0) as usize;
    let sheet = Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.accent))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .show(ui, |ui| {
            theme::scroll_area(
                ui,
                egui::ScrollArea::vertical()
                    .id_salt("editor_scroll")
                    .content_margin(theme::scroll_margin(20.0, theme::SPACE_5))
                    .auto_shrink([false, false]),
                true,
                |ui| {
                    let id = egui::Id::new(EDITOR_ID);
                    let mut output = egui::TextEdit::multiline(&mut editor.text)
                        .id(id)
                        .font(theme::edit_body())
                        .code_editor()
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .desired_width(f32::INFINITY)
                        .desired_rows(rows)
                        .show(ui);
                    if focus {
                        let end = CCursor::new(editor.text.chars().count());
                        output
                            .state
                            .cursor
                            .set_char_range(Some(CCursorRange::one(end)));
                        output.state.store(ui.ctx(), id);
                        output.response.request_focus();
                    }
                    if output.response.changed() {
                        editor.mark_edited();
                        ui.ctx().request_repaint_after(AUTOSAVE_AFTER);
                    }
                },
            );
        });
    widgets::paint_focus_halo(ui, sheet.response.rect, theme::RADIUS_MD);
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
