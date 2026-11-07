//! The status bar, and the strip naming what failed.

use super::compose::DISCARD_UNDO;
use super::dock::IssueDrag;
use super::{
    Align, ButtonKind, Frame, IboardApp, Issue, Layout, NOTICE_FOR, Path, RichText, Screen, Undo,
    display_path, file_name, relative_time, sheet, theme, widgets,
};

/// Between a status bar item and the `·` that parts it from the next.
const STATUS_GAP: f32 = 6.0;

impl IboardApp {
    /// The 24pt bottom strip: where you are on the left and on the right the
    /// Editor's save state and, while ⌘ is held, what you can press.
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().interaction.selectable_labels = false;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = STATUS_GAP;
            let screen = self.session.screen.clone();
            let hints = match &screen {
                Screen::Board => self.board_status(ui),
                Screen::Detail(path, _) => self.detail_status(ui, path),
                Screen::Diff => self.diff_status(ui),
            };
            let editing = match &screen {
                Screen::Detail(path, _) if self.session.editing().is_some() => Some(path),
                _ => None,
            };
            let keys = widgets::key_hints_shown(ui.ctx());
            if editing.is_none() && !keys {
                return;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                if let Some(path) = editing {
                    self.save_pill(ui, path);
                }
                if keys {
                    widgets::sliding_hints(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                        for (key, action) in hints.into_iter().rev() {
                            ui.horizontal(|ui| widgets::hint(ui, key, action));
                        }
                    });
                }
            });
        });
    }

    /// The board's status: where it reads from or what just happened, its
    /// warnings and the AI build. Returns its key hints.
    fn board_status(&mut self, ui: &mut egui::Ui) -> Vec<Hint> {
        let p = theme::palette(ui);
        let hints = self.board_message(ui);
        if !self.board.warnings.is_empty() {
            let n = self.board.warnings.len();
            let noun = if n == 1 { "warning" } else { "warnings" };
            status_sep(ui);
            ui.label(status_meta(ui, format!("{n} {noun}")))
                .on_hover_text(self.board.warnings.join("\n"));
        }
        if let Some(status) = self.build_status() {
            status_sep(ui);
            let text = RichText::new(status).font(theme::meta()).color(p.accent);
            ui.add(egui::Label::new(text).truncate());
        }
        hints
    }

    /// The issues folder, or the projects of a group's board.
    fn issues_folder(&self) -> String {
        match self.shown_group() {
            Some(group) => group.count(),
            None => display_path(&self.board.root),
        }
    }

    /// While filtering: "4 of 9 issues match “bug”".
    fn filter_matches(&self) -> Option<String> {
        let filter = &self.filter_bar.filter;
        if filter.is_empty() {
            return None;
        }
        let total = self.board.issue_count();
        let issues = self.board.columns.iter().flat_map(|c| &c.issues);
        let shown = issues.filter(|issue| filter.matches(issue)).count();
        let noun = if total == 1 { "issue" } else { "issues" };
        let verb = if shown == 1 { "matches" } else { "match" };
        Some(format!(
            "{shown} of {total} {noun} {verb} “{}”",
            filter.describe()
        ))
    }

    /// The issues folder, then what the filter matches.
    fn folder_line(&self, ui: &mut egui::Ui) {
        ui.label(status_meta(ui, self.issues_folder()));
        if let Some(matches) = self.filter_matches() {
            status_sep(ui);
            ui.label(status_meta(ui, matches));
        }
    }

    /// The board's first status line, by priority: a drag, an undo, a
    /// notice, else the folder. The selection and the composer set the keys.
    fn board_message(&mut self, ui: &mut egui::Ui) -> Vec<Hint> {
        let notice = self
            .session
            .notice
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NOTICE_FOR)
            .map(|(text, at)| (text.clone(), *at));
        let mut idle = vec![("⌘N", "new"), ("⌘F", "filter")];
        if !self.prefs.auto {
            idle.push(("⌘R", "refresh"));
        }
        if let Some(drag) = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx()) {
            ui.label(status_meta(ui, self.relative(&drag.path)));
            return vec![("esc", "cancel drag")];
        }
        if let Some((_, at)) = self
            .session
            .discarded
            .as_ref()
            .filter(|(_, at)| at.elapsed() < DISCARD_UNDO)
        {
            ui.label(status_meta(ui, "Discarded the new issue · ⌘Z".to_owned()));
            ui.ctx()
                .request_repaint_after(DISCARD_UNDO.saturating_sub(at.elapsed()));
            return vec![("⌘Z", "undo"), ("⌘N", "new")];
        }
        if let Some(undo) = self.session.undo.as_ref() {
            let text = match undo {
                Undo::Deleted(deleted) => format!("{} · ⌘Z", self.trashed_notice(deleted)),
                Undo::Split(split) => format!(
                    "Split {} into {} tickets",
                    file_name(&split.original),
                    split.children.len()
                ),
            };
            ui.label(status_meta(ui, text));
            return vec![("⌘Z", "undo"), ("⌘,", "settings")];
        }
        if let Some((text, at)) = notice {
            ui.label(status_meta(ui, text));
            ui.ctx()
                .request_repaint_after(NOTICE_FOR.saturating_sub(at.elapsed()));
            return idle;
        }
        self.folder_line(ui);
        if self.composer_open() && self.board.todo_column().is_some() {
            return vec![("↩", "add"), ("⌘↩", "add & edit"), ("esc", "close")];
        }
        let Some(selected) = self.selected_issue() else {
            return idle;
        };
        let done = self
            .board
            .find(&selected)
            .is_some_and(|issue| issue.done_at.is_some());
        if self.can_edit(&selected) {
            vec![("↩", "open"), ("E", "edit"), ("⌘↩", "next column")]
        } else if done {
            vec![("↩", "open"), ("A", "archive")]
        } else if self.updatable(&selected) {
            vec![("↩", "open"), ("⌘U", "update"), ("Del", "delete")]
        } else {
            vec![("↩", "open"), ("Del", "delete")]
        }
    }

    /// The Diff screen's status: its commits and their size.
    fn diff_status(&mut self, ui: &mut egui::Ui) -> Vec<Hint> {
        if let Some(view) = &self.session.diff {
            ui.label(status_meta(ui, view.summary()));
        }
        vec![("⌘F", "find"), ("n p", "file"), ("esc", "back")]
    }

    /// The open issue's status: its file, then in the Reader when it was
    /// modified and how far it is. Returns its key hints.
    fn detail_status(&mut self, ui: &mut egui::Ui, path: &Path) -> Vec<Hint> {
        copyable_path(ui, self.relative(path), path);
        if self.session.editing().is_some() {
            return if self.session.leave_prompt.is_some() {
                vec![("esc", "keep editing")]
            } else {
                vec![("⌘S", "save"), ("esc", "leave")]
            };
        }
        if let Some(issue) = self.board.find(path) {
            reader_facts(ui, issue);
        }
        let todo = self
            .board
            .column_of(path)
            .is_some_and(|column| column.accepts_new_issues());
        if todo && self.prefs.agent.configured() {
            vec![("E", "edit"), ("esc", "board"), ("⌘⇧R", "review")]
        } else if !todo && self.can_edit(path) && self.updatable(path) {
            vec![("E", "edit"), ("esc", "board"), ("⌘U", "update")]
        } else if self.can_edit(path) {
            vec![("E", "edit"), ("esc", "board"), ("Del", "delete")]
        } else if self.is_done_issue(path) {
            vec![("esc", "board")]
        } else if self.updatable(path) {
            vec![("⌘U", "update"), ("Del", "delete"), ("esc", "board")]
        } else {
            vec![("Del", "delete"), ("esc", "board")]
        }
    }

    /// A dismissible `danger` line naming what failed, across the top. A
    /// project with no issues directory is offered one.
    pub(super) fn error_strip(&mut self, ui: &mut egui::Ui, error: &str) {
        let p = theme::palette(ui);
        let uninitialised = self
            .uninitialised
            .as_ref()
            .filter(|(_, raised)| raised == error)
            .map(|(project, _)| project.clone());
        Frame::new().inner_margin(theme::BAND_PAD).show(ui, |ui| {
            // Dismiss first, so a long error truncates before it rather
            // than pushing it out of the window.
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, ButtonKind::Ghost, None, "Dismiss", None).clicked() {
                        self.error = None;
                        self.uninitialised = None;
                        self.scan_error = None;
                    }
                    if let Some(project) = uninitialised
                        && widgets::button(ui, ButtonKind::Primary, None, "Create issues/", None)
                            .clicked()
                    {
                        self.initialise(project);
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let text = RichText::new(error).color(p.danger);
                        ui.add(egui::Label::new(text).truncate())
                            .on_hover_text(error);
                    });
                });
            });
        });
    }
}

