//! The status bar, and the strip naming what failed.

use super::*;

impl IboardApp {
    /// The 24pt bottom strip: where you are on the left, what you can press on
    /// the right.
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().interaction.selectable_labels = false;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_5;
            let hints = match self.session.screen.clone() {
                Screen::Board => self.board_status(ui),
                Screen::Detail(path) => self.detail_status(ui, &path),
                Screen::Diff => self.diff_status(ui),
            };
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                for (key, action) in hints.into_iter().rev() {
                    ui.horizontal(|ui| widgets::hint(ui, key, action));
                }
            });
        });
    }

    /// The board's status: what the last action did or what is selected,
    /// the issue count and the board's warnings. Returns its key hints.
    fn board_status(&mut self, ui: &mut egui::Ui) -> Vec<Hint> {
        let p = theme::palette(ui);
        let hints = self.board_message(ui);
        if self.column_focus.column.is_none() {
            let count = self.board.issue_count();
            let noun = if count == 1 { "issue" } else { "issues" };
            ui.label(status_meta(ui, format!("{count} {noun}")));
        }
        if !self.board.warnings.is_empty() {
            let n = self.board.warnings.len();
            let noun = if n == 1 { "warning" } else { "warnings" };
            ui.label(
                RichText::new(format!("{n} {noun}"))
                    .font(theme::meta())
                    .color(p.status_doing),
            )
            .on_hover_text(self.board.warnings.join("\n"));
        }
        hints
    }

    /// The board's first status line, by priority: a drag, an undo, a
    /// notice, ColumnFocus, the composer, the selection.
    fn board_message(&mut self, ui: &mut egui::Ui) -> Vec<Hint> {
        let p = theme::palette(ui);
        let notice = self
            .session
            .notice
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NOTICE_FOR)
            .map(|(text, at)| (text.clone(), *at));
        let idle = vec![("⌘N", "new"), ("⌘F", "filter"), ("⌘R", "refresh")];
        if let Some(drag) = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx()) {
            ui.label(status_meta(ui, self.relative(&drag.path)));
            return vec![("esc", "cancel drag")];
        }
        if let Some(undo) = self.session.undo.as_ref().filter(|_| self.can_undo()) {
            let text = match undo {
                Undo::Deleted(deleted) => {
                    format!("Moved {} to the Trash", self.relative(&deleted.path))
                }
                Undo::Split(split) => format!(
                    "Split {} into {} tickets",
                    file_name(&split.original),
                    split.children.len()
                ),
            };
            ui.label(RichText::new(text).font(theme::meta()).color(p.ink));
            ui.ctx()
                .request_repaint_after(UNDO_FOR.saturating_sub(undo.at().elapsed()));
            return vec![("⌘Z", "undo"), ("⌘,", "settings")];
        }
        if let Some((text, at)) = notice {
            ui.label(RichText::new(text).font(theme::meta()).color(p.ink));
            ui.ctx()
                .request_repaint_after(NOTICE_FOR.saturating_sub(at.elapsed()));
            return idle;
        }
        if let Some(dir) = self.column_focus.column.clone() {
            // ColumnFocus: what is shown, and how it is sorted.
            let n = self
                .board
                .columns
                .iter()
                .find(|column| column.dir == dir)
                .map(|column| column.issues.len())
                .unwrap_or(0);
            let noun = if n == 1 { "issue" } else { "issues" };
            let sort = self.column_sort(&dir).label().to_lowercase();
            let text = format!("{} · {n} {noun} · sorted by {sort}", self.relative(&dir));
            ui.label(status_meta(ui, text));
            return vec![("↩", "open"), ("⌘1–4", "column"), ("esc", "board")];
        }
        if self.session.composer.is_some()
            && let Some(todo) = self.board.todo_column()
        {
            ui.label(status_meta(ui, self.relative(&todo.dir)));
            return vec![("↩", "add"), ("⇧↩", "new line"), ("esc", "close")];
        }
        let Some(selected) = self.selected_issue() else {
            return idle;
        };
        ui.label(status_meta(ui, self.relative(&selected)));
        let done = self
            .board
            .find(&selected)
            .is_some_and(|issue| issue.done_at.is_some());
        if self.can_edit(&selected) {
            vec![
                ("↩", "open"),
                ("E", "edit"),
                ("⌘↩", "move"),
                ("Del", "delete"),
            ]
        } else if done {
            vec![("↩", "open"), ("A", "archive"), ("Del", "delete")]
        } else {
            vec![("↩", "open"), ("Del", "delete")]
        }
    }

    /// The Diff screen's status: its commits and their size.
    fn diff_status(&mut self, ui: &mut egui::Ui) -> Vec<Hint> {
        if let Some(view) = &self.session.diff {
            ui.label(status_meta(ui, view.summary()));
        }
        vec![
            ("⌘F", "find"),
            ("n p", "file"),
            ("] [", "hunk"),
            ("esc", "back"),
        ]
    }

    /// The open issue's status: its file, then the save state in the Editor
    /// or when it was modified in the Reader. Returns its key hints.
    fn detail_status(&mut self, ui: &mut egui::Ui, path: &Path) -> Vec<Hint> {
        ui.label(status_meta(ui, self.relative(path)));
        if self.session.detail_mode == DetailMode::Edit {
            self.save_pill(ui, path);
            return if self.session.leave_prompt.is_some() {
                vec![("esc", "keep editing")]
            } else {
                vec![("⌘S", "save"), ("esc", "leave")]
            };
        }
        if let Some(modified) = self.board.find(path).and_then(|t| t.modified) {
            ui.label(status_meta(
                ui,
                format!("modified {}", relative_time(modified)),
            ));
        }
        if self.can_edit(path) {
            vec![("E", "edit"), ("Del", "delete"), ("esc", "back")]
        } else {
            vec![("Del", "delete"), ("esc", "back")]
        }
    }

    /// A dismissible `danger` line naming what failed.
    pub(super) fn error_strip(&mut self, ui: &mut egui::Ui, error: &str) {
        let p = theme::palette(ui);
        Frame::new()
            .inner_margin(Margin::symmetric(
                theme::SPACE_5 as i8,
                theme::SPACE_3 as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(error).color(p.danger));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::button(ui, ButtonKind::Ghost, None, "Dismiss", None).clicked() {
                            self.error = None;
                            self.scan_error = None;
                        }
                    });
                });
            });
    }
}

/// A key hint of the status bar: the key, and what it does.
type Hint = (&'static str, &'static str);

/// Status bar text in `meta`, `ink-muted`.
fn status_meta(ui: &egui::Ui, text: String) -> RichText {
    RichText::new(text)
        .font(theme::meta())
        .color(theme::palette(ui).ink_muted)
}
