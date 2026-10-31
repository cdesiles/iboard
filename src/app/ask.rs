//! Ask AI: the author asks the agent for something on an issue ("take care
//! of point 1"), on a leash of three boxes: read-only, no commit or push, no
//! deploy. The agent works in the issue's project as a build does, its steps
//! in the agent log, and brings the issue up to date.

use super::*;
use crate::agent::Leash;
use crate::build_log;

/// The Ask AI dialog, open on an issue.
pub(super) struct AskDraft {
    path: PathBuf,
    pub(super) request: String,
    read_only: bool,
    no_commit: bool,
    no_deploy: bool,
    focus: bool,
}

impl AskDraft {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            request: String::new(),
            read_only: false,
            no_commit: false,
            no_deploy: true,
            focus: true,
        }
    }

    pub(super) fn leash(&self) -> Leash {
        Leash {
            edits: !self.read_only,
            commits: !self.no_commit,
            deploys: !self.no_deploy,
        }
    }
}

/// An ask the agent is working on.
pub(super) struct AskRun {
    answer: Job<Result<String, String>>,
}

impl IboardApp {
    /// Whether the agent can be asked about `path`: on the board, not done.
    pub(super) fn askable(&self, path: &Path) -> bool {
        self.board
            .column_of(path)
            .is_some_and(|column| !column.is_done())
    }

    /// Whether the agent works on an ask about `path`.
    pub(super) fn asking(&self, path: &Path) -> bool {
        self.session.asks.contains_key(path)
    }

    /// The button and `⌘K`: opens the dialog, or stops the ask running.
    pub(super) fn toggle_ask(&mut self, path: &Path) {
        if self.session.asks.remove(path).is_some() {
            let notice = format!("Stopped the ask on {}", file_name(path));
            self.session.notice = Some((notice, Instant::now()));
        } else if self.askable(path) {
            self.session.ask_draft = Some(AskDraft::new(path.to_path_buf()));
        }
    }

    /// Hands the dialog's request to the agent: the review agent for a
    /// read-only one, the build agent when it may change the project.
    pub(super) fn send_ask(&mut self) {
        let Some(draft) = self.session.ask_draft.take() else {
            return;
        };
        let request = draft.request.trim();
        if request.is_empty() {
            self.session.ask_draft = Some(draft);
            return;
        }
        let config = match draft.read_only {
            true => self.agent.clone(),
            false => self.build_agent.clone(),
        };
        if !config.configured() {
            self.open_config();
            return;
        }
        let Some(content) = self.board.find(&draft.path).map(|i| i.markdown.clone()) else {
            return;
        };
        let leash = draft.leash();
        let prompt = prompts::ask_prompt(&self.relative(&draft.path), &content, request, leash);
        let project = self.board.project_of(&draft.path).to_path_buf();
        let log = build_log::path(self.board.root_of(&draft.path), &draft.path);
        let ends = (project.as_path(), log.as_path());
        let (answer, _) = agent::start_ask(&config, &prompt, &draft.path, ends, leash);
        self.session.asks.insert(draft.path, AskRun { answer });
    }

    /// Takes the asks that are done: the board reads the issue again, the
    /// status bar says what the agent did, and the issue being read opens its
    /// agent log. Called every frame.
    pub(super) fn poll_asks(&mut self, ctx: &egui::Context) {
        let done: Vec<(PathBuf, Result<String, String>)> = self
            .session
            .asks
            .iter()
            .filter_map(|(path, run)| Some((path.clone(), run.answer.answer()?)))
            .collect();
        if self.session.asks.len() > done.len() {
            ctx.request_repaint_after(job::POLL_EVERY);
        }
        for (path, result) in done {
            self.session.asks.remove(&path);
            self.refresh();
            match result {
                Ok(answer) => {
                    let line = answer
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("done");
                    let notice = format!("{}: {}", file_name(&path), line.trim());
                    self.session.notice = Some((notice, Instant::now()));
                }
                Err(err) => self.error = Some(format!("Ask on {} failed: {err}", file_name(&path))),
            }
            let reading = matches!(&self.session.screen, Screen::Detail(open) if *open == path);
            if reading && !self.agent_log_open(&path) {
                self.toggle_agent_log(&path);
            }
        }
    }

    /// The dialog's keys while it is up: `⌘↩` sends, `esc` closes.
    pub(super) fn ask_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter)) {
            self.send_ask();
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.session.ask_draft = None;
        }
    }

    /// Ask AI…, in the Reader of an issue not done; Asking… while the agent
    /// works, to stop it.
    pub(super) fn ask_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let asking = self.asking(path);
        let (icon, label, hint) = match asking {
            true => (
                Icon::Thinking,
                "Asking…",
                "The agent works on your ask: stop it",
            ),
            false => (
                Icon::Review,
                "Ask AI…",
                "Ask the agent for something on this issue: \"take care of point 1\"",
            ),
        };
        if widgets::button(ui, ButtonKind::Review, Some(icon), label, Some("⌘K"))
            .on_hover_text(hint)
            .clicked()
        {
            self.toggle_ask(path);
        }
    }

    /// The Ask AI dialog: the request, its leash, Send and Cancel.
    pub(super) fn ask_dialog(&mut self, ui: &egui::Ui) {
        let Some(draft) = self.session.ask_draft.as_mut() else {
            return;
        };
        let p = theme::palette(ui);
        let title = self
            .board
            .find(&draft.path)
            .map(|issue| issue.title.clone())
            .unwrap_or_else(|| file_name(&draft.path));
        let (width, pad) = (480.0, 20.0);
        let (mut send, mut cancel) = (false, false);
        let frame = widgets::popover_frame(ui).inner_margin(Margin::same(pad as i8));
        let modal = egui::Modal::new(egui::Id::new("ask-ai"))
            .frame(frame)
            .backdrop_color(p.scrim)
            .show(ui.ctx(), |ui| {
                let inner = width - 2.0 * pad;
                ui.set_width(inner);
                ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                let head = RichText::new("Ask the agent")
                    .font(theme::ui_title())
                    .color(p.ink);
                ui.label(head);
                let about = RichText::new(&title)
                    .font(theme::ui_small())
                    .color(p.ink_muted);
                ui.add(egui::Label::new(about).truncate());
                let field = ui.add(
                    egui::TextEdit::multiline(&mut draft.request)
                        .hint_text("Take care of point 1…")
                        .desired_rows(4)
                        .desired_width(inner),
                );
                if std::mem::take(&mut draft.focus) {
                    field.request_focus();
                }
                widgets::checkbox(
                    ui,
                    &mut draft.read_only,
                    "Read-only: answer, change nothing",
                );
                widgets::checkbox(ui, &mut draft.no_commit, "Prevent commit / push");
                widgets::checkbox(ui, &mut draft.no_deploy, "Prevent deploy");
                ui.add_space(theme::SPACE_2);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    let ready = !draft.request.trim().is_empty();
                    send = ui
                        .add_enabled_ui(ready, |ui| {
                            widgets::button(ui, ButtonKind::Primary, None, "Send", Some("⌘↩"))
                        })
                        .inner
                        .clicked();
                    cancel =
                        widgets::button(ui, ButtonKind::Secondary, None, "Cancel", Some("esc"))
                            .clicked();
                });
            });
        if send {
            self.send_ask();
        } else if cancel || modal.should_close() {
            self.session.ask_draft = None;
        }
    }
}
