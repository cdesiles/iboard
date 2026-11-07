//! Done's Release flag: a note in `CHANGELOG.md`, a commit and an annotated
//! tag, then a push. Each step waits for the user to confirm, skip or cancel.

use iboard::release;

use super::{
    Align, ButtonKind, IboardApp, Icon, Instant, Layout, MarginF32, Path, PathBuf, RichText, Vec2,
    theme, widgets,
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
    note: Option<PathBuf>,
    committed: bool,
    tagged: bool,
    history: Vec<(ReleaseStep, Outcome)>,
    pub(super) error: Option<String>,
    issues: usize,
}

impl ReleaseFlow {
    fn new(project: PathBuf, project_name: String, tag: String, titles: &[String]) -> Self {
        Self {
            project,
            project_name,
            tag,
            body: release::note_body(titles),
            step: ReleaseStep::Note,
            note: None,
            committed: false,
            tagged: false,
            history: Vec::new(),
            error: None,
            issues: titles.len(),
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
        self.error = None;
    }

    fn finished(&self) -> bool {
        self.step == ReleaseStep::Push
            && matches!(self.history.last(), Some((ReleaseStep::Push, _)))
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
                self.history_ui(ui, p);
                match self.step {
                    ReleaseStep::Note => self.note_ui(ui, p),
                    ReleaseStep::CommitAndTag => self.commit_ui(ui, p),
                    ReleaseStep::Push => self.push_ui(ui, p),
                }
                if let Some(error) = &self.error {
                    ui.label(RichText::new(error).font(theme::ui_small()).color(p.danger));
                }
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
        ui.label(
            RichText::new(format!(
                "{} · {} · {noun} in Done",
                self.project_name,
                self.tag.trim()
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
        let note = egui::TextEdit::multiline(&mut self.body)
            .hint_text(RichText::new("What changed in this release").color(p.ink_faint))
            .desired_rows(6)
            .desired_width(f32::INFINITY)
            .margin(Vec2::new(theme::SPACE_3, theme::SPACE_2));
        ui.add(note);
        ui.label(
            RichText::new(format!(
                "Written at the top of {} in the project root.",
                release::NOTE_FILE
            ))
            .font(theme::ui_small())
            .color(p.ink_muted),
        );
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
            let confirm = widgets::button(
                ui,
                ButtonKind::Primary,
                Some(Icon::Release),
                self.step.action(),
                Some("⌘↩"),
            );
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
    pub(super) fn open_release(&mut self, paths: &[PathBuf]) {
        let project = self.board.project_dir.clone();
        let tag = release::next_tag(&project, &self.prefs.releases.tags);
        let titles: Vec<String> = paths
            .iter()
            .filter_map(|path| self.board.find(path))
            .map(|issue| issue.title.clone())
            .collect();
        let flow = ReleaseFlow::new(project, self.board.name(), tag, &titles);
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

    /// Done's release modal, while a step asks.
    pub(super) fn release_window(&mut self, ui: &egui::Ui) {
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
