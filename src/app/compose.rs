//! NewIssueModal's logic: opening and parking it, Draft with Review, and
//! writing the issue to Todo, its title landing later (`naming`). Its look is
//! `new_issue_ui`.

use super::{
    Duration, Engine, HashMap, IboardApp, Instant, Job, Leave, Path, PathBuf, Screen, agent,
    atomic_write, images, labels, prompts, titles,
};
use iboard::new_issue::{self, Parked};

/// How long `⌘Z` can bring back a discarded new issue.
pub(super) const DISCARD_UNDO: Duration = Duration::from_secs(10);

/// NewIssueModal's read model: your notes, the title iboard names from them,
/// and the agent's draft. Closed with notes, it stays parked for the next
/// `⌘N`. Nothing touches Todo until the issue is added.
#[derive(Default)]
pub(super) struct Composer {
    /// The title field, without its tags.
    pub(super) title: String,
    /// The `[label]`s typed at the start of the title, as typed, shown as tags.
    pub(super) title_labels: Vec<String>,
    /// Your notes.
    pub(super) description: String,
    /// Shown over the board; closed, it holds the parked notes.
    pub(super) open: bool,
    /// The engine that names issues, checked when the composer opened;
    /// `None` when it is Off or unavailable: the first line names an untitled
    /// issue.
    pub(super) engine: Option<Engine>,
    pub(super) stage: TitleStage,
    /// The field to give keyboard focus on the next frame.
    pub(super) focus: Option<ComposerField>,
    /// Select the whole title when it gets focus, so typing replaces it.
    pub(super) select_title: bool,
    pub(super) error: Option<String>,
    /// The images pasted before the issue has a name.
    pub(super) images: Option<images::PendingImages>,
    /// What the agent wrote, once it has started writing.
    pub(super) draft: Option<Drafted>,
    /// The agent at work on the draft.
    pub(super) drafting: Option<DraftRun>,
    /// Why the last draft failed, until the next one.
    pub(super) draft_error: Option<String>,
    /// The issue pane keeps the line being written in view.
    pub(super) follow: bool,
}

/// A draft in the issue pane, and what it was made from.
pub(super) struct Drafted {
    pub(super) draft: iboard::new_issue::Draft,
    pub(super) notes_hash: u64,
    /// `tool · model · 14s`, once the agent is done.
    pub(super) receipt: Option<String>,
    /// `esc` stopped the agent before it finished.
    pub(super) stopped: bool,
    /// The title before the draft, for Undo draft.
    title_before: (String, TitleStage),
}

pub(super) struct DraftRun {
    pub(super) job: Job<Result<String, String>>,
    text: agent::DraftText,
    agent: String,
}

impl Composer {
    /// Nothing typed: closing it loses nothing.
    pub(super) fn is_empty(&self) -> bool {
        self.title.trim().is_empty()
            && self.title_labels.is_empty()
            && self.description.trim().is_empty()
    }

    /// The title as the file has it: the tags in brackets, then the field.
    pub(super) fn written_title(&self) -> String {
        new_issue::labelled_title(&self.title_labels, &self.title)
            .trim_end()
            .to_string()
    }

    /// After typing: `[bug]` ending at `cursor`, at the start of the field,
    /// becomes a tag, and a space right after a tag is swallowed. Returns the
    /// cursor's new place when the field changed.
    pub(super) fn tag_typed_label(&mut self, cursor: usize) -> Option<usize> {
        if cursor == 1 && !self.title_labels.is_empty() && self.title.starts_with(' ') {
            self.title.remove(0);
            return Some(0);
        }
        let head: String = self.title.chars().take(cursor).collect();
        let inner = head.strip_prefix('[')?.strip_suffix(']')?;
        if inner.contains(['[', ']']) {
            return None;
        }
        labels::normalize(inner)?;
        self.title_labels.push(inner.to_string());
        self.title = self.title[head.len()..].trim_start().to_string();
        Some(0)
    }

    /// `⌫` right after a tag: the last tag is text again. Returns the cursor's
    /// new place, after the `]`.
    pub(super) fn untag_label(&mut self) -> Option<usize> {
        let text = format!("[{}]", self.title_labels.pop()?);
        let cursor = text.chars().count();
        self.title = if self.title.is_empty() {
            text
        } else {
            format!("{text} {}", self.title)
        };
        Some(cursor)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ComposerField {
    Title,
    Description,
}

/// Where the composer's title comes from.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum TitleStage {
    /// Typed, or named once the issue is written, if an engine is ready.
    #[default]
    Idle,
    /// The title Review drafted, untouched.
    Drafted,
}

impl IboardApp {
    /// Opens NewIssueModal over the board, from any screen.
    pub(super) fn open_composer(&mut self) {
        if self.board.todo_column().is_none() {
            return;
        }
        if matches!(self.session.screen, Screen::Board) {
            self.show_composer();
        } else {
            self.leave(Leave::Composer);
        }
    }

