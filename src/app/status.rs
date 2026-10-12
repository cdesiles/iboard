//! The status bar, and the strip naming what failed.

use super::*;

impl IboardApp {
    /// The 24pt bottom strip: where you are on the left, what you can press on
    /// the right.
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.style_mut().interaction.selectable_labels = false;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_5;
            let meta = |text: String| RichText::new(text).font(theme::meta()).color(p.ink_muted);
            let hints: Vec<(&str, &str)>;
            match self.screen.clone() {
                Screen::Board => {
                    let count = self.board.issue_count();
                    let noun = if count == 1 { "issue" } else { "issues" };
                    let notice = self
                        .notice
                        .as_ref()
                        .filter(|(_, at)| at.elapsed() < NOTICE_FOR)
                        .map(|(text, at)| (text.clone(), *at));
                    if let Some(drag) = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx()) {
                        ui.label(meta(self.relative(&drag.path)));
                        hints = vec![("esc", "cancel drag")];
                    } else if let Some(undo) = self.undo.as_ref().filter(|_| self.can_undo()) {
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
                        hints = vec![("⌘Z", "undo"), ("⌘,", "settings")];
                    } else if let Some((text, at)) = notice {
                        ui.label(RichText::new(text).font(theme::meta()).color(p.ink));
                        ui.ctx()
                            .request_repaint_after(NOTICE_FOR.saturating_sub(at.elapsed()));
                        hints = vec![("⌘N", "new"), ("⌘F", "filter"), ("⌘R", "refresh")];
                    } else if let Some(dir) = self.focused_column.clone() {
                        // ColumnFocus: what is shown, and how it is sorted.
                        let sort = self.column_sort(&dir);
                        let n = self
                            .board
                            .columns
                            .iter()
                            .find(|column| column.dir == dir)
                            .map(|column| column.issues.len())
                            .unwrap_or(0);
                        let noun = if n == 1 { "issue" } else { "issues" };
                        ui.label(meta(format!(
                            "{} · {} {} · sorted by {}",
                            self.relative(&dir),
                            n,
                            noun,
                            sort.label().to_lowercase()
                        )));
                        hints = vec![("↩", "open"), ("⌘1–4", "column"), ("esc", "board")];
                    } else if self.composer.is_some()
                        && let Some(todo) = self.board.todo_column()
                    {
                        ui.label(meta(self.relative(&todo.dir)));
                        hints = vec![("↩", "add"), ("⇧↩", "new line"), ("esc", "close")];
                    } else if let Some(selected) = self.selected_issue() {
                        ui.label(meta(self.relative(&selected)));
                        let done = self
                            .board
                            .find(&selected)
                            .is_some_and(|issue| issue.done_at.is_some());
                        hints = if self.can_edit(&selected) {
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
                        };
                    } else {
                        hints = vec![("⌘N", "new"), ("⌘F", "filter"), ("⌘R", "refresh")];
                    }
                    if self.focused_column.is_none() {
                        ui.label(meta(format!("{count} {noun}")));
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
                }
                Screen::Detail(path) => {
                    ui.label(meta(self.relative(&path)));
                    if self.detail_mode == DetailMode::Edit {
                        self.save_pill(ui, &path);
                        hints = if self.leave_prompt.is_some() {
                            vec![("esc", "keep editing")]
                        } else {
                            vec![("⌘S", "save"), ("esc", "leave")]
                        };
                    } else {
                        if let Some(modified) = self.board.find(&path).and_then(|t| t.modified) {
                            ui.label(meta(format!("modified {}", relative_time(modified))));
                        }
                        hints = if self.can_edit(&path) {
                            vec![("E", "edit"), ("Del", "delete"), ("esc", "back")]
                        } else {
                            vec![("Del", "delete"), ("esc", "back")]
                        };
                    }
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                for (key, action) in hints.into_iter().rev() {
                    ui.horizontal(|ui| widgets::hint(ui, key, action));
                }
            });
        });
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
