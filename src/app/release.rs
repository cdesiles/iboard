//! Done's Release flag: one panel that picks the version, whether to write a
//! release note and push, then makes it all in one go.

use iboard::release::{self, ReleaseChoices};

use super::{
    Align, ButtonKind, Duration, IboardApp, Icon, Instant, Job, Layout, MarginF32, PathBuf,
    ReviewMode, RichText, Vec2, agent, job, settings, theme, widgets,
};

/// What the user chose on the release panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ReleaseChoice {
    Confirm,
    Cancel,
}

/// One release in progress: its version, its note, and the choices made.
pub(super) struct ReleaseFlow {
    project: PathBuf,
    project_name: String,
    pub(super) tag: String,
    pub(super) body: String,
    /// Whether the release commits an updated note before tagging.
    pub(super) note: bool,
    /// The note file, relative to the project root.
    pub(super) file: String,
    /// Whether the release pushes the branch and its new tag.
    pub(super) push: bool,
    /// The commit subjects the note lists, as the list the agent was asked to
    /// group.
    subjects: Vec<String>,
    /// The sectioned note the agent is writing, while it runs.
    writer: Option<Job<Result<String, String>>>,
    /// Whether the agent grouped the note into sections.
    pub(super) grouped: bool,
    /// Why the agent could not group the note; the raw list stays.
    note_error: Option<String>,
    /// The newest tag the note follows, if any: the release window.
    since: Option<String>,
    progress: release::ReleaseProgress,
    pub(super) error: Option<String>,
}

impl ReleaseFlow {
    fn new(
        project: PathBuf,
        project_name: String,
        tag: String,
        subjects: Vec<String>,
        since: Option<String>,
        choices: ReleaseChoices,
        writer: Option<Job<Result<String, String>>>,
    ) -> Self {
        Self {
            project,
            project_name,
            tag,
            body: release::note_body(&subjects),
            note: choices.note,
            file: choices.file,
            push: choices.push,
            subjects,
            writer,
            grouped: false,
            note_error: None,
            since,
            progress: release::ReleaseProgress::default(),
            error: None,
        }
    }

    /// The section this release writes, dated today.
    fn section(&self) -> String {
        let day = chrono::Local::now().format("%-d %b %Y").to_string();
        release::section(self.tag.trim(), &day, &self.body)
    }

    /// Makes the release in order: the note and its commit, the tag, the
    /// push. An error stays on screen and the flow can be confirmed again.
    fn confirm(&mut self) -> Result<(), String> {
        release::confirm(
            &self.project,
            &self.tag,
            &self.section(),
            &self.file,
            self.note,
            self.push,
            &mut self.progress,
        )?;
        self.error = None;
        Ok(())
    }

    /// Whether the release is made, its push included when one was asked.
    fn finished(&self) -> bool {
        self.progress.finished(self.push)
    }

    /// Whether the agent is still grouping the note.
    fn writing(&self) -> bool {
        self.writer.is_some()
    }

    /// Adopts the sectioned note once the agent answers, unless the user has
    /// typed over the raw list it was asked to group. True while it writes.
    fn adopt_note(&mut self) -> bool {
        let Some(writer) = &self.writer else {
            return false;
        };
        let Some(result) = writer.answer() else {
            return true;
        };
        self.writer = None;
        match result {
            Ok(text) => {
                let text = text.trim();
                if !text.is_empty() && self.body == release::note_body(&self.subjects) {
                    self.body = format!("{text}\n");
                    self.grouped = true;
                }
            }
            Err(error) => self.note_error = Some(error),
        }
        false
    }