    pub(super) fn show_composer(&mut self) {
        // Checked on every open: Apple Intelligence or an agent may have been
        // set up meanwhile.
        let engine = Some(self.prefs.title_engine).filter(|engine| {
            *engine != Engine::Off && titles::availability(*engine, &self.prefs.agent).is_ready()
        });
        if let Some(engine) = engine {
            titles::prewarm(engine);
        }
        if self.session.composer.is_none() {
            self.session.composer = Some(self.parked_composer().unwrap_or_default());
        }
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        composer.engine = engine;
        composer.open = true;
        composer.focus = Some(ComposerField::Description);
    }

    /// Whether NewIssueModal is up.
    pub(super) fn composer_open(&self) -> bool {
        self.session.composer.as_ref().is_some_and(|c| c.open)
    }

    /// Notes left in a closed modal, in memory or in the park file.
    pub(super) fn has_parked_note(&self) -> bool {
        match &self.session.composer {
            Some(composer) => !composer.open && !composer.is_empty(),
            None => self.park_file().is_some_and(|path| path.exists()),
        }
    }

    fn park_file(&self) -> Option<PathBuf> {
        self.board
            .todo_column()
            .map(|_| new_issue::park_path(&self.board.project_dir))
    }

    fn parked_composer(&self) -> Option<Composer> {
        let parked = Parked::load(&self.park_file()?)?;
        let draft = parked.draft.map(|draft| Drafted {
            draft,
            notes_hash: new_issue::notes_hash(&parked.notes),
            receipt: None,
            stopped: false,
            title_before: (String::new(), TitleStage::Idle),
        });
        let (groups, title) = labels::bracket_groups(&parked.title);
        let title_labels = groups
            .into_iter()
            .filter(|group| labels::normalize(group).is_some())
            .map(str::to_string)
            .collect();
        Some(Composer {
            title: title.to_string(),
            title_labels,
            description: parked.notes,
            draft,
            ..Composer::default()
        })
    }

    /// Close, `esc` or ×: notes are parked for the next `⌘N`; with none,
    /// the modal simply goes.
    pub(super) fn close_composer(&mut self) {
        self.stop_draft();
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        let parked = Parked {
            title: composer.written_title(),
            notes: composer.description.clone(),
            draft: composer.draft.as_ref().map(|d| d.draft.clone()),
        };
        if composer.is_empty() {
            self.session.composer = None;
        } else {
            composer.open = false;
        }
        if let Some(path) = self.park_file()
            && let Err(err) = parked.save(&path)
        {
            self.error = Some(format!("Couldn't park the new issue: {err}"));
        }
    }

    /// Discard: drops the notes and the draft without asking; `⌘Z` in the
    /// next [`DISCARD_UNDO`] brings them back.
    pub(super) fn discard_composer(&mut self) {
        let Some(mut composer) = self.session.composer.take() else {
            return;
        };
        composer.drafting = None;
        composer.open = false;
        if let Some(path) = self.park_file() {
            let _ = new_issue::clear(&path);
        }
        self.session.discarded = Some((composer, Instant::now()));
    }

    /// `⌘Z` after Discard: the modal opens again as it was. True when there
    /// was one to bring back.
    pub(super) fn restore_discarded(&mut self) -> bool {
        match self.session.discarded.take() {
            Some((composer, at)) if at.elapsed() < DISCARD_UNDO => {
                self.session.composer = Some(composer);
                self.show_composer();
                true
            }
            _ => false,
        }
    }

    /// The text of a new issue named `title`, as `↩` writes it, and as the
    /// issue pane shows it.
    fn composed_text(&self, composer: &Composer, title: &str, notes: &str) -> String {
        let draft = composer.draft.as_ref().map(|d| &d.draft);
        new_issue::render(title, notes, draft, self.prefs.keep_notes)
    }

