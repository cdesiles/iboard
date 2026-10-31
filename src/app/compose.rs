//! NewIssueModal's logic: opening and parking it, naming the issue, Draft
//! with Review, and writing the issue to Todo. Its look is `new_issue_ui`.

use super::*;
use iboard::new_issue::{self, Parked};

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
        // New issues belong to the board's Todo column, so leave ColumnFocus.
        self.column_focus.column = None;
        // Checked on every open: Apple Intelligence or an agent may have been
        // set up meanwhile.
        let engine = Some(self.title_engine).filter(|engine| {
            *engine != Engine::Manual && titles::availability(*engine, &self.agent).is_ready()
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
        Some(Composer {
            stage: if parked.title.trim().is_empty() {
                TitleStage::Idle
            } else {
                TitleStage::Skipped
            },
            title: parked.title,
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
        composer.naming = None;
        if composer.stage == TitleStage::Naming {
            composer.stage = TitleStage::Idle;
        }
        let parked = Parked {
            title: composer.title.clone(),
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
        composer.naming = None;
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
        new_issue::render(title, notes, draft, self.keep_notes)
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

    /// `↩`: with no title yet and an engine ready, it names the issue first
    /// and stops on the suggestion; otherwise it writes the issue. `⌘↩`
    /// (`edit`) never stops: it names if needed, writes, and opens the
    /// Editor. While Review drafts, it adds what is shown.
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
        if composer.stage == TitleStage::Naming {
            if edit && let Some(run) = composer.naming.as_mut() {
                run.then_edit = true;
            }
            return;
        }
        let typed = composer.title.trim().to_string();
        let description = composer.description.clone();
        if typed.is_empty() && description.trim().is_empty() {
            return;
        }
        let title = if !typed.is_empty() {
            typed
        } else if let (Some(engine), TitleStage::Idle | TitleStage::Inferred) =
            (composer.engine, composer.stage)
        {
            composer.naming = Some(NamingRun {
                answer: titles::start(engine, &self.agent, &description),
                then_edit: edit,
            });
            composer.stage = TitleStage::Naming;
            composer.error = None;
            return;
        } else {
            titles::first_line_title(&description)
        };
        self.write_composed(&title, edit);
    }

    /// Writes the new issue to Todo; on success the modal closes and the
    /// card flashes at the top of Todo, on failure it keeps everything and
    /// says why.
    pub(super) fn write_composed(&mut self, title: &str, edit: bool) {
        let Some(column_dir) = self.board.todo_column().map(|c| c.dir.clone()) else {
            return;
        };
        let dir = match self.compose_dir(&column_dir) {
            Ok(dir) => dir,
            Err(err) => {
                self.error = Some(err);
                return;
            }
        };
        let Some(composer) = self.session.composer.as_ref() else {
            return;
        };
        let (name, _) = crate::issue::new_file_name(&dir, title.trim());
        let notes = composer.description.clone();
        let notes = self.adopt_pending_images(&name, &notes);
        let Some(composer) = self.session.composer.as_ref() else {
            return;
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
            }
            Err(err) => {
                if let Some(composer) = self.session.composer.as_mut() {
                    composer.error = Some(err);
                    composer.focus = Some(ComposerField::Description);
                }
            }
        }
    }

    /// Adopts the engine's title when it lands, or gives up after
    /// [`NAMING_BUDGET`] and lets the user type it.
    pub(super) fn poll_naming(&mut self, ctx: &egui::Context) {
        let Some(run) = self
            .session
            .composer
            .as_ref()
            .and_then(|c| c.naming.as_ref())
        else {
            return;
        };
        let result = match run.answer.answer() {
            Some(result) => result,
            None if run.answer.elapsed() < NAMING_BUDGET => {
                ctx.request_repaint_after(job::POLL_EVERY);
                return;
            }
            None => Err("the engine took too long".to_string()),
        };
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        let then_edit = composer.naming.take().is_some_and(|run| run.then_edit);
        let description = composer.description.clone();
        match result {
            Ok(title) if then_edit => self.write_composed(&title, true),
            Ok(title) => {
                composer.title = title;
                composer.stage = TitleStage::Inferred;
                composer.select_title = true;
                composer.focus = Some(ComposerField::Title);
            }
            // `⌘↩` never stops: the first line names it.
            Err(_) if then_edit => {
                let title = titles::first_line_title(&description);
                self.write_composed(&title, true);
            }
            Err(_) => {
                composer.stage = TitleStage::Failed;
                composer.focus = Some(ComposerField::Title);
            }
        }
    }

    /// `esc`: stops Review and keeps what arrived; while naming, stops and
    /// lets you type the title; otherwise closes and parks.
    pub(super) fn composer_escape(&mut self) {
        match self.session.composer.as_mut() {
            Some(composer) if composer.drafting.is_some() => self.stop_draft(),
            Some(composer) if composer.stage == TitleStage::Naming => {
                composer.naming = None;
                composer.stage = TitleStage::Skipped;
                composer.focus = Some(ComposerField::Title);
            }
            _ => self.close_composer(),
        }
    }

    /// Why Draft with Review can't run, if it can't.
    pub(super) fn draft_blocked(&self) -> Option<&'static str> {
        if !self.agent.configured() {
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
        let agent = self.agent.clone();
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
        composer.naming = None;
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
    pub(super) fn poll_draft(&mut self, ctx: &egui::Context) {
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        let Some(run) = composer.drafting.as_ref() else {
            return;
        };
        let answer = run.job.answer();
        let elapsed = run.job.elapsed();
        let so_far = run
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let agent = run.agent.clone();
        let Some(drafted) = composer.draft.as_mut() else {
            return;
        };
        match answer {
            None => {
                drafted.draft = new_issue::parse(&so_far, false);
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Some(Ok(text)) => {
                composer.drafting = None;
                drafted.draft = new_issue::parse(&text, true);
                drafted.receipt = Some(format!("{agent} · {}s", elapsed.as_secs()));
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
