//! Done's Release flag: one panel that picks the version, whether to write a
//! release note and push, then makes it all in one go.

use std::collections::BTreeMap;

use crate::release::{self, ReleaseChoices};

use super::{
    Align, ButtonKind, Duration, IboardApp, Icon, Instant, Job, Layout, MarginF32, PathBuf,
    RichText, Vec2, agent, job, settings, theme, widgets,
};

/// How far the release thread got, and why it stopped when it did.
type Released = Result<(release::ReleaseProgress, Option<String>), String>;

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
    /// Whether the note opens with sentences the agent writes.
    pub(super) preamble: bool,
    /// Whether an agent is set up to write them; without one the box is off.
    can_write: bool,
    /// The opening sentences the agent wrote, editable like the note.
    pub(super) lead: String,
    /// The opening the agent is writing, while it runs.
    writer: Option<Job<Result<String, String>>>,
    /// Whether the box was just ticked: the panel starts the agent.
    ask_preamble: bool,
    /// Why the agent could not write the opening; the note keeps none.
    preamble_error: Option<String>,
    /// Whether the note prints its technical section.
    pub(super) technical: bool,
    /// The note file, relative to the project root.
    pub(super) file: String,
    /// Whether the release pushes the branch and its new tag.
    pub(super) push: bool,
    /// The commit subjects the note is written from, in commit order.
    subjects: Vec<String>,
    /// The commits' own bodies: the why the opening is written from.
    context: String,
    /// The subjects with no kind label: they stay out of the note.
    pub(super) unclassified: usize,
    /// The technical subjects, printed only when `technical` is on.
    technical_count: usize,
    /// The `feat` subjects, what the bump follows.
    features: usize,
    /// The newest tag the note follows, if any: the release window.
    since: Option<String>,
    /// The release being made on its own thread: the progress it reached and
    /// why it stopped, when it did.
    release: Option<Job<Released>>,
    progress: release::ReleaseProgress,
    pub(super) error: Option<String>,
    /// The project's own step before the release commit, from its
    /// `.iboard.toml`.
    hook: Option<release::Hook>,
    /// Why `.iboard.toml` could not be read: the release does not start over
    /// a hook it cannot see.
    hook_error: Option<String>,
    /// The hooks the user trusts, per project folder.
    trusted: BTreeMap<String, String>,
    /// Whether the hook shown is trusted: ticked once, remembered after.
    pub(super) trust: bool,
}

