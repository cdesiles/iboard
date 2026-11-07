//! The header bar: the breadcrumb, the project menu and the toolbar.

use super::board::column_status;
use super::labelling::set_label_anchor;
use super::{
    Align, ButtonKind, IboardApp, Icon, Layout, Path, PathBuf, Pos2, RichText, Screen, Sense,
    Stroke, UNIFIED_TITLEBAR, Vec2, file_name, theme, widgets,
};
use widgets::DynamicButton;

mod crumbs;
mod more;

/// An issue's dynamic icon buttons, in the order they are laid out.
#[derive(Debug, PartialEq)]
enum Detail {
    Commits,
    Delete,
    AddLabel,
    Edit,
}

/// Delete, Add label and Edit, laid out right to left. The Editor keeps only
/// Delete. A Done issue is archived, never deleted, and shows Commits instead.
fn detail_buttons(done: bool, reading: bool, editable: bool) -> Vec<Detail> {
    match (done, reading) {
        (true, true) => vec![Detail::Commits],
        (true, false) => vec![],
        (false, true) if editable => vec![Detail::Delete, Detail::AddLabel, Detail::Edit],
        (false, _) => vec![Detail::Delete],
    }
}

impl IboardApp {
    /// The 40pt top bar: breadcrumb on the left, the screen's controls on the
    /// right with its main action rightmost. On macOS it is also the window's
    /// title bar: dragging it moves the window, double-clicking zooms it.
    pub(super) fn header_bar(&mut self, ui: &mut egui::Ui) {
        if UNIFIED_TITLEBAR {
            // Interacted with first, so the controls drawn over it win. It
            // runs under the traffic lights too, as a native title bar does.
            let bar = ui.interact(
                ui.max_rect().with_min_x(ui.ctx().content_rect().left()),
                egui::Id::unique("header_drag"),
                Sense::click_and_drag(),
            );
            if bar.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if bar.double_clicked() {
                let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
        }
        ui.style_mut().interaction.selectable_labels = false;
        // Controls first, so the breadcrumb truncates into the space left.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            self.header_controls(ui);
            ui.add_space(theme::SPACE_5);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                self.breadcrumb(ui);
            });
        });
    }

    /// The header's buttons, laid out right to left.
    pub(super) fn header_controls(&mut self, ui: &mut egui::Ui) {
        if self.session.stats.is_some() {
            self.stats_controls(ui);
            return;
        }
        match self.session.screen.clone() {
            Screen::Board => self.board_controls(ui),
            Screen::Detail(path, _) => self.detail_controls(ui, &path),
            Screen::Diff => self.refresh_button(ui),
        }
    }

    /// The board's toolbar: New issue, Refresh, Design and the filter.
    fn board_controls(&mut self, ui: &mut egui::Ui) {
        let has_todo = self.board.todo_column().is_some();
        let new_issue = ui.add_enabled_ui(has_todo, |ui| {
            widgets::button(
                ui,
                ButtonKind::Primary,
                Some(Icon::Plus),
                "New issue",
                Some("⌘N"),
            )
            .on_hover_text("Add an issue to Todo  ⌘N")
        });
        if new_issue.inner.clicked() {
            self.open_composer();
        }
        self.refresh_button(ui);
        if self.design_reachable()
            && widgets::button(ui, ButtonKind::Header, None, "Design", None)
                .on_hover_text("Browse the project's design system")
                .clicked()
        {
            self.design_button(ui.visuals().dark_mode);
        }
        self.filter_field(ui);
    }

    /// An issue's toolbar, right to left: Save in the Editor, the Reader's `…`
    /// menu, the AI split button by column, then the dynamic icon buttons.
    fn detail_controls(&mut self, ui: &mut egui::Ui, path: &Path) {
        let path = path.to_path_buf();
        let exists = self.board.find(&path).is_some();
        // The leave prompt's Save is the screen's one fill while it asks.
        let save = match self.session.leave_prompt {
            Some(_) => ButtonKind::Secondary,
            None => ButtonKind::Primary,
        };
        if self.session.editing().is_some()
            && exists
            && widgets::button(ui, save, None, "Save", Some("⌘S"))
                .on_hover_text("Save this issue  ⌘S")
                .clicked()
        {
            self.save_and_preview();
        }
        if self.session.reading().is_some() {
            self.more_button(ui, &path, exists);
        }
        self.ai_button(ui, &path);
        if exists {
            let gap = std::mem::replace(&mut ui.spacing_mut().item_spacing.x, theme::SPACE_1);
            self.dynamic_buttons(ui, &path);
            ui.spacing_mut().item_spacing.x = gap;
        }
    }

    /// The buttons `detail_buttons` picks. Delete has no key in the Editor,
    /// since `Del` edits text there.
    fn dynamic_buttons(&mut self, ui: &mut egui::Ui, path: &Path) {
        let reading = self.session.reading().is_some();
        let buttons = detail_buttons(self.is_done_issue(path), reading, self.can_edit(path));
        for button in buttons {
            match button {
                Detail::Commits => self.commits_button(ui, path),
                Detail::Delete => {
                    let delete = DynamicButton::new(Icon::Trash, "Delete")
                        .key(reading.then_some("Del"))
                        .danger();
                    if ui.add(delete).clicked() {
                        self.request_delete(path.to_path_buf());
                    }
                }
                Detail::AddLabel => self.add_label_button(ui, path),
                Detail::Edit => {
                    let edit = DynamicButton::new(Icon::Edit, "Edit").key(Some("E"));
                    if ui.add(edit).clicked() {
                        self.start_editing(path);
                    }
                }
            }
        }
    }

    /// Add label, `accent` while its picker shows, which hangs from it.
    fn add_label_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let open = self
            .session
            .label_picker
            .as_ref()
            .is_some_and(|picker| picker.path == path);
        let add = DynamicButton::new(Icon::TagPlus, "Add label")
            .key(Some("L"))
            .open(open);
        let add = ui.add(add);
        set_label_anchor(ui, path, add.rect);
        if add.clicked() {
            self.toggle_label_picker(path.to_path_buf());
        }
    }

    /// Commits, `accent` while its popup shows, and the newest commit's hash.
    fn commits_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let open = self
            .session
            .commits
            .as_ref()
            .is_some_and(|panel| panel.path == *path);
        let button = ui.add(DynamicButton::new(Icon::Commit, "Commits").open(open));
        if let Some(hash) = self.latest_commit(path, ui.ctx()) {
            let p = theme::palette(ui);
            ui.add_space(theme::SPACE_2);
            ui.label(RichText::new(hash).font(theme::meta()).color(p.ink_muted))
                .on_hover_text("Newest commit naming this ticket");
        }
        if button.clicked() {
            self.open_commits(path);
        }
    }

    /// `⌘R` reloads, except on a Todo issue, where it refines.
    pub(super) fn refresh_key(&self, path: &Path) -> &'static str {
        if self.is_todo_issue(path) { "" } else { "⌘R" }
    }

    /// Only offered when Settings › General turns Auto off: with Auto on the
    /// board already reloads itself. An issue's is a row of its `…` menu.
    pub(super) fn refresh_button(&mut self, ui: &mut egui::Ui) {
        if self.prefs.auto {
            return;
        }
        if widgets::button(
            ui,
            ButtonKind::Header,
            Some(Icon::Refresh),
            "Refresh",
            Some("⌘R"),
        )
        .on_hover_text("Reload from disk  ⌘R")
        .clicked()
        {
            self.refresh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Detail, detail_buttons};

    #[test]
    fn detail_buttons_follow_the_column_and_the_mode() {
        let all = vec![Detail::Delete, Detail::AddLabel, Detail::Edit];
        assert_eq!(detail_buttons(false, true, true), all);
        assert_eq!(detail_buttons(false, true, false), vec![Detail::Delete]);
        assert_eq!(detail_buttons(false, false, true), vec![Detail::Delete]);
        assert_eq!(detail_buttons(true, true, false), vec![Detail::Commits]);
        assert_eq!(detail_buttons(true, false, false), vec![]);
    }
}
