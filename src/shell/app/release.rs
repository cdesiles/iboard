//! Done's Release flag: one panel that picks the version, whether to write a
//! release note and push, then makes it all in one go.

use crate::release::{self, ReleaseChoices};

use super::{
    Align, ButtonKind, IboardApp, Icon, Instant, Layout, MarginF32, PathBuf, RichText, Vec2,
    settings, theme, widgets,
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
    /// Which part of the version the release moves.
    pub(super) bump: release::Bump,
    /// The tag pattern the project releases under.
    pattern: String,
    /// Whether the tag follows the bump, or the user typed over it.
    tag_follows: bool,
    pub(super) body: String,
    /// The note the subjects make: what `body` holds while it is untouched.
    generated: String,
    /// Whether the release commits an updated note before tagging.
    pub(super) note: bool,
    /// Whether the note prints its technical section.
    pub(super) technical: bool,
    /// The note file, relative to the project root.
    pub(super) file: String,
    /// Whether the release pushes the branch and its new tag.
    pub(super) push: bool,
    /// The commit subjects the note is written from, in commit order.
    subjects: Vec<String>,
    /// The subjects with no kind label: they stay out of the note.
    pub(super) unclassified: usize,
    /// The technical subjects, printed only when `technical` is on.
    technical_count: usize,
    /// The `feat` subjects, what the bump follows.
    features: usize,
    /// The newest tag the note follows, if any: the release window.
    since: Option<String>,
    progress: release::ReleaseProgress,
    pub(super) error: Option<String>,
}

impl ReleaseFlow {
    fn new(
        project: PathBuf,
        project_name: String,
        pattern: String,
        tag: String,
        subjects: Vec<String>,
        since: Option<String>,
        choices: ReleaseChoices,
    ) -> Self {
        let bump = release::Bump::of(&subjects);
        let note = release::note(&subjects, false);
        Self {
            project,
            project_name,
            tag,
            bump,
            pattern,
            tag_follows: true,
            body: note.body.clone(),
            generated: note.body,
            note: choices.note,
            technical: false,
            file: choices.file,
            push: choices.push,
            subjects,
            unclassified: note.unclassified,
            technical_count: note.technical,
            features: note.features,
            since,
            progress: release::ReleaseProgress::default(),
            error: None,
        }
    }

    /// Rewrites the note from the subjects, unless the user typed over it.
    fn rebuild(&mut self) {
        let note = release::note(&self.subjects, self.technical);
        if self.body == self.generated {
            self.body = note.body.clone();
        }
        self.generated = note.body;
        self.unclassified = note.unclassified;
        self.technical_count = note.technical;
        self.features = note.features;
    }

    /// What the bump says about the window, in the field's own words.
    fn bump_help(&self) -> String {
        let window = match &self.since {
            Some(tag) => format!("since {tag}"),
            None => "in the repository".to_string(),
        };
        let noun = match self.features {
            1 => "feature",
            _ => "features",
        };
        match (self.bump, self.features) {
            (release::Bump::Patch, 0) => format!("No {noun} {window}, so a patch."),
            (release::Bump::Patch, n) => {
                format!("You chose a patch, though {n} {noun} shipped {window}.")
            }
            (release::Bump::Minor, 0) => format!("You chose a minor, with no feature {window}."),
            (release::Bump::Minor, n) => format!("{n} {noun} {window}, so a minor."),
            (release::Bump::Major, n) => {
                format!("You chose a major, with {n} {noun} {window}.")
            }
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
            self.tag_follows = false;
            self.error = None;
        }
        ui.add_space(theme::SPACE_2);
        let bump = widgets::segmented(
            ui,
            self.bump,
            &[
                (release::Bump::Patch, "Patch"),
                (release::Bump::Minor, "Minor"),
                (release::Bump::Major, "Major"),
            ],
        );
        if let Some(bump) = bump
            && bump != self.bump
        {
            self.bump = bump;
            self.error = None;
            if self.tag_follows {
                self.tag = release::next_tag(&self.project, &self.pattern, bump);
            }
        }
        widgets::help(ui, &self.bump_help(), p.ink_muted);
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
        let note = egui::TextEdit::multiline(&mut self.body)
            .hint_text(RichText::new("What changed in this release").color(p.ink_faint))
            .desired_rows(6)
            .desired_width(f32::INFINITY)
            .margin(Vec2::new(theme::SPACE_3, theme::SPACE_2));
        ui.add(note);
        let label = match self.technical_count {
            0 => "Include technical changes".to_string(),
            n => format!("Include technical changes ({n})"),
        };
        if widgets::checkbox(ui, &mut self.technical, &label).changed() {
            self.rebuild();
        }
        if self.unclassified > 0 {
            let noun = match self.unclassified {
                1 => "commit",
                _ => "commits",
            };
            widgets::help(
                ui,
                &format!("{} {noun} without a kind won't appear.", self.unclassified),
                p.ink_muted,
            );
        }
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
            let confirm = widgets::button(
                ui,
                ButtonKind::Primary,
                Some(Icon::Release),
                self.action(),
                Some("⌘↩"),
            );
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
    /// newest release tag, grouped by the kind and domain their subjects name.
    pub(super) fn open_release(&mut self) {
        let project = self.board.project_dir.clone();
        let pattern = self.prefs.releases.tags.clone();
        let last = release::last_release(&project, &pattern);
        let subjects = release::commit_subjects(&project, &pattern);
        let tag = release::next_tag(&project, &pattern, release::Bump::of(&subjects));
        let mut choices = self.prefs.release.clone();
        choices.file = release::note_file(&project, &choices.file);
        let since = last.map(|(tag, _)| tag);
        let flow = ReleaseFlow::new(
            project,
            self.board.name(),
            pattern,
            tag,
            subjects,
            since,
            choices,
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

    /// Done's release modal, while one is open.
    pub(super) fn release_window(&mut self, ui: &egui::Ui) {
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
