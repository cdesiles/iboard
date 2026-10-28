//! The header bar: the breadcrumb, the project menu and the toolbar.

use super::*;

impl IboardApp {
    /// The 40pt top bar: breadcrumb on the left, the screen's controls on the
    /// right with its main action rightmost. On macOS it is also the window's
    /// title bar: dragging it moves the window, double-clicking zooms it.
    pub(super) fn header_bar(&mut self, ui: &mut egui::Ui) {
        if UNIFIED_TITLEBAR {
            // Interacted with first, so the controls drawn over it win.
            let bar = ui.interact(
                ui.max_rect(),
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

    /// Board: the project name, then the issues path. Reader and editor: a
    /// `‹ Board` button, then `project / Column / Issue title`, every segment
    /// but the last a link.
    pub(super) fn breadcrumb(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let project = self.board.name();
        match self.session.screen.clone() {
            Screen::Board if self.column_focus.column.is_some() => {
                // ColumnFocus: `project / Column`, the project linking back.
                let dir = self.column_focus.column.clone().unwrap_or_default();
                let column = self
                    .board
                    .columns
                    .iter()
                    .find(|column| column.dir == dir)
                    .map(|column| column.name.clone())
                    .unwrap_or_default();
                let back = widgets::crumb(ui, &project, theme::ui_body()).clicked();
                ui.label(RichText::new("/").color(p.ink_faint));
                ui.add(
                    egui::Label::new(RichText::new(column).font(theme::ui_strong()).color(p.ink))
                        .truncate(),
                );
                if back {
                    self.unfocus_column();
                }
            }
            Screen::Board => {
                ui.label(RichText::new(&project).font(theme::ui_title()).color(p.ink));
                let path = if self.showing_all() {
                    self.workspace_groups()
                } else {
                    display_path(&self.board.root)
                };
                let response = ui
                    .horizontal(|ui| {
                        let label = ui.add(
                            egui::Label::new(
                                RichText::new(path)
                                    .font(egui::FontId::monospace(11.0))
                                    .color(p.ink_muted),
                            )
                            .truncate()
                            .sense(Sense::click()),
                        );
                        let (rect, caret) =
                            ui.allocate_exact_size(Vec2::new(12.0, 12.0), Sense::click());
                        if ui.is_rect_visible(rect) {
                            theme::paint_icon(ui.painter(), Icon::ChevronDown, rect, p.ink_faint);
                        }
                        label.union(caret)
                    })
                    .inner
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("Switch project  Ctrl⇥");
                self.project_menu(&response);
            }
            Screen::Detail(path) => self.detail_breadcrumb(ui, &path, &project),
            Screen::Diff => {
                if let Some(path) = self.session.diff.as_ref().map(|view| view.issue.clone()) {
                    self.detail_breadcrumb(ui, &path, &project);
                    ui.label(RichText::new("/").color(p.ink_faint));
                    ui.label(RichText::new("Diff").font(theme::ui_strong()).color(p.ink));
                }
            }
        }
    }

    /// An issue's trail: Board, then `project / Column / Title`, each crumb
    /// leading back to the board.
    fn detail_breadcrumb(&mut self, ui: &mut egui::Ui, path: &Path, project: &str) {
        let p = theme::palette(ui);
        // The visible way out, for whoever does not know the shortcuts.
        let hint = if self.session.detail_mode == DetailMode::Edit {
            "Back to the board (asks if unsaved)"
        } else {
            "Back to the board (esc)"
        };
        let mut back =
            widgets::button(ui, ButtonKind::Back, Some(Icon::Back), "Board", Some("esc"))
                .on_hover_text(hint)
                .clicked();
        // A 16pt `line-strong` divider between the button and the trail.
        let (divider, _) = ui.allocate_exact_size(Vec2::new(1.0, theme::CONTROL_H), Sense::hover());
        ui.painter().line_segment(
            [
                Pos2::new(divider.center().x, divider.center().y - 8.0),
                Pos2::new(divider.center().x, divider.center().y + 8.0),
            ],
            Stroke::new(1.0, p.line_strong),
        );
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        let column = self
            .board
            .columns
            .iter()
            .find(|c| c.issues.iter().any(|t| t.path == *path))
            .map(|c| c.name.clone());
        let title = self
            .board
            .find(path)
            .map(|t| t.title.clone())
            .unwrap_or_else(|| file_name(path));
        let separator = |ui: &mut egui::Ui| {
            ui.label(RichText::new("/").color(p.ink_faint));
        };
        back |= widgets::crumb(ui, project, theme::ui_body()).clicked();
        if let Some(column) = column {
            separator(ui);
            back |= widgets::crumb(ui, &column, theme::ui_body()).clicked();
        }
        separator(ui);
        ui.add(
            egui::Label::new(RichText::new(title).font(theme::ui_strong()).color(p.ink)).truncate(),
        );
        if back {
            self.back_to_board();
        }
    }

    /// The project menu, opened from the path in the board header: All
    /// projects, the recent projects, the workspace's other projects, and a
    /// last row that asks the OS for a folder. Every project goes through
    /// [`Self::open_project`].
    pub(super) fn project_menu(&mut self, anchor: &egui::Response) {
        let showing_all = self.showing_all();
        let current = match showing_all {
            true => PathBuf::new(),
            false => recents::canonical(&self.located.project_dir),
        };
        let projects = self.recents.clone();
        let others = self.workspace_projects();
        let groups = self.workspace.is_some().then(|| self.workspace_groups());
        let picked = egui::Popup::menu(anchor)
            .width(PROJECT_MENU_WIDTH)
            .show(|ui| {
                ui.set_min_width(PROJECT_MENU_WIDTH);
                ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                let mut picked = None;
                if let Some(groups) = &groups {
                    let title = workspace::TITLE;
                    if widgets::menu_row(ui, PROJECT_MENU_WIDTH, title, groups, showing_all)
                        .clicked()
                    {
                        picked = Some(ProjectChoice::All);
                    }
                    ui.separator();
                }
                for project in &projects {
                    let title = project
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("project");
                    let detail = display_path(project);
                    if widgets::menu_row(
                        ui,
                        PROJECT_MENU_WIDTH,
                        title,
                        &detail,
                        *project == current,
                    )
                    .clicked()
                    {
                        picked = Some(ProjectChoice::Recent(project.clone()));
                    }
                }
                if !projects.is_empty() && !others.is_empty() {
                    ui.separator();
                }
                for (project, group) in &others {
                    let title = project
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("project");
                    let detail = format!("{group} · {}", display_path(project));
                    if widgets::menu_row(ui, PROJECT_MENU_WIDTH, title, &detail, false).clicked() {
                        picked = Some(ProjectChoice::Recent(project.clone()));
                    }
                }
                if !projects.is_empty() || !others.is_empty() {
                    ui.separator();
                }
                if widgets::menu_row(
                    ui,
                    PROJECT_MENU_WIDTH,
                    "Open folder…",
                    "Choose a project containing an issues/ folder",
                    false,
                )
                .clicked()
                {
                    picked = Some(ProjectChoice::Browse);
                }
                picked
            })
            .and_then(|inner| inner.inner);
        match picked {
            Some(ProjectChoice::All) => self.open_all(),
            Some(ProjectChoice::Recent(path)) => self.open_project(path),
            Some(ProjectChoice::Browse) => {
                if let Some(dir) = dialog::pick_folder() {
                    self.open_project(dir);
                }
            }
            None => {}
        }
    }

    /// The header's buttons, laid out right to left.
    pub(super) fn header_controls(&mut self, ui: &mut egui::Ui) {
        match self.session.screen.clone() {
            Screen::Board => self.board_controls(ui),
            Screen::Detail(path) => self.detail_controls(ui, &path),
            Screen::Diff => self.refresh_button(ui),
        }
    }

    /// The board's toolbar: New issue, Refresh, Stats, Design and the filter.
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
            .on_hover_text("Add an issue to Todo")
        });
        if new_issue.inner.clicked() {
            self.open_composer();
        }
        self.refresh_button(ui);
        if !self.showing_all()
            && widgets::button(ui, ButtonKind::Secondary, None, "Stats", Some("⌘I"))
                .on_hover_text("Project stats: what the commits say, and the Agenda")
                .clicked()
        {
            self.open_stats();
        }
        if self.design_reachable()
            && widgets::button(ui, ButtonKind::Secondary, None, "Design", None)
                .on_hover_text("Browse the project's design system")
                .clicked()
        {
            self.design_button(ui.visuals().dark_mode);
        }
        self.filter_field(ui);
    }

