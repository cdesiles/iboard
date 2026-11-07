//! Windows over the board: the delete confirmation, an issue's commits, Settings.

use super::{
    AgentConfig, Align, ButtonKind, CornerRadius, IboardApp, Instant, Job, Layout, MarginF32, Path,
    PathBuf, Preferences, RichText, Sense, Vec2, commits, display_path, file_name, job, labels,
    settings, settings_ui, theme, trash, widgets,
};

/// The newest commit naming one issue's file, for the board it was looked up
/// on.
pub(super) struct CommitHash {
    path: PathBuf,
    generation: u64,
    lookup: Option<Job<Option<String>>>,
    hash: Option<String>,
}

/// The commits popup: the hashes whose message names a ticket's file, read
/// from git when its button is pressed.
pub(super) struct CommitsPanel {
    pub(super) path: PathBuf,
    title: String,
    pub(super) list: Vec<commits::Commit>,
}

impl IboardApp {
    /// ConfirmDialog for a delete, centred over a scrim: `↩` (or the focused
    /// Delete button) confirms; `esc`, Cancel or a click on the scrim cancels.
    pub(super) fn confirm_dialog(&mut self, ui: &egui::Ui, paths: &[PathBuf]) {
        let (question, title, relative) = match paths {
            [path] => (
                "Delete this issue?".to_string(),
                self.board
                    .find(path)
                    .map(|issue| issue.title.clone())
                    .unwrap_or_else(|| file_name(path)),
                self.relative(path),
            ),
            many => (
                format!("Delete {} issues?", many.len()),
                many.iter()
                    .filter_map(|path| self.board.find(path))
                    .map(|issue| issue.title.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                many.iter()
                    .map(|path| self.relative(path))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        };
        let ask = widgets::Ask {
            id: "confirm-delete",
            question: &question,
            title: &title,
            detail: &relative,
            consequence: trash::CONSEQUENCE,
            confirm: "Delete",
        };
        let answer = widgets::confirm(ui, &ask);
        match answer {
            widgets::Answer::Confirmed => self.delete_issues(paths),
            widgets::Answer::Cancelled => self.session.confirm = None,
            widgets::Answer::Pending => {}
        }
    }

    /// The quit confirmation, while AI runs are at work: Quit stops them and
    /// loses their answers; `esc`, Cancel or the scrim keeps iboard open.
    pub(super) fn quit_dialog(&mut self, ui: &egui::Ui, runs: usize) {
        let p = theme::palette(ui);
        let size = theme::ModalSize::S;
        let (width, pad) = (theme::modal_size(size, ui.ctx().content_rect()).x, 20.0);
        let mut answer = widgets::Answer::Pending;
        let frame = widgets::modal_frame(ui).inner_margin(MarginF32 {
            left: pad,
            right: pad,
            top: pad,
            bottom: theme::SPACE_5,
        });
        let modal = widgets::modal(ui, "confirm-quit", size, "Quit iboard?")
            .frame(frame)
            .show(ui.ctx(), |ui| {
                ui.set_width(width - 2.0 * pad);
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                let title = RichText::new("Quit iboard?")
                    .font(theme::ui_title())
                    .color(p.ink);
                ui.label(title);
                let what = match runs {
                    1 => "An AI run is at work: quitting stops it, and its answer is lost."
                        .to_string(),
                    n => format!(
                        "{n} AI runs are at work: quitting stops them, and their answers are lost."
                    ),
                };
                ui.label(
                    RichText::new(what)
                        .font(theme::ui_small())
                        .color(p.ink_muted),
                );
                ui.add_space(theme::SPACE_3);
                answer = widgets::confirm_buttons(ui, None, "Quit");
            });
        if answer == widgets::Answer::Confirmed {
            self.quit_anyway();
        } else if answer == widgets::Answer::Cancelled || modal.should_close() {
            self.session.confirm = None;
        }
    }

    /// The commits popup: the hashes whose message names the ticket's file,
    /// read from git when its node button was pressed. `esc` closes it.
    pub(super) fn commits_window(&mut self, ui: &egui::Ui) {
        let p = theme::palette(ui);
        let Some(panel) = self.session.commits.as_ref() else {
            return;
        };
        let size = theme::ModalSize::M;
        let width = theme::modal_size(size, ui.ctx().content_rect()).x;
        let pad = 20.0;
        let mut close = false;
        let mut picked = None;
        let frame = widgets::modal_frame(ui).inner_margin(MarginF32 {
            left: pad,
            right: pad,
            top: pad,
            bottom: theme::SPACE_5,
        });
        let modal = widgets::modal(ui, "commits", size, "Commits")
            .frame(frame)
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
                widgets::focus_here(ui);
                picked = commit_list(ui, &panel.list);
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
        if let Some(hashes) = picked {
            let path = panel.path.clone();
            self.open_diff(&path, hashes);
        } else if close || (!modal.backdrop_response.clicked() && modal.should_close()) {
            self.session.commits = None;
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
            settings_ui::Action::Keep => {}
            settings_ui::Action::Close => {
                self.settings_tab = dialog.tab;
                self.settings = None;
            }
            settings_ui::Action::Save => {
                // The AI review settings are written once they were seen, so
                // saving a title engine never marks an agent as set up.
                let save_review = dialog.review_seen || self.prefs.agent.configured();
                if save_review && !dialog.validate() {
                    dialog.tab = settings_ui::Tab::Review;
                    return;
                }
                let save_build = dialog.build_seen;
                if save_build && !dialog.validate_build() {
                    dialog.tab = settings_ui::Tab::Build;
                    return;
                }
                let mut prefs = Preferences {
                    title_engine: dialog.engine,
                    confirm_delete: dialog.confirm_delete,
                    appearance: dialog.appearance,
                    auto: dialog.auto,
                    ..self.prefs.clone()
                };
                if save_review {
                    prefs.agent = AgentConfig {
                        configured: true,
                        ..dialog.form.clone()
                    };
                }
                if save_build {
                    prefs.build_agent = AgentConfig {
                        configured: true,
                        ..dialog.build_form.clone()
                    };
                    prefs.build_timeout = dialog.build_timeout;
                }
                // `[labels]` is rewritten only once its tab was looked at.
                if dialog.labels_seen {
                    prefs.labels = dialog.labels();
                }
                let renames = dialog.renames.clone();
                // Only what changed is written, so a setting another window
                // saved in the meantime survives.
                let result = prefs.sections_since(&self.prefs).and_then(|sections| {
                    match &self.config_file {
                        Some(file) if !sections.is_empty() => settings::save(file, &sections),
                        _ => Ok(()),
                    }
                });
                match result {
                    Ok(()) => {
                        self.settings_tab = dialog.tab;
                        theme::apply(ui.ctx(), prefs.appearance);
                        self.prefs = prefs;
                        if dialog.labels_seen {
                            self.rename_labels(&renames);
                        }
                        self.config_problem = None;
                        self.settings = None;
                        // The composer follows the new engine at once.
                        if self.composer_open() {
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
        self.open_settings(settings_ui::Tab::Review);
    }

    /// Settings › Titles, from the composer's engine tag.
    pub(super) fn open_title_settings(&mut self) {
        self.open_settings(settings_ui::Tab::Titles);
    }

    pub(super) fn open_settings(&mut self, tab: settings_ui::Tab) {
        let used: Vec<String> = self
            .board
            .columns
            .iter()
            .flat_map(|column| column.issues.iter())
            .flat_map(|issue| issue.labels.iter().cloned())
            .filter(|label| !labels::is_origin(label))
            .collect();
        let mut dialog = settings_ui::Dialog::new(&self.prefs, tab).with_used_labels(used);
        dialog.save_error = self.config_problem.clone();
        self.settings = Some(dialog);
    }

    /// The short hash of the newest commit naming `path`'s file, once a
    /// lookup off the UI thread has found it. The lookup runs again when the
    /// issue or the board changes, showing the last hash meanwhile.
    pub(super) fn latest_commit(&mut self, path: &Path, ctx: &egui::Context) -> Option<String> {
        let current =
            self.session.commit_hash.as_ref().is_some_and(|known| {
                known.path == path && known.generation == self.board_generation
            });
        if !current {
            let hash = self
                .session
                .commit_hash
                .take()
                .filter(|known| known.path == path)
                .and_then(|known| known.hash);
            let dir = self.board.project_of(path).to_path_buf();
            let file = file_name(path);
            let lookup = Job::spawn(move |_| {
                commits::for_file(&dir, &file)
                    .first()
                    .map(|commit| commit.hash.clone())
            });
            self.session.commit_hash = Some(CommitHash {
                path: path.to_path_buf(),
                generation: self.board_generation,
                lookup: Some(lookup),
                hash,
            });
        }
        let known = self.session.commit_hash.as_mut()?;
        if let Some(lookup) = &known.lookup {
            match lookup.poll() {
                job::Poll::Ready(hash) => {
                    known.hash = hash;
                    known.lookup = None;
                }
                job::Poll::Lost => known.lookup = None,
                job::Poll::Waiting => ctx.request_repaint_after(job::POLL_EVERY),
            }
        }
        known.hash.clone()
    }

    /// Opens the commits popup for `path`: the git log entries of its project
    /// whose message names its file (the `closes <file>.md` line a commit
    /// carries).
    pub(super) fn open_commits(&mut self, path: &Path) {
        let file = file_name(path);
        let dir = self.board.project_of(path).to_path_buf();
        let list = commits::for_file(&dir, &file);
        let title = self
            .board
            .find(path)
            .map(|issue| issue.title.clone())
            .unwrap_or_else(|| file.clone());
        self.session.commits = Some(CommitsPanel {
            path: path.to_path_buf(),
            title,
            list,
        });
    }

    /// Opens the Diff screen on every commit that closed `path`, newest
    /// first; or says there is none.
    pub(super) fn show_issue_diff(&mut self, path: &Path) {
        let dir = self.board.project_of(path).to_path_buf();
        let hashes: Vec<String> = commits::for_file(&dir, &file_name(path))
            .into_iter()
            .map(|commit| commit.hash)
            .collect();
        if hashes.is_empty() {
            self.session.notice = Some((
                format!("No commit names {}", file_name(path)),
                Instant::now(),
            ));
        } else {
            self.open_diff(path, hashes);
        }
    }
}

/// The commits naming a ticket, newest first: hash, day and subject; or
/// that there are none. A click on one asks for its diff, `All` for every
/// one's, newest first.
fn commit_list(ui: &mut egui::Ui, list: &[commits::Commit]) -> Option<Vec<String>> {
    let p = theme::palette(ui);
    if list.is_empty() {
        ui.label(
            RichText::new("No commit names this ticket.")
                .font(theme::ui_small())
                .color(p.ink_muted),
        );
        return None;
    }
    let mut picked = None;
    theme::scroll_area(
        ui,
        egui::ScrollArea::vertical()
            .id_salt("commits-list")
            .content_margin(theme::scroll_margin(0.0, 0.0))
            .max_height(260.0)
            .auto_shrink([false, true]),
        |ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
            for commit in list {
                if commit_row(ui, commit).clicked() {
                    picked = Some(vec![commit.hash.clone()]);
                }
            }
        },
    );
    if list.len() > 1
        && widgets::button(
            ui,
            ButtonKind::Secondary,
            None,
            &format!("All {} commits", list.len()),
            None,
        )
        .on_hover_text("Every closing commit's diff, newest first")
        .clicked()
    {
        picked = Some(list.iter().map(|commit| commit.hash.clone()).collect());
    }
    picked
}

/// A commit as a clickable row: its hash in `accent`, its day, its subject;
/// `surface-hover` under the pointer.
fn commit_row(ui: &mut egui::Ui, commit: &commits::Commit) -> egui::Response {
    let p = theme::palette(ui);
    // The hover fill goes under the row, drawn once its rect is known.
    let fill = ui.painter().add(egui::Shape::Noop);
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        ui.add_space(theme::SPACE_1);
        ui.label(
            RichText::new(&commit.hash)
                .font(theme::meta())
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
    let rect = row.response.rect.expand2(Vec2::new(0.0, 2.0));
    let response = ui
        .interact(
            rect,
            egui::Id::unique(("commit", &commit.hash)),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Show this commit's diff");
    response.widget_info(|| {
        let label = format!("Diff of {}", commit.hash);
        egui::WidgetInfo::labeled(egui::Role::Button, true, label)
    });
    if response.hovered() {
        ui.painter().set(
            fill,
            egui::Shape::rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover),
        );
    }
    response
}
