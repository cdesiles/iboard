//! Windows over the board: the delete confirmation, an issue's commits, Settings.

use super::*;

impl IboardApp {
    /// ConfirmDialog for a delete, centred over a scrim: `↩` (or the focused
    /// Delete button) confirms; `esc`, Cancel or a click on the scrim cancels.
    pub(super) fn confirm_dialog(&mut self, ui: &egui::Ui, path: &Path) {
        let p = theme::palette(ui);
        let title = self
            .board
            .find(path)
            .map(|issue| issue.title.clone())
            .unwrap_or_else(|| file_name(path));
        let relative = self.relative(path);
        let width = 360.0;
        let pad = 20.0;
        let mut confirmed = false;
        let mut cancelled = false;
        let frame = Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .shadow(ui.visuals().popup_shadow)
            .inner_margin(Margin {
                left: pad as i8,
                right: pad as i8,
                top: pad as i8,
                bottom: theme::SPACE_5 as i8,
            });
        let modal = egui::Modal::new(egui::Id::new("confirm-delete"))
            .frame(frame)
            .backdrop_color(p.scrim)
            .show(ui.ctx(), |ui| {
                let inner = width - 2.0 * pad;
                ui.set_width(inner);
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                ui.label(
                    RichText::new("Delete this issue?")
                        .font(theme::ui_title())
                        .color(p.ink),
                );
                // The subject: what goes, by title and by file.
                ui.add_space(theme::SPACE_2);
                Frame::new()
                    .fill(p.surface_sunken)
                    .corner_radius(CornerRadius::same(theme::RADIUS_SM))
                    .inner_margin(Margin::symmetric(10, theme::SPACE_3 as i8))
                    .show(ui, |ui| {
                        ui.set_width(inner - 20.0);
                        ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                        ui.add(
                            egui::Label::new(
                                RichText::new(&title).font(theme::ui_strong()).color(p.ink),
                            )
                            .truncate(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&relative)
                                    .font(theme::meta())
                                    .color(p.ink_muted),
                            )
                            .truncate(),
                        );
                    });
                ui.label(
                    RichText::new(trash::CONSEQUENCE)
                        .font(theme::ui_small())
                        .color(p.ink_muted),
                );
                ui.add_space(theme::SPACE_3);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    // Delete starts focused: `Del` then `↩` is the fast path,
                    // and the dialog itself is the safety step.
                    let delete = widgets::button(
                        ui,
                        ButtonKind::Danger,
                        Some(Icon::Trash),
                        "Delete",
                        Some("↩"),
                    );
                    ui.painter().rect_stroke(
                        delete.rect.expand(2.0),
                        CornerRadius::same(theme::RADIUS_SM + 2),
                        Stroke::new(2.0, p.accent),
                        StrokeKind::Outside,
                    );
                    confirmed = delete.clicked();
                    cancelled =
                        widgets::button(ui, ButtonKind::Secondary, None, "Cancel", Some("esc"))
                            .clicked();
                });
            });
        if confirmed {
            self.delete_issue(path);
        } else if cancelled || modal.should_close() {
            self.confirm = None;
        }
    }

    /// The commits popup: the hashes whose message names the ticket's file,
    /// read from git when its node button was pressed. `esc` or the scrim
    /// closes it.
    pub(super) fn commits_window(&mut self, ui: &egui::Ui) {
        let p = theme::palette(ui);
        let Some(panel) = self.commits.as_ref() else {
            return;
        };
        let width = 460.0;
        let pad = 20.0;
        let mut close = false;
        let frame = Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .shadow(ui.visuals().popup_shadow)
            .inner_margin(Margin {
                left: pad as i8,
                right: pad as i8,
                top: pad as i8,
                bottom: theme::SPACE_5 as i8,
            });
        let modal = egui::Modal::new(egui::Id::new("commits"))
            .frame(frame)
            .backdrop_color(p.scrim)
            .show(ui.ctx(), |ui| {
                let inner = width - 2.0 * pad;
                ui.set_width(inner);
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                ui.label(
                    RichText::new("Commits")
                        .font(theme::ui_title())
                        .color(p.ink),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(&panel.title)
                            .font(theme::ui_strong())
                            .color(p.ink),
                    )
                    .truncate(),
                );
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        self.relative(&panel.path),
                        file_name(&panel.path)
                    ))
                    .font(theme::meta())
                    .color(p.ink_muted),
                );
                ui.add_space(theme::SPACE_2);
                if panel.list.is_empty() {
                    ui.label(
                        RichText::new("No commit names this ticket.")
                            .font(theme::ui_small())
                            .color(p.ink_muted),
                    );
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("commits-list")
                        .max_height(260.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                            for commit in &panel.list {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                                    ui.label(
                                        RichText::new(&commit.hash)
                                            .font(egui::FontId::monospace(12.0))
                                            .color(p.accent),
                                    );
                                    ui.label(
                                        RichText::new(commit.day())
                                            .font(theme::meta())
                                            .color(p.ink_muted),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&commit.subject)
                                                .font(theme::ui_small())
                                                .color(p.ink),
                                        )
                                        .truncate(),
                                    );
                                });
                            }
                        });
                }
                ui.add_space(theme::SPACE_2);
                ui.label(
                    RichText::new(
                        "Read from the commit messages that name this file (`closes <file>.md`).",
                    )
                    .font(theme::ui_small())
                    .color(p.ink_muted),
                );
                ui.add_space(theme::SPACE_3);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, ButtonKind::Secondary, None, "Close", Some("esc"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if close || modal.should_close() {
            self.commits = None;
        }
    }

    /// The ReviewSettings dialog (`⌘,`): the tool and model to run, saved to
    /// the user's `iboard.toml`.
    pub(super) fn config_window(&mut self, ui: &mut egui::Ui) {
        let Some(dialog) = self.settings.as_mut() else {
            return;
        };
        let saved_to = match &self.config_file {
            Some(file) => format!("Saved to {}", display_path(file)),
            None => "Saved for this session".to_string(),
        };
        match dialog.show(ui, &mut self.catalog, &saved_to) {
            review_settings::Action::Keep => {}
            review_settings::Action::Close => {
                self.settings_tab = dialog.tab;
                self.settings = None;
            }
            review_settings::Action::Save => {
                // The AI review settings are written once they were seen, so
                // saving a title engine never marks an agent as set up.
                let save_review = dialog.review_seen || self.agent.configured();
                if save_review && !dialog.validate() {
                    dialog.tab = review_settings::Tab::Review;
                    return;
                }
                let mut config = dialog.form.clone();
                config.configured = self.agent.configured() || save_review;
                let engine = dialog.engine;
                let confirm_delete = dialog.confirm_delete;
                // `[labels]` is rewritten only once its tab was looked at.
                let label_table = dialog
                    .labels_seen
                    .then(|| labels::table(&dialog.label_rows));
                let renames = dialog.renames.clone();
                // Only what changed is written, so a setting another window
                // saved in the meantime survives.
                let changed = || -> Result<Vec<settings::Section>, String> {
                    let mut sections = Vec::new();
                    if save_review && config != self.agent {
                        sections.push(agent::section(&config)?);
                    }
                    if engine != self.title_engine {
                        sections.push(titles::section(engine)?);
                    }
                    if confirm_delete != self.confirm_delete {
                        sections.push(general::section(confirm_delete)?);
                    }
                    if let Some(table) = &label_table
                        && *table != labels::table(&self.labels.entries())
                    {
                        sections.push(labels::section(table)?);
                    }
                    Ok(sections)
                };
                let result = changed().and_then(|sections| match &self.config_file {
                    Some(file) if !sections.is_empty() => settings::save(file, &sections),
                    _ => Ok(()),
                });
                match result {
                    Ok(()) => {
                        self.settings_tab = dialog.tab;
                        if save_review {
                            self.agent = config;
                        }
                        self.title_engine = engine;
                        self.confirm_delete = confirm_delete;
                        if let Some(table) = &label_table {
                            self.rename_labels(&renames);
                            self.labels = Labels::with(table);
                        }
                        self.config_problem = None;
                        self.settings = None;
                        // The composer follows the new engine at once.
                        if self.composer.is_some() {
                            self.show_composer();
                        }
                    }
                    Err(error) => dialog.save_error = Some(error),
                }
            }
        }
    }

    /// Opens the ReviewSettings dialog on the current settings.
    pub(super) fn open_config(&mut self) {
        self.open_settings(review_settings::Tab::Review);
    }

    /// Settings › Titles, from the composer's engine tag.
    pub(super) fn open_title_settings(&mut self) {
        self.open_settings(review_settings::Tab::Titles);
    }

    pub(super) fn open_settings(&mut self, tab: review_settings::Tab) {
        let used: Vec<String> = self
            .board
            .columns
            .iter()
            .flat_map(|column| column.issues.iter())
            .flat_map(|issue| issue.labels.iter().cloned())
            .collect();
        let mut dialog =
            review_settings::Dialog::new(&self.agent, self.title_engine, self.confirm_delete, tab)
                .with_labels(&self.labels, used);
        dialog.save_error = self.config_problem.clone();
        self.settings = Some(dialog);
    }

    /// The short hash of the newest commit naming `path`'s file, if any.
    pub(super) fn latest_commit(&self, path: &Path) -> Option<String> {
        let dir = path.parent().unwrap_or(&self.board.root);
        commits::for_file(dir, &file_name(path))
            .first()
            .map(|commit| commit.hash.clone())
    }

    /// Opens the commits popup for `path`: the git log entries whose message
    /// names its file (the `closes <file>.md` line a commit carries).
    pub(super) fn open_commits(&mut self, path: &Path) {
        let file = file_name(path);
        let dir = path.parent().unwrap_or(&self.board.root).to_path_buf();
        let list = commits::for_file(&dir, &file);
        let title = self
            .board
            .find(path)
            .map(|issue| issue.title.clone())
            .unwrap_or_else(|| file.clone());
        self.commits = Some(CommitsPanel {
            path: path.to_path_buf(),
            title,
            list,
        });
    }
}
