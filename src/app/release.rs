//! Done's Release flag: a note in `CHANGELOG.md`, a commit and an annotated
//! tag, then a push. Each step waits for the user to confirm, skip or cancel.

use iboard::release;

use super::{
    Align, ButtonKind, Duration, IboardApp, Icon, Instant, Job, Layout, MarginF32, Path, PathBuf,
    ReviewMode, RichText, Vec2, agent, job, theme, widgets,
};

/// The three steps of a release, in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ReleaseStep {
    Note,
    CommitAndTag,
    Push,
}

impl ReleaseStep {
    fn title(self) -> &'static str {
        match self {
            Self::Note => "Release note",
            Self::CommitAndTag => "Commit and tag",
            Self::Push => "Push",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::Note => "Write note",
            Self::CommitAndTag => "Commit and tag",
            Self::Push => "Push",
        }
    }
}

/// What the user chose on the step on screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ReleaseChoice {
    Confirm,
    Skip,
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    Done,
    Skipped,
}

impl Outcome {
    fn word(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Skipped => "skipped",
        }
    }
}

/// One release in progress: its version, its note, and which step is asking.
pub(super) struct ReleaseFlow {
    project: PathBuf,
    project_name: String,
    pub(super) tag: String,
    pub(super) body: String,
    pub(super) step: ReleaseStep,
    /// The titles the note lists, as the list the agent was asked to group.
    titles: Vec<String>,
    /// The sectioned note the agent is writing, while it runs.
    writer: Option<Job<Result<String, String>>>,
    /// Whether the agent grouped the note into sections.
    pub(super) grouped: bool,
    /// Why the agent could not group the note; the raw list stays.
    note_error: Option<String>,
    /// The newest tag the note follows, if any: the release window.
    since: Option<String>,
    note: Option<PathBuf>,
    committed: bool,
    tagged: bool,
    history: Vec<(ReleaseStep, Outcome)>,
    pub(super) error: Option<String>,
    issues: usize,
}

impl ReleaseFlow {
    fn new(
        project: PathBuf,
        project_name: String,
        tag: String,
        titles: Vec<String>,
        since: Option<String>,
        writer: Option<Job<Result<String, String>>>,
    ) -> Self {
        Self {
            project,
            project_name,
            tag,
            body: release::note_body(&titles),
            step: ReleaseStep::Note,
            issues: titles.len(),
            titles,
            writer,
            grouped: false,
            note_error: None,
            since,
            note: None,
            committed: false,
            tagged: false,
            history: Vec::new(),
            error: None,
        }
    }

    /// The section this release writes, dated today.
    fn section(&self) -> String {
        let day = chrono::Local::now().format("%-d %b %Y").to_string();
        release::section(self.tag.trim(), &day, &self.body)
    }

    fn check_tag(&self) -> Result<(), String> {
        let tag = self.tag.trim();
        if tag.is_empty() {
            return Err("The tag can't be empty.".to_string());
        }
        if tag.starts_with('-') || tag.contains(char::is_whitespace) {
            return Err(format!("`{tag}` is not a tag git accepts."));
        }
        Ok(())
    }

    /// Runs the current step. An error stays on screen and the step stays up.
    fn confirm(&mut self) -> Result<(), String> {
        match self.step {
            ReleaseStep::Note => {
                self.check_tag()?;
                if release::tag_exists(&self.project, self.tag.trim()) {
                    return Err(format!("{} is already a tag.", self.tag.trim()));
                }
                self.note = Some(release::write_note(&self.project, &self.section())?);
                self.writer = None;
            }
            ReleaseStep::CommitAndTag => {
                self.check_tag()?;
                if !self.committed {
                    let message = format!("Release {}", self.tag.trim());
                    release::commit(&self.project, self.note.as_deref(), &message)?;
                    self.committed = true;
                }
                if !self.tagged {
                    release::tag(&self.project, self.tag.trim(), &self.section())?;
                    self.tagged = true;
                }
            }
            ReleaseStep::Push => {
                release::push(&self.project, self.tagged)?;
            }
        }
        self.finish_step();
        Ok(())
    }

    fn finish_step(&mut self) {
        self.history.push((self.step, Outcome::Done));
        self.step = match self.step {
            ReleaseStep::Note => ReleaseStep::CommitAndTag,
            ReleaseStep::CommitAndTag => ReleaseStep::Push,
            ReleaseStep::Push => ReleaseStep::Push,
        };
        self.error = None;
    }

    fn skip(&mut self) {
        self.history.push((self.step, Outcome::Skipped));
        if self.step != ReleaseStep::Push {
            self.step = match self.step {
                ReleaseStep::Note => ReleaseStep::CommitAndTag,
                _ => ReleaseStep::Push,
            };
        }
        self.writer = None;
        self.error = None;
    }