    /// The modal's body and buttons; `close` when its scrim or `esc` asked.
    fn show(&mut self, ui: &egui::Ui) -> (Option<ReleaseChoice>, bool) {
        let p = theme::palette(ui);
        let size = theme::ModalSize::M;
        let (width, pad) = (theme::modal_size(size, ui.ctx().content_rect()).x, 20.0);
        let mut choice = None;
        let frame = widgets::modal_frame(ui).inner_margin(MarginF32 {
            left: pad,
            right: pad,
            top: pad,
            bottom: theme::SPACE_5,
        });
        let modal = widgets::modal(ui, "release", size, "Release")
            .frame(frame)
            .show(ui.ctx(), |ui| {
                ui.set_width(width - 2.0 * pad);
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                widgets::mark_busy(ui, self.writing());
                self.head(ui, p);
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .id_salt("release-body")
                        .max_height(widgets::modal_body_max(ui.ctx().content_rect()))
                        .auto_shrink([false, true])
                        .content_margin(theme::scroll_margin(0.0, 0.0)),
                    |ui| {
                        self.version_ui(ui, p);
                        self.note_choice_ui(ui, p);
                        if self.note {
                            self.note_ui(ui, p);
                        }
                        self.push_ui(ui, p);
                        if let Some(error) = &self.error {
                            ui.label(RichText::new(error).font(theme::ui_small()).color(p.danger));
                        }
                    },
                );
                ui.add_space(theme::SPACE_2);
                choice = self.footer(ui);
            });
        let close = choice.is_none() && modal.should_close();
        (choice, close)
    }

    fn head(&self, ui: &mut egui::Ui, p: &theme::Palette) {
        ui.label(
            RichText::new("Release")
                .font(theme::ui_title())
                .color(p.ink),
        );
        let noun = if self.subjects.len() == 1 {
            "commit"
        } else {
            "commits"
        };
        let window = match &self.since {
            Some(tag) => format!("since {tag}"),
            None => "in the repository".to_string(),
        };
        ui.label(
            RichText::new(format!(
                "{} · {} · {} {noun} {window}",
                self.project_name,
                self.tag.trim(),
                self.subjects.len()
            ))
            .font(theme::meta())
            .color(p.ink_muted),
        );
        ui.add_space(theme::SPACE_1);
    }

    fn version_ui(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        widgets::field_label(ui, "Version");
        let tag = egui::TextEdit::singleline(&mut self.tag)
            .hint_text(RichText::new("v0.4.0").color(p.ink_faint))
            .desired_width(f32::INFINITY)
            .margin(Vec2::new(theme::SPACE_3, 4.0));
        if widgets::text_entry(ui, tag).changed() {
            self.error = None;
        }
        ui.label(
            RichText::new("The next patch after the newest tag that matches [stats] release_tags.")
                .font(theme::ui_small())
                .color(p.ink_muted),
        );
    }

    fn note_choice_ui(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        widgets::field_label(ui, "Release note");
        let picked = widgets::segmented(
            ui,
            self.note,
            &[(true, "Update the release note"), (false, "Tag only")],
        );
        if let Some(note) = picked
            && note != self.note
        {
            self.note = note;
            self.error = None;
            if !note {
                self.writer = None;
                self.note_error = None;
            }
        }
        let help = match self.note {
            true => "Commits the updated note before tagging the commit.",
            false => "Tags the newest commit, with no release note and no new commit.",
        };
        ui.label(
            RichText::new(help)
                .font(theme::ui_small())
                .color(p.ink_muted),
        );
    }

    fn note_ui(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        widgets::field_label(ui, "Note file");
        let file = egui::TextEdit::singleline(&mut self.file)
            .hint_text(RichText::new(release::NOTE_FILE).color(p.ink_faint))
            .desired_width(f32::INFINITY)
            .margin(Vec2::new(theme::SPACE_3, 4.0));
        if widgets::text_entry(ui, file).changed() {
            self.error = None;
        }
        ui.label(
            RichText::new("A path from the project root.")
                .font(theme::ui_small())
                .color(p.ink_muted),
        );
        if self.writing() {
            self.grouping_ui(ui, p);
        } else if self.grouped {
            widgets::announce(ui, "release-grouped", "The note was grouped into sections");
            ui.label(
                RichText::new("Grouped into sections.")
                    .font(theme::ui_small())
                    .color(p.status_done),
            );
        }
        let writing = self.writing();
        let note = egui::TextEdit::multiline(&mut self.body)
            .hint_text(RichText::new("What changed in this release").color(p.ink_faint))
            .desired_rows(6)
            .desired_width(f32::INFINITY)
            .margin(Vec2::new(theme::SPACE_3, theme::SPACE_2));
        ui.add_enabled(!writing, note);
        if let Some(error) = &self.note_error {
            ui.label(
                RichText::new(format!("The note stayed a list: {error}"))
                    .font(theme::ui_small())
                    .color(p.danger),
            );
        }
    }

    /// The Thinking mark and the time spent while the agent groups the note.
    fn grouping_ui(&self, ui: &mut egui::Ui, p: &theme::Palette) {
        let elapsed = self
            .writer
            .as_ref()
            .map_or_default(|writer| writer.elapsed());
        widgets::announce_thinking(ui, "Grouping the note into sections", elapsed);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_2;
            widgets::thinking(ui, 14.0);
            ui.label(
                RichText::new("Grouping the note into sections…")
                    .font(theme::ui_button())
                    .color(p.ink),
            );
            ui.label(
                RichText::new(crate::format::clock(elapsed))
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        });
    }

    fn push_ui(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        widgets::field_label(ui, "After release");
        if let Some(push) =
            widgets::segmented(ui, self.push, &[(true, "Push"), (false, "Keep it local")])
        {
            self.push = push;
            self.error = None;
        }
        let help = match self.push {
            true => "Pushes the branch and its new tag to origin.",
            false => "Leaves the tag on this machine.",
        };
        ui.label(
            RichText::new(help)
                .font(theme::ui_small())
                .color(p.ink_muted),
        );
    }

    /// What the confirm button says, from the two selectors.
    fn action(&self) -> &'static str {
        match (self.note, self.push) {
            (true, true) => "Update release note, version and push",
            (true, false) => "Update release note and version",
            (false, true) => "Set and push the release tag",
            (false, false) => "Set the release tag",
        }
    }

    fn footer(&self, ui: &mut egui::Ui) -> Option<ReleaseChoice> {
        let mut choice = None;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            let confirm = ui
                .add_enabled_ui(!self.writing(), |ui| {
                    widgets::button(
                        ui,
                        ButtonKind::Primary,
                        Some(Icon::Release),
                        self.action(),
                        Some("⌘↩"),
                    )
                })
                .inner;
            if confirm.clicked() {
                choice = Some(ReleaseChoice::Confirm);
            }
            if widgets::button(ui, ButtonKind::Secondary, None, "Cancel", Some("esc")).clicked() {
                choice = Some(ReleaseChoice::Cancel);
            }
        });
        choice
    }
}