/// A key hint of the status bar: the key, and what it does.
type Hint = (&'static str, &'static str);

/// What the Reader's status line says of an open issue after its path: "modified
/// 15d ago", then its checklist as a 48pt meter and "1/3 done", or with no
/// checkboxes its word count.
fn reader_facts(ui: &mut egui::Ui, issue: &Issue) {
    if let Some(modified) = issue.modified {
        status_sep(ui);
        let ago = format!("modified {}", relative_time(modified));
        ui.label(status_meta(ui, ago));
    }
    status_sep(ui);
    let (text, _) = sheet::hide_title_labels(&issue.markdown);
    let (ticked, total) = sheet::checklist(&text);
    if total > 0 {
        widgets::meter(ui, ticked, total);
        ui.label(status_meta(ui, format!("{ticked}/{total} done")));
    } else {
        let words = sheet::word_count(&text);
        let unit = if words == 1 { "word" } else { "words" };
        ui.label(status_meta(ui, format!("{words} {unit}")));
    }
}

/// The open issue's file, `shown` as its relative path: a click copies its
/// full path, and its tooltip says so until the pointer leaves.
fn copyable_path(ui: &mut egui::Ui, shown: String, path: &Path) {
    let id = egui::Id::unique(("copied-path", path));
    let response = ui
        .add(egui::Label::new(status_meta(ui, shown)).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        ui.ctx().copy_text(path.display().to_string());
        ui.data_mut(|d| d.insert_temp(id, true));
    } else if !response.hovered() {
        ui.data_mut(|d| d.remove::<bool>(id));
    }
    if ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        response.show_tooltip_text("Copied the full path");
    } else {
        response.on_hover_text("Click to copy the full path");
    }
}

/// The `·` between two status bar items, in `ink-faint`.
fn status_sep(ui: &mut egui::Ui) {
    let faint = theme::palette(ui).ink_faint;
    ui.label(RichText::new("·").font(theme::meta()).color(faint));
}

/// Status bar text in `meta`, `ink-muted`.
fn status_meta(ui: &egui::Ui, text: String) -> RichText {
    RichText::new(text)
        .font(theme::meta())
        .color(theme::palette(ui).ink_muted)
}
