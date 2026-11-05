//! Ask AI: the author asks the agent for something on an issue ("take care
//! of point 1"), on a leash of three boxes: read-only, no commit or push, no
//! deploy. The agent works in the issue's project as a build does, its steps
//! in the agent log, and brings the issue up to date.

use super::{
    ButtonKind, Duration, IboardApp, Icon, Instant, Job, Key, Path, PathBuf, RichText, Screen,
    file_name, job, prompts, theme, widgets,
};
use crate::agent::{self, Leash};
use crate::build_log;

/// The Ask AI dialog, open on an issue.
pub(super) struct AskDraft {
    pub(super) path: PathBuf,
    pub(super) request: String,
    read_only: bool,
    no_commit: bool,
    no_deploy: bool,
    /// `esc` found typed text: the dialog asks before dropping it.
    discarding: bool,
}

impl AskDraft {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            request: String::new(),
            read_only: false,
            no_commit: false,
            no_deploy: true,
            discarding: false,
        }
    }

    pub(super) fn leash(&self) -> Leash {
        Leash {
            edits: !self.read_only,
            commits: !self.no_commit,
            deploys: !self.no_deploy,
        }
    }

    /// Esc, Cancel or the close button: true when it may close now, having
    /// asked first about typed text.
    fn closes(&mut self) -> bool {
        let closes = self.request.trim().is_empty() || self.discarding;
        self.discarding = !closes;
        closes
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
            true => self.prefs.agent.clone(),
            false => self.prefs.build_agent.clone(),
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
        let (answer, _) = agent::start_ask(
            &config,
            &prompt,
            &draft.path,
            ends,
            leash,
            self.prefs.build_timeout,
        );
        self.session.asks.insert(draft.path, AskRun { answer });
    }

    /// Takes the asks that are done: the board reads the issue again, the
    /// status bar says what the agent did, and the issue being read opens its
    /// agent log. Called every frame.
    pub(super) fn poll_asks(&mut self) -> Option<Duration> {
        for (path, _, result) in job::take_landed(&mut self.session.asks, |run| &run.answer) {
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
            let reading = matches!(&self.session.screen, Screen::Detail(open, _) if *open == path);
            if reading && !self.agent_log_open(&path) {
                self.toggle_agent_log(&path);
            }
        }
        (!self.session.asks.is_empty()).then_some(job::POLL_EVERY)
    }

    /// The dialog's keys while it is up: `⌘↩` sends, `esc` closes.
    pub(super) fn ask_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter)) {
            self.send_ask();
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
            && let Some(draft) = self.session.ask_draft.as_mut()
        {
            if draft.discarding {
                draft.discarding = false;
            } else if draft.closes() {
                self.session.ask_draft = None;
            }
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

    /// The Ask AI dialog: the issue in its head, the request and its leash in
    /// the body, Cancel and Send in its foot.
    pub(super) fn ask_dialog(&mut self, ui: &egui::Ui) {
        let Some(path) = self.session.ask_draft.as_ref().map(|d| d.path.clone()) else {
            return;
        };
        let relative = self.relative(&path);
        let title = self
            .board
            .find(&path)
            .map_or_else(|| file_name(&path), |issue| issue.title.clone());
        let Some(draft) = self.session.ask_draft.as_mut() else {
            return;
        };
        let size = theme::ModalSize::M;
        let width = theme::modal_size(size, ui.ctx().content_rect()).x;
        let (mut send, mut cancel, mut discard) = (false, false, false);
        widgets::modal(ui, "ask-ai", size, "Ask the agent").show(ui.ctx(), |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            cancel = widgets::modal_header(ui, "Ask the agent", |ui| about(ui, &title));
            widgets::rule(ui);
            egui::Frame::new()
                .inner_margin(theme::SPACE_5)
                .show(ui, |ui| ask_body(ui, draft));
            widgets::rule(ui);
            if draft.discarding
                && let Some(answer) = widgets::discard_bar(ui, "Discard this request?")
            {
                discard = answer;
                draft.discarding = false;
            }
            let foot = ask_foot(ui, &relative, !draft.request.trim().is_empty());
            send = foot.0;
            cancel |= foot.1;
        });
        if send {
            self.send_ask();
        } else if discard || (cancel && draft.closes()) {
            self.session.ask_draft = None;
        }
    }
}

/// The issue asked about, in the head: `meta`, cut short before the close
/// button.
fn about(ui: &mut egui::Ui, title: &str) {
    let p = theme::palette(ui);
    let room = ui.available_width() - theme::CONTROL_H - theme::SPACE_3;
    ui.allocate_ui(egui::vec2(room, theme::CONTROL_H), |ui| {
        let text = RichText::new(title).font(theme::meta()).color(p.ink_muted);
        ui.add(egui::Label::new(text).truncate());
    });
}

fn ask_body(ui: &mut egui::Ui, draft: &mut AskDraft) {
    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
    widgets::focus_here(ui);
    ui.add(
        egui::TextEdit::multiline(&mut draft.request)
            .hint_text("Take care of point 1…")
            .desired_rows(4)
            .desired_width(ui.available_width()),
    );
    widgets::checkbox(
        ui,
        &mut draft.read_only,
        "Read-only: answer, change nothing",
    );
    widgets::checkbox(ui, &mut draft.no_commit, "Prevent commit / push");
    widgets::checkbox(ui, &mut draft.no_deploy, "Prevent deploy");
}

/// The foot: the issue's file on the left, Cancel and Send on the right.
/// Says whether each was clicked.
fn ask_foot(ui: &mut egui::Ui, file: &str, ready: bool) -> (bool, bool) {
    let p = theme::palette(ui);
    widgets::footer_bar(ui, theme::SPACE_5, |ui| {
        let send = ui
            .add_enabled_ui(ready, |ui| {
                widgets::button(ui, ButtonKind::Send, None, "Send", Some("⌘↩"))
            })
            .inner
            .clicked();
        let cancel =
            widgets::button(ui, ButtonKind::Secondary, None, "Cancel", Some("esc")).clicked();
        let note = RichText::new(file).font(theme::meta()).color(p.ink_muted);
        ui.add(egui::Label::new(note).truncate());
        (send, cancel)
    })
    .inner
}
