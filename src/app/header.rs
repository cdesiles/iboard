//! The header bar: the breadcrumb, the project menu and the toolbar.

use super::board::column_status;
use super::labelling::set_label_anchor;
use super::{
    Align, ButtonKind, IboardApp, Icon, Layout, Path, PathBuf, Pos2, RichText, Screen, Sense,
    Stroke, UNIFIED_TITLEBAR, Vec2, file_name, theme, widgets,
};
use widgets::{DynamicButton, MenuAction};

/// The `…` menu, and the Move to… submenu in it: column names only.
const MORE_MENU_WIDTH: f32 = 220.0;
const MOVE_MENU_WIDTH: f32 = 180.0;

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
                egui::Id::new("header_drag"),
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

    /// Board: the project name. Reader and editor: a `‹ Board` button, then
    /// `project / Column`, each a link, and the issue's title once the page's
    /// own has scrolled away (always in the editor).
    pub(super) fn breadcrumb(&mut self, ui: &mut egui::Ui) {
        if self.session.stats.is_some() {
            self.stats_crumbs(ui);
            return;
        }
        let p = theme::palette(ui);
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let project = self.board.name();
        match self.session.screen.clone() {
            Screen::Board => {
                let open = self.project_menu.is_some();
                let response = widgets::project_trigger(ui, &project, open)
                    .on_hover_text("Switch project  ⌃⇥");
                if response.clicked() {
                    self.toggle_project_menu(ui.ctx());
                }
                if self.project_menu.is_some() {
                    self.show_project_menu(ui.ctx(), response.rect);
                }
            }
            Screen::Detail(path, _) => {
                let titled = self.session.editing().is_some()
                    || self.reader.title_away.as_deref() == Some(path.as_path());
                self.detail_breadcrumb(ui, &path, &project, titled);
            }
            Screen::Diff => {
                if let Some(path) = self.session.diff.as_ref().map(|view| view.issue.clone()) {
                    self.detail_breadcrumb(ui, &path, &project, true);
                    ui.add_space(widgets::CRUMB_PAD);
                    ui.label(RichText::new("/").color(p.ink_faint));
                    ui.add_space(widgets::CRUMB_PAD);
                    ui.label(RichText::new("Diff").font(theme::ui_strong()).color(p.ink));
                }
            }
        }
    }

    /// An issue's trail: Board, then `project / ○ Column`, each crumb leading
    /// back to the board, then the title when `titled`.
    fn detail_breadcrumb(&mut self, ui: &mut egui::Ui, path: &Path, project: &str, titled: bool) {
        let p = theme::palette(ui);
        // The visible way out, for whoever does not know the shortcuts.
        let hint = if self.session.editing().is_some() {
            "Back to the board, asking if unsaved  esc"
        } else {
            "Back to the board  esc"
        };
        let mut back =
            widgets::button(ui, ButtonKind::Back, Some(Icon::Back), "Board", Some("esc"))
                .on_hover_text(hint)
                .clicked();
        // A 16pt `line` divider between the button and the trail.
        let (divider, _) = ui.allocate_exact_size(Vec2::new(1.0, theme::CONTROL_H), Sense::hover());
        ui.painter().line_segment(
            [
                Pos2::new(divider.center().x, divider.center().y - 8.0),
                Pos2::new(divider.center().x, divider.center().y + 8.0),
            ],
            Stroke::new(1.0, p.line),
        );
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        let column = self
            .board
            .column_of(path)
            .map(|column| (column.name.clone(), column_status(column)));
        let title = self
            .board
            .find(path)
            .map(|t| t.title.clone())
            .unwrap_or_else(|| file_name(path));
        let separator = |ui: &mut egui::Ui| {
            ui.label(RichText::new("/").color(p.ink_faint));
        };
        back |= widgets::crumb(ui, project, theme::ui_body()).clicked();
        if let Some((column, mark)) = column {
            separator(ui);
            back |= widgets::crumb_marked(ui, &column, theme::ui_body(), Some(mark)).clicked();
        }
        if titled {
            separator(ui);
            ui.add_space(widgets::CRUMB_PAD);
            let title = RichText::new(title).font(theme::ui_strong()).color(p.ink);
            ui.add(egui::Label::new(title).truncate());
        }
        if back {
            self.back_to_board();
        }
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

    /// An issue's toolbar, right to left: Save in the Editor, the `…` menu, the
    /// AI split button by column, then the dynamic icon buttons.
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
        self.more_button(ui, &path, exists);
        self.ai_button(ui, &path);
        if exists {
            let gap = std::mem::replace(&mut ui.spacing_mut().item_spacing.x, theme::SPACE_1);
            self.dynamic_buttons(ui, &path);
            ui.spacing_mut().item_spacing.x = gap;
        }
    }

    /// Delete, Add label and Edit, laid out right to left. The Editor keeps
    /// only Delete, with no key since `Del` edits text there. A Done issue is
    /// archived, never deleted, and shows Commits instead.
    fn dynamic_buttons(&mut self, ui: &mut egui::Ui, path: &Path) {
        let reading = self.session.reading().is_some();
        if self.is_done_issue(path) {
            if reading {
                self.commits_button(ui, path);
            }
            return;
        }
        let delete = DynamicButton::new(Icon::Trash, "Delete")
            .key(reading.then_some("Del"))
            .danger();
        if ui.add(delete).clicked() {
            self.request_delete(path.to_path_buf());
        }
        if !reading || !self.can_edit(path) {
            return;
        }
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
        if ui
            .add(DynamicButton::new(Icon::Edit, "Edit").key(Some("E")))
            .clicked()
        {
            self.start_editing(path);
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

    /// What the `…` menu offers on `path`.
    fn more_menu(&self, path: &Path, exists: bool) -> MoreMenu {
        let reading = self.session.reading().is_some() && exists;
        let done = self.is_done_issue(path);
        let here = self.board.column_of(path).map(|column| column.dir.clone());
        let columns = self
            .board
            .columns
            .iter()
            .filter(|column| exists && Some(&column.dir) != here.as_ref())
            .map(|column| (column.name.clone(), column.dir.clone()))
            .collect();
        MoreMenu {
            columns,
            refresh: (!self.prefs.auto).then(|| self.refresh_key(path)),
            agent_log: reading && self.has_agent_log(path),
            draft_design: reading && !done && self.design_reachable(),
            archive: reading && done,
            discard: self.session.editing().is_some() && exists,
        }
    }

    /// `⌘R` reloads, except on a Todo issue, where it refines.
    pub(super) fn refresh_key(&self, path: &Path) -> &'static str {
        if self.is_todo_issue(path) { "" } else { "⌘R" }
    }

    /// The `…` button and its menu.
    fn more_button(&mut self, ui: &mut egui::Ui, path: &Path, exists: bool) {
        let menu = self.more_menu(path, exists);
        if menu.is_empty() {
            return;
        }
        let name = "More actions";
        let button = widgets::button(ui, ButtonKind::Ghost, Some(Icon::More), "", None)
            .on_hover_text("Move to…");
        button.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name)
        });
        let picked = egui::Popup::menu(&button)
            .width(MORE_MENU_WIDTH)
            .align(egui::RectAlign::BOTTOM_END)
            .show(|ui| {
                ui.set_min_width(MORE_MENU_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                menu.rows(ui)
            })
            .and_then(|inner| inner.inner);
        match picked {
            Some(More::Move(dir)) => self.send_to(path, &dir),
            Some(More::Refresh) => self.refresh(),
            Some(More::AgentLog) => self.toggle_agent_log(path),
            Some(More::DraftDesign) => self.start_enrichment(path),
            Some(More::Archive) => self.archive_issue(path),
            Some(More::Discard) => self.discard_edits(),
            None => {}
        }
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

/// What a row of the `…` menu asks for.
enum More {
    Move(PathBuf),
    Refresh,
    AgentLog,
    DraftDesign,
    Archive,
    Discard,
}

/// The rows of the `…` menu.
struct MoreMenu {
    /// The columns to send the issue to.
    columns: Vec<(String, PathBuf)>,
    /// Refresh, with Auto off, and its key if it has one here.
    refresh: Option<&'static str>,
    agent_log: bool,
    draft_design: bool,
    archive: bool,
    discard: bool,
}

impl MoreMenu {
    fn is_empty(&self) -> bool {
        self.columns.is_empty()
            && self.refresh.is_none()
            && !self.agent_log
            && !self.draft_design
            && !self.archive
            && !self.discard
    }

    fn rows(&self, ui: &mut egui::Ui) -> Option<More> {
        let mut picked = self.move_row(ui);
        let mut row = |ui: &mut egui::Ui, shown: bool, icon, name, key, pick: fn() -> More| {
            let action = MenuAction {
                icon,
                name,
                key,
                ..MenuAction::default()
            };
            if shown && widgets::menu_action(ui, action).clicked() {
                picked = Some(pick());
            }
        };
        let refresh = Some(Icon::Refresh);
        let key = self.refresh.unwrap_or_default();
        row(ui, self.refresh.is_some(), refresh, "Refresh", key, || {
            More::Refresh
        });
        let log = Some(Icon::Log);
        row(ui, self.agent_log, log, "Agent log", "", || More::AgentLog);
        row(ui, self.draft_design, None, "Draft design", "", || {
            More::DraftDesign
        });
        let archive = Some(Icon::Archive);
        row(ui, self.archive, archive, "Archive", "", || More::Archive);
        if self.discard {
            widgets::menu_separator(ui);
        }
        let close = Some(Icon::Close);
        row(ui, self.discard, close, "Discard changes", "", || {
            More::Discard
        });
        picked
    }

    /// Move to…, whose submenu lists the other columns.
    fn move_row(&self, ui: &mut egui::Ui) -> Option<More> {
        if self.columns.is_empty() {
            return None;
        }
        let row = widgets::menu_action(
            ui,
            MenuAction {
                icon: Some(Icon::ArrowRight),
                name: "Move to…",
                submenu: true,
                ..MenuAction::default()
            },
        );
        let picked = egui::containers::menu::SubMenu::new().show(ui, &row, |ui| {
            ui.set_width(MOVE_MENU_WIDTH);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let mut picked = None;
            for (name, dir) in &self.columns {
                let action = MenuAction {
                    name,
                    ..MenuAction::default()
                };
                if widgets::menu_action(ui, action).clicked() {
                    picked = Some(More::Move(dir.clone()));
                }
            }
            picked
        });
        picked.and_then(|inner| inner.inner)
    }
}