    /// An issue's toolbar: Refine and its reviews (Todo), Save and Discard in the Editor,
    /// Edit, Commits (Done) and Draft design in the Reader, then the right
    /// group.
    fn detail_controls(&mut self, ui: &mut egui::Ui, path: &Path) {
        let path = path.to_path_buf();
        let exists = self.board.find(&path).is_some();
        let done = self
            .board
            .column_of(&path)
            .map(Column::is_done)
            .unwrap_or(false);
        let reading = self.session.detail_mode == DetailMode::Preview && exists;
        // Reviewing refines an issue, so only Todo issues offer it.
        if self.is_todo_issue(&path) {
            self.review_button(ui, &path);
        }
        // In the Editor, Save and Discard take Edit's place.
        if self.session.detail_mode == DetailMode::Edit && exists {
            if widgets::button(ui, ButtonKind::Primary, None, "Save", Some("⌘S")).clicked() {
                self.save_and_preview();
            }
            if widgets::button(ui, ButtonKind::Secondary, None, "Discard", None)
                .on_hover_text("Drop unsaved changes and go back to the issue")
                .clicked()
            {
                self.discard_edits();
            }
        }
        if reading
            && self.can_edit(&path)
            && widgets::button(
                ui,
                ButtonKind::Secondary,
                Some(Icon::Edit),
                "Edit",
                Some("E"),
            )
            .clicked()
        {
            self.start_editing(&path);
        }
        if reading && done {
            self.commits_button(ui, &path);
        }
        if reading && self.has_agent_log(&path) {
            self.agent_log_button(ui, &path);
        }
        if reading
            && !done
            && self.design_reachable()
            && widgets::button(ui, ButtonKind::Secondary, None, "Draft design", None)
                .on_hover_text("Ask the agent for a static mockup of this change")
                .clicked()
        {
            self.start_enrichment(&path);
        }
        self.detail_right_group(ui, &path, exists, done);
    }