impl IboardApp {
    /// Opens the release flow: its note lists the commits made since the
    /// newest release tag, and the refinement agent groups their subjects into
    /// sections when it is set up.
    pub(super) fn open_release(&mut self) {
        let project = self.board.project_dir.clone();
        let last = release::last_release(&project, &self.prefs.releases.tags);
        let tag = release::next_tag(&project, &self.prefs.releases.tags);
        let subjects = release::commit_subjects(&project, &self.prefs.releases.tags);
        let mut choices = self.prefs.release.clone();
        choices.file = release::note_file(&project, &choices.file);
        let writer =
            (choices.note && self.prefs.agent.configured() && !subjects.is_empty()).then(|| {
                let prompt = release::note_prompt(&subjects);
                let note = project.join(&choices.file);
                let (writer, _) = agent::start_review(
                    &self.prefs.agent,
                    &prompt,
                    &note,
                    Some(project.as_path()),
                    ReviewMode::Quick,
                );
                writer
            });
        let since = last.map(|(tag, _)| tag);
        let flow = ReleaseFlow::new(
            project,
            self.board.name(),
            tag,
            subjects,
            since,
            choices,
            writer,
        );
        self.session.release = Some(flow);
    }

    /// Confirms or cancels the release on screen. A finished flow closes and
    /// says so in the status bar.
    pub(super) fn release_choice(&mut self, choice: ReleaseChoice) {
        let Some(flow) = self.session.release.as_mut() else {
            return;
        };
        match choice {
            ReleaseChoice::Cancel => {
                self.session.release = None;
                return;
            }
            ReleaseChoice::Confirm => {
                if flow.writing() {
                    return;
                }
                if let Err(error) = flow.confirm() {
                    flow.error = Some(error);
                    return;
                }
            }
        }
        if flow.finished() {
            let tag = flow.tag.trim().to_string();
            self.session.release = None;
            self.session.notice = Some((format!("Released {tag}"), Instant::now()));
        }
    }

    /// Adopts the sectioned note once the agent writes it; called every
    /// frame the modal draws, and asks to draw again while it does.
    pub(super) fn poll_release(&mut self) -> Option<Duration> {
        let flow = self.session.release.as_mut()?;
        flow.adopt_note().then_some(job::POLL_EVERY)
    }

    /// Done's release modal, while one is open.
    pub(super) fn release_window(&mut self, ui: &egui::Ui) {
        if let Some(after) = self.poll_release() {
            ui.ctx().request_repaint_after(after);
        }
        let Some(flow) = self.session.release.as_mut() else {
            return;
        };
        let (choice, close) = flow.show(ui);
        let choices = ReleaseChoices {
            note: flow.note,
            file: flow.file.clone(),
            push: flow.push,
        };
        if choices != self.prefs.release {
            self.prefs.release = choices;
            self.remember_section(settings::Section::new(
                "release",
                &self.prefs.release.section(),
            ));
        }
        match (choice, close) {
            (Some(choice), _) => self.release_choice(choice),
            (None, true) => self.release_choice(ReleaseChoice::Cancel),
            (None, false) => {}
        }
    }
}