    /// Writes `content` to a new file in `dir` named after `title`. Returns
    /// the new file's path; the caller decides whether to open it.
    pub(super) fn create_issue(
        &mut self,
        dir: &Path,
        title: &str,
        content: &str,
    ) -> Result<PathBuf, String> {
        let (name, _) = crate::issue::new_file_name(dir, title.trim());
        let path = dir.join(&name);
        match atomic_write(&path, content) {
            Ok(()) => {
                self.refresh();
                Ok(path)
            }
            Err(err) => Err(format!(
                "Couldn't create {name} in {}: {err}",
                self.relative(dir)
            )),
        }
    }

    /// `↩`: writes the issue and closes, never waiting. A title you typed is
    /// used as it is; with none, it lands as `unnamed-ticket-xxxx` while an
    /// engine names it, or takes the first line when no engine is ready.
    /// `⌘↩` (`edit`) also opens the Editor. While Review drafts, it adds what
    /// is shown. Tags stay on the title it ends up with. With no notes and no
    /// titled draft to write, nothing is created and the cursor goes to the
    /// notes.
    pub(super) fn submit_composer(&mut self, edit: bool) {
        if self
            .session
            .composer
            .as_ref()
            .is_some_and(|c| c.drafting.is_some())
        {
            self.stop_draft();
        }
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        let typed = composer.written_title();
        let named = !labels::display_title(&typed).is_empty();
        let description = composer.description.clone();
        let engine = composer.engine;
        let drafted = composer.draft.as_ref().is_some_and(|d| !d.draft.is_empty());
        if description.trim().is_empty() && !(named && drafted) {
            composer.focus = Some(ComposerField::Description);
            return;
        }
        let tagged = |name: &str| format!("{typed} {name}").trim().to_string();
        if named {
            self.write_composed(&typed, edit);
        } else if let Some(engine) = engine {
            let placeholder = self.unnamed_title();
            if self.write_composed(&tagged(&placeholder), edit) {
                self.start_naming(engine, placeholder, &description);
            }
        } else {
            self.write_composed(&tagged(&titles::first_line_title(&description)), edit);
        }
    }

    /// A section's ghost tile: adds the issue, then opens the Editor with the
    /// `at`th standard section under the caret.
    pub(super) fn submit_for_section(&mut self, at: usize) {
        let Some(section) = self.board.reader_sections.get(at).cloned() else {
            return;
        };
        self.submit_composer(true);
        if self.session.composer.is_none()
            && let Some(path) = self.session.selected.clone()
        {
            self.add_section(&path, &section);
        }
    }

    /// Writes the new issue to Todo; on success the modal closes and the
    /// card flashes at the top of Todo, on failure it keeps everything and
    /// says why. True when the issue was written.
    pub(super) fn write_composed(&mut self, title: &str, edit: bool) -> bool {
        let Some(column_dir) = self.board.todo_column().map(|c| c.dir.clone()) else {
            return false;
        };
        let dir = match self.compose_dir(&column_dir) {
            Ok(dir) => dir,
            Err(err) => {
                self.error = Some(err);
                return false;
            }
        };
        let Some(composer) = self.session.composer.as_ref() else {
            return false;
        };
        let (name, _) = crate::issue::new_file_name(&dir, title.trim());
        let notes = composer.description.clone();
        let notes = self.adopt_pending_images(&name, &notes);
        let Some(composer) = self.session.composer.as_ref() else {
            return false;
        };
        let content = self.composed_text(composer, title, &notes);
        match self.create_issue(&dir, title, &content) {
            Ok(path) => {
                // A new ticket lands at position 1 of Todo, deferred to the
                // end of the frame.
                if self.board.find(&path).is_some() {
                    self.session.pending_front = Some((column_dir.clone(), path.clone()));
                }
                self.session.composer = None;
                if let Some(park) = self.park_file() {
                    let _ = new_issue::clear(&park);
                }
                self.session.selected = Some(path.clone());
                self.session.new_card = Some((path.clone(), Instant::now()));
                if edit {
                    self.start_editing(&path);
                }
                true
            }
            Err(err) => {
                if let Some(composer) = self.session.composer.as_mut() {
                    composer.error = Some(err);
                    composer.focus = Some(ComposerField::Description);
                }
                false
            }
        }
    }

    /// `esc`: stops Review and keeps what arrived; otherwise closes and parks.
    pub(super) fn composer_escape(&mut self) {
        match self.session.composer.as_mut() {
            Some(composer) if composer.drafting.is_some() => self.stop_draft(),
            _ => self.close_composer(),
        }
    }