    fn finished(&self) -> bool {
        self.step == ReleaseStep::Push
            && matches!(self.history.last(), Some((ReleaseStep::Push, _)))
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
                if !text.is_empty() && self.body == release::note_body(&self.titles) {
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
                        self.history_ui(ui, p);
                        match self.step {
                            ReleaseStep::Note => self.note_ui(ui, p),
                            ReleaseStep::CommitAndTag => self.commit_ui(ui, p),
                            ReleaseStep::Push => self.push_ui(ui, p),
                        }
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
        let noun = if self.issues == 1 { "issue" } else { "issues" };
        let window = match &self.since {
            Some(tag) => format!("since {tag}"),
            None => "in Done".to_string(),
        };
        ui.label(
            RichText::new(format!(
                "{} · {} · {} {noun} {window}",
                self.project_name,
                self.tag.trim(),
                self.issues
            ))
            .font(theme::meta())
            .color(p.ink_muted),
        );
        ui.add_space(theme::SPACE_1);
    }

    fn history_ui(&self, ui: &mut egui::Ui, p: &theme::Palette) {
        for (step, outcome) in &self.history {
            let color = match outcome {
                Outcome::Done => p.status_done,
                Outcome::Skipped => p.ink_faint,
            };
            ui.label(
                RichText::new(format!("{}: {}", step.title(), outcome.word()))
                    .font(theme::ui_small())
                    .color(color),
            );
        }
    }

    fn note_ui(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        widgets::field_label(ui, "Tag");
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
        widgets::field_label(ui, "Release note");
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
        ui.label(
            RichText::new(format!(
                "Written at the top of {} in the project root.",
                release::NOTE_FILE
            ))
            .font(theme::ui_small())
            .color(p.ink_muted),
        );
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

    fn commit_ui(&self, ui: &mut egui::Ui, p: &theme::Palette) {
        let note = match &self.note {
            Some(path) => format!(
                "Commit {}, then tag the commit.",
                relative(&self.project, path)
            ),
            None => "No release note was written: the commit is empty, then the tag.".to_string(),
        };
        ui.label(
            RichText::new(note)
                .font(theme::ui_small())
                .color(p.ink_muted),
        );
        ui.label(
            RichText::new(format!(
                "tag -a {} -m \"{}\"",
                self.tag.trim(),
                self.tag.trim()
            ))
            .font(theme::meta())
            .color(p.ink_faint),
        );
    }

    fn push_ui(&self, ui: &mut egui::Ui, p: &theme::Palette) {
        let what = match self.tagged {
            true => "Push the branch and its new tag to origin.",
            false => "No tag was made: push the branch to origin.",
        };
        ui.label(
            RichText::new(what)
                .font(theme::ui_small())
                .color(p.ink_muted),
        );
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
                        self.step.action(),
                        Some("⌘↩"),
                    )
                })
                .inner;
            if confirm.clicked() {
                choice = Some(ReleaseChoice::Confirm);
            }
            if widgets::button(ui, ButtonKind::Secondary, None, "Skip", None).clicked() {
                choice = Some(ReleaseChoice::Skip);
            }
            if widgets::button(ui, ButtonKind::Secondary, None, "Cancel", Some("esc")).clicked() {
                choice = Some(ReleaseChoice::Cancel);
            }
        });
        choice
    }
}

/// `path` under `project`, for the dialog's text.
fn relative(project: &Path, path: &Path) -> String {
    path.strip_prefix(project)
        .unwrap_or(path)
        .display()
        .to_string()
}

impl IboardApp {
    /// Opens the release flow on `paths`, the Done issues its note lists.
    /// Only the issues done since the newest release tag are kept, and the
    /// refinement agent groups their titles into sections when it is set up.
    pub(super) fn open_release(&mut self, paths: &[PathBuf]) {
        let project = self.board.project_dir.clone();
        let last = release::last_release(&project, &self.prefs.releases.tags);
        let tag = release::next_tag(&project, &self.prefs.releases.tags);
        let titles: Vec<String> = paths
            .iter()
            .filter_map(|path| self.board.find(path))
            .filter(|issue| match (&last, issue.done_at) {
                (Some((_, since)), Some(done)) => done >= *since,
                _ => true,
            })
            .map(|issue| issue.title.clone())
            .collect();
        let writer = (self.prefs.agent.configured() && !titles.is_empty()).then(|| {
            let prompt = release::note_prompt(&titles);
            let note = project.join(release::NOTE_FILE);
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
        let flow = ReleaseFlow::new(project, self.board.name(), tag, titles, since, writer);
        self.session.release = Some(flow);
    }

    /// Confirms, skips or cancels the step on screen. A finished flow closes
    /// and says so in the status bar.
    pub(super) fn release_choice(&mut self, choice: ReleaseChoice) {
        let Some(flow) = self.session.release.as_mut() else {
            return;
        };
        match choice {
            ReleaseChoice::Cancel => {
                self.session.release = None;
                return;
            }
            ReleaseChoice::Skip => flow.skip(),
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

    /// Done's release modal, while a step asks.
    pub(super) fn release_window(&mut self, ui: &egui::Ui) {
        if let Some(after) = self.poll_release() {
            ui.ctx().request_repaint_after(after);
        }
        let Some(flow) = self.session.release.as_mut() else {
            return;
        };
        let (choice, close) = flow.show(ui);
        match (choice, close) {
            (Some(choice), _) => self.release_choice(choice),
            (None, true) => self.release_choice(ReleaseChoice::Cancel),
            (None, false) => {}
        }
    }
}