impl ReleaseFlow {
    #[allow(clippy::too_many_arguments)]
    fn new(
        project: PathBuf,
        project_name: String,
        pattern: String,
        tag: String,
        subjects: Vec<String>,
        since: Option<String>,
        choices: ReleaseChoices,
        can_write: bool,
        hook: Result<Option<release::Hook>, String>,
    ) -> Self {
        let (hook, hook_error) = match hook {
            Ok(hook) => (hook, None),
            Err(error) => (None, Some(error)),
        };
        let trust = hook
            .as_ref()
            .is_some_and(|hook| hook.trusted(&choices.trusted, &project));
        let bump = release::Bump::of(&subjects);
        let note = release::note(&subjects, false);
        let context = release::commit_context(&project, &pattern);
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
            preamble: choices.preamble,
            can_write,
            lead: String::new(),
            writer: None,
            ask_preamble: choices.preamble && can_write,
            preamble_error: None,
            technical: false,
            file: choices.file,
            push: choices.push,
            subjects,
            context,
            unclassified: note.unclassified,
            technical_count: note.technical,
            features: note.features,
            since,
            release: None,
            progress: release::ReleaseProgress::default(),
            error: None,
            hook,
            hook_error,
            trusted: choices.trusted,
            trust,
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

    /// The note's opening sentences, then its sections, as one body.
    fn composed(&self) -> String {
        let lead = self.lead.trim();
        match self.preamble && !lead.is_empty() {
            true => format!("{lead}\n\n{}", self.body.trim_end()),
            false => self.body.trim_end().to_string(),
        }
    }

    /// The section this release writes, dated today.
    pub(super) fn section(&self) -> String {
        let day = chrono::Local::now().format("%-d %b %Y").to_string();
        release::section(self.tag.trim(), &day, &self.composed())
    }

    /// Whether the agent is still writing the opening.
    fn writing(&self) -> bool {
        self.writer.is_some()
    }

    /// Adopts the opening once the agent answers; true while it writes.
    fn adopt_preamble(&mut self) -> bool {
        let Some(writer) = &self.writer else {
            return false;
        };
        let Some(result) = writer.answer() else {
            return true;
        };
        self.writer = None;
        match result {
            Ok(text) => {
                let opening = release::opening(&text);
                if opening.is_empty() {
                    self.preamble_error =
                        Some("the agent answered with nothing usable".to_string());
                } else {
                    self.lead = opening;
                }
            }
            Err(error) => self.preamble_error = Some(error),
        }
        false
    }

    /// Whether the panel should ask the agent for the opening now.
    fn preamble_requested(&mut self) -> bool {
        std::mem::take(&mut self.ask_preamble)
    }

    /// Starts the release on its own thread: the note and its commit, the tag,
    /// the push. Its error stays on screen and the flow can be confirmed again.
    fn start(&mut self) {
        if self.running() || !self.can_start() {
            return;
        }
        if let Some(hook) = &self.hook {
            hook.trust(&mut self.trusted, &self.project);
        }
        let progress = self.progress;
        let (project, tag, section) = (self.project.clone(), self.tag.clone(), self.section());
        let (file, note, push) = (self.file.clone(), self.note, self.push);
        let (bump, previous, hook) = (self.bump, self.since.clone(), self.hook.clone());
        self.error = None;
        self.release = Some(Job::spawn(move |_cancel| {
            let mut progress = progress;
            let release = release::Release {
                project: &project,
                tag: &tag,
                note: &section,
                file: &file,
                update_note: note,
                push,
                bump,
                previous: previous.as_deref(),
                hook: hook.as_ref(),
            };
            let error = release::confirm(&release, &mut progress).err();
            Ok((progress, error))
        }));
    }

    /// Whether the release may start: the project's hook, if it has one, is
    /// readable and trusted.
    fn can_start(&self) -> bool {
        self.hook_error.is_none() && (self.hook.is_none() || self.trust)
    }

    /// Whether the release is still being made.
    pub(super) fn running(&self) -> bool {
        self.release.is_some()
    }

    /// Takes the release's outcome once its thread answers; true while it runs.
    fn adopt_release(&mut self) -> bool {
        let Some(release) = &self.release else {
            return false;
        };
        let Some(answer) = release.answer() else {
            return true;
        };
        self.release = None;
        match answer {
            Ok((progress, error)) => {
                self.progress = progress;
                self.error = error;
            }
            Err(lost) => self.error = Some(lost),
        }
        false
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
                widgets::mark_busy(ui, self.running());
                self.head(ui, p);
                if self.running() {
                    self.releasing_ui(ui, p);
                }
                let editable = !self.running();
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .id_salt("release-body")
                        .max_height(widgets::modal_body_max(ui.ctx().content_rect()))
                        .auto_shrink([false, true])
                        .content_margin(theme::scroll_margin(0.0, 0.0)),
                    |ui| {
                        ui.add_enabled_ui(editable, |ui| {
                            self.version_ui(ui, p);
                            self.hook_ui(ui, p);
                            self.note_choice_ui(ui, p);
                            if self.note {
                                self.note_ui(ui, p);
                            }
                            self.push_ui(ui, p);
                        });
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

    /// The Thinking mark and the time spent while the release is being made.
    fn releasing_ui(&self, ui: &mut egui::Ui, p: &theme::Palette) {
        let elapsed = self
            .release
            .as_ref()
            .map_or_default(|release| release.elapsed());
        widgets::announce_thinking(ui, "Making the release", elapsed);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_2;
            widgets::thinking(ui, 14.0);
            ui.label(
                RichText::new("Making the release…")
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
            false if self.hook.is_some() => {
                "Tags the newest commit, with no release note; the hook's changes get a commit."
            }
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
        self.preamble_ui(ui, p);
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

    /// The opening: the agent's box, the sentences it wrote, and why it
    /// could not.
    fn preamble_ui(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let box_ui = ui.add_enabled_ui(self.can_write, |ui| {
            widgets::checkbox(ui, &mut self.preamble, "Open with a sentence")
        });
        if box_ui.inner.changed() {
            self.error = None;
            if self.preamble && self.lead.trim().is_empty() && !self.writing() {
                self.ask_preamble = true;
            }
        }
        if !self.can_write {
            widgets::help(
                ui,
                "Set an agent up to have the release note opened for you.",
                p.ink_muted,
            );
        }
        if self.writing() {
            widgets::announce_thinking(ui, "Writing the opening", Duration::ZERO);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                widgets::thinking(ui, 14.0);
                ui.label(
                    RichText::new("Writing the opening…")
                        .font(theme::ui_button())
                        .color(p.ink),
                );
            });
        } else if self.preamble {
            let lead = egui::TextEdit::multiline(&mut self.lead)
                .hint_text(RichText::new("What this release brings").color(p.ink_faint))
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .margin(Vec2::new(theme::SPACE_3, theme::SPACE_2));
            ui.add(lead);
        }
        if let Some(error) = &self.preamble_error {
            widgets::help(ui, &format!("The opening stayed empty: {error}"), p.danger);
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

    /// The project's hook: what it runs and commits, and the trust it needs.
    fn hook_ui(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        if let Some(error) = &self.hook_error {
            widgets::field_label(ui, "Project hook");
            widgets::help(ui, error, p.danger);
        }
        let Some(hook) = &self.hook else {
            return;
        };
        widgets::field_label(ui, "Project hook");
        let command = RichText::new(&hook.command)
            .font(theme::meta_mono())
            .color(p.ink);
        ui.label(command);
        let help = match hook.files.as_slice() {
            [] => "Runs before the release commit, from the project root.".to_string(),
            files => format!(
                "Runs before the release commit; {} go into it.",
                files.join(", ")
            ),
        };
        widgets::help(ui, &help, p.ink_muted);
        widgets::checkbox(ui, &mut self.trust, "Trust this command in this project");
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
                .add_enabled_ui(!self.running() && self.can_start(), |ui| {
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
        let hook = release::hook::read(&project);
        let flow = ReleaseFlow::new(
            project,
            self.board.name(),
            pattern,
            tag,
            subjects,
            since,
            choices,
            self.prefs.agent.configured(),
            hook,
        );
        self.session.release = Some(flow);
    }

    /// Confirms or cancels the release on screen. A confirmed release is made
    /// on its own thread; the panel closes once it is done.
    pub(super) fn release_choice(&mut self, choice: ReleaseChoice) {
        let Some(flow) = self.session.release.as_mut() else {
            return;
        };
        match choice {
            ReleaseChoice::Cancel => {
                if !flow.running() {
                    self.session.release = None;
                }
            }
            ReleaseChoice::Confirm => {
                flow.start();
                self.remember_release();
            }
        }
    }

    /// Keeps what the panel's selectors say, and the hooks it trusts, in the
    /// settings file.
    fn remember_release(&mut self) {
        let Some(flow) = self.session.release.as_ref() else {
            return;
        };
        let choices = ReleaseChoices {
            note: flow.note,
            file: flow.file.clone(),
            preamble: flow.preamble,
            push: flow.push,
            trusted: flow.trusted.clone(),
        };
        if choices != self.prefs.release {
            self.prefs.release = choices;
            self.remember_section(settings::Section::new(
                "release",
                &self.prefs.release.section(),
            ));
        }
    }

    /// Follows the flow's two threads: the opening and the release itself.
    /// Asks to draw again while either runs, and closes a made release.
    pub(super) fn poll_release(&mut self) -> Option<Duration> {
        let flow = self.session.release.as_mut()?;
        let waiting = flow.adopt_preamble() | flow.adopt_release();
        if waiting {
            return Some(job::POLL_EVERY);
        }
        if !flow.finished() {
            return None;
        }
        let tag = flow.tag.trim().to_string();
        self.session.release = None;
        self.session.notice = Some((format!("Released {tag}"), Instant::now()));
        None
    }

    /// Starts the agent the panel just asked for, from the note it will open.
    fn start_preamble(&mut self) {
        let agent = self.prefs.agent.clone();
        let Some(flow) = self.session.release.as_mut() else {
            return;
        };
        if !flow.preamble_requested() || !flow.can_write {
            return;
        }
        let prompt = release::preamble_prompt(&flow.body, &flow.context);
        let (writer, _) = agent::start_draft(&agent, &prompt, Some(flow.project.as_path()));
        flow.writer = Some(writer);
        flow.preamble_error = None;
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
        self.remember_release();
        self.start_preamble();
        match (choice, close) {
            (Some(choice), _) => self.release_choice(choice),
            (None, true) => self.release_choice(ReleaseChoice::Cancel),
            (None, false) => {}
        }
    }
}