    /// Commits, `Open` while its popup shows, and the newest commit's hash.
    fn commits_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let open = self
            .session
            .commits
            .as_ref()
            .is_some_and(|panel| panel.path == *path);
        let button = widgets::button(
            ui,
            if open {
                ButtonKind::Open
            } else {
                ButtonKind::Secondary
            },
            Some(Icon::Commit),
            "Commits",
            None,
        )
        .on_hover_text("The git commits that name this ticket");
        if let Some(hash) = self.latest_commit(path, ui.ctx()) {
            let p = theme::palette(ui);
            ui.label(
                RichText::new(hash)
                    .font(egui::FontId::monospace(11.0))
                    .color(p.ink_muted),
            )
            .on_hover_text("Newest commit naming this ticket");
        }
        if button.clicked() {
            self.open_commits(path);
        }
    }

    /// The right group: Refresh, then Delete and Add label in the Reader, or
    /// Archive on Done.
    fn detail_right_group(&mut self, ui: &mut egui::Ui, path: &Path, exists: bool, done: bool) {
        let path = path.to_path_buf();
        self.refresh_button(ui);
        // Labelled, and the one control drawn in `danger` at rest. In
        // the Editor `Del` edits text, so the key hint goes.
        let delete_key = (self.session.detail_mode == DetailMode::Preview).then_some("Del");
        if exists
            && !done
            && widgets::button(
                ui,
                ButtonKind::DangerOutline,
                Some(Icon::Trash),
                "Delete",
                delete_key,
            )
            .on_hover_text(if self.confirm_delete {
                "Delete this issue (asks to confirm)"
            } else {
                "Delete this issue"
            })
            .clicked()
        {
            self.request_delete(path.clone());
        }
        // Add label sits first in the right group, before Delete.
        if exists && self.can_edit(&path) && self.session.detail_mode == DetailMode::Preview {
            let open = self
                .session
                .label_picker
                .as_ref()
                .is_some_and(|picker| picker.path == path);
            let add = widgets::button(
                ui,
                if open {
                    ButtonKind::Open
                } else {
                    ButtonKind::Secondary
                },
                Some(Icon::TagPlus),
                "Add label",
                Some("L"),
            )
            .on_hover_text("Add a label to this issue");
            set_label_anchor(ui.ctx(), &path, add.rect);
            if add.clicked() {
                self.toggle_label_picker(path.clone());
            }
        }
        if exists
            && done
            && self.session.detail_mode == DetailMode::Preview
            && widgets::button(
                ui,
                ButtonKind::Secondary,
                Some(Icon::Archive),
                "Archive",
                None,
            )
            .on_hover_text(format!("Move to {ARCHIVE_DIR}/"))
            .clicked()
        {
            self.archive_issue(&path);
        }
    }

    /// Only offered when Settings › General turns Auto off: with Auto on the
    /// board already reloads itself. `⌘R` keeps working either way, but on a
    /// Todo issue it refines.
    pub(super) fn refresh_button(&mut self, ui: &mut egui::Ui) {
        if self.auto {
            return;
        }
        if widgets::button(
            ui,
            ButtonKind::Secondary,
            Some(Icon::Refresh),
            "Refresh",
            None,
        )
        .on_hover_text("Reload from disk (⌘R)")
        .clicked()
        {
            self.refresh();
        }
    }
}