    /// Why Draft with Review can't run, if it can't.
    pub(super) fn draft_blocked(&self) -> Option<&'static str> {
        if !self.prefs.agent.configured() {
            Some("Set up Review in ⌘, › AI review")
        } else if self
            .session
            .composer
            .as_ref()
            .is_none_or(|c| c.description.trim().is_empty())
        {
            Some("Write some notes first")
        } else {
            None
        }
    }

    /// Draft with Review (`⌘⇧R`), or Redraft: the agent writes the issue
    /// from the notes, which stay read-only meanwhile.
    pub(super) fn start_draft(&mut self) {
        if self.draft_blocked().is_some() {
            return;
        }
        let labels = self.known_labels();
        let sections = self.board.reader_sections.clone();
        let project = self.board.project_dir.clone();
        let agent = self.prefs.agent.clone();
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        if composer.drafting.is_some() {
            return;
        }
        let notes = composer.description.clone();
        let prompt = prompts::draft_prompt(&notes, &sections, &labels);
        let (job, text) = agent::start_draft(&agent, &prompt, Some(&project));
        let title_before = match composer.draft.take() {
            Some(drafted) => drafted.title_before,
            None => (composer.title.clone(), composer.stage),
        };
        composer.draft = Some(Drafted {
            draft: new_issue::Draft::default(),
            notes_hash: new_issue::notes_hash(&notes),
            receipt: None,
            stopped: false,
            title_before,
        });
        composer.drafting = Some(DraftRun {
            job,
            text,
            agent: agent.label(),
        });
        composer.draft_error = None;
        composer.follow = true;
    }

    /// The labels the board's issues already use, most used first.
    fn known_labels(&self) -> Vec<String> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for issue in self.board.columns.iter().flat_map(|c| &c.issues) {
            for label in &issue.labels {
                *counts.entry(label.to_lowercase()).or_default() += 1;
            }
        }
        let mut labels: Vec<(String, usize)> = counts.into_iter().collect();
        labels.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        labels
            .into_iter()
            .map(|(label, _)| label)
            .take(12)
            .collect()
    }

    /// Fills the draft with what the agent wrote so far, and adopts its
    /// answer once it is in.
    pub(super) fn poll_draft(&mut self) -> Option<Duration> {
        let composer = self.session.composer.as_mut()?;
        let run = composer.drafting.as_ref()?;
        let answer = run.job.answer();
        let elapsed = run.job.elapsed();
        let so_far = run
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let agent = run.agent.clone();
        let drafted = composer.draft.as_mut()?;
        match answer {
            None => {
                drafted.draft = new_issue::parse(&so_far, false);
                return Some(Duration::from_millis(50));
            }
            Some(Ok(text)) => {
                composer.drafting = None;
                drafted.draft = new_issue::parse(&text, true);
                drafted.receipt = Some(format!("{agent} · {}", crate::format::clock(elapsed)));
                adopt_draft_title(composer);
            }
            Some(Err(err)) => {
                composer.drafting = None;
                if let Some(drafted) = composer.draft.take() {
                    (composer.title, composer.stage) = drafted.title_before;
                }
                composer.draft_error = Some(err);
            }
        }
        None
    }

    /// `esc` while drafting: the agent stops, what arrived stays.
    pub(super) fn stop_draft(&mut self) {
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        let Some(run) = composer.drafting.take() else {
            return;
        };
        let text = run
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let draft = new_issue::parse(&text, true);
        if draft.is_empty() {
            if let Some(drafted) = composer.draft.take() {
                (composer.title, composer.stage) = drafted.title_before;
            }
            return;
        }
        if let Some(drafted) = composer.draft.as_mut() {
            drafted.draft = draft;
            drafted.stopped = true;
        }
        adopt_draft_title(composer);
    }

    /// Undo draft (`⌘Z` outside the notes): back to the mirror.
    pub(super) fn undo_draft(&mut self) {
        self.stop_draft();
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        if let Some(drafted) = composer.draft.take() {
            (composer.title, composer.stage) = drafted.title_before;
        }
    }
}

/// The draft's title becomes the issue's, selected so typing renames it.
fn adopt_draft_title(composer: &mut Composer) {
    let Some(title) = composer.draft.as_ref().and_then(|d| d.draft.title.clone()) else {
        return;
    };
    composer.title = title;
    composer.stage = TitleStage::Drafted;
    composer.select_title = true;
    composer.focus = Some(ComposerField::Title);
}
