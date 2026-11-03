//! Ask AI: a conversation with the agent on an issue ("take care of point
//! 1"), in a panel beside the reader, on a leash of three boxes: read-only,
//! no commit or push, no deploy. The agent works in the issue's project as a
//! build does, its steps in the agent log. With Claude Code each message
//! resumes the session of the one before; the conversation is kept in the
//! issue's sidecar.

use super::review::{review_head, running_line};
use super::*;
use crate::agent::{Asked, Leash, Progress};
use crate::build_log;
use crate::reviews::{self, Chat, Turn};

/// Room under the conversation for the message, its leash and the buttons.
const COMPOSER_HEIGHT: f32 = 230.0;

/// The Ask AI panel, open beside an issue.
pub(super) struct AskPanel {
    pub(super) path: PathBuf,
    pub(super) request: String,
    read_only: bool,
    no_commit: bool,
    no_deploy: bool,
    focus: bool,
    pub(super) chat: Chat,
    cache: CommonMarkCache,
}

impl AskPanel {
    fn new(path: PathBuf, chat: Chat) -> Self {
        Self {
            path,
            request: String::new(),
            read_only: false,
            no_commit: false,
            no_deploy: true,
            focus: true,
            chat,
            cache: CommonMarkCache::default(),
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
    answer: Job<Result<Asked, String>>,
    progress: Progress,
    since: Instant,
}

/// What a click in the panel asks for.
enum PanelAction {
    Send,
    Stop,
    Terminal,
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

    pub(super) fn ask_open(&self, path: &Path) -> bool {
        self.session
            .ask_panel
            .as_ref()
            .is_some_and(|panel| panel.path == path)
    }

    /// The button and `⌘K`: opens the panel beside the issue, or closes it.
    /// An ask running goes on either way.
    pub(super) fn toggle_ask(&mut self, path: &Path) {
        if self.ask_open(path) {
            self.session.ask_panel = None;
        } else if self.askable(path) || self.asking(path) {
            self.open_ask(path);
        }
    }

    /// Opens the panel on the conversation kept for `path`, in place of the
    /// review or the agent log.
    pub(super) fn open_ask(&mut self, path: &Path) {
        let chat = reviews::load(self.board.root_of(path), path)
            .chat
            .unwrap_or_default();
        self.close_review();
        self.session.agent_log = None;
        self.session.ask_panel = Some(AskPanel::new(path.to_path_buf(), chat));
    }

    /// Continue in terminal: the conversation's session, in Terminal.
    fn continue_in_terminal(&mut self, path: &Path) {
        let Some(session) = self
            .session
            .ask_panel
            .as_ref()
            .and_then(|panel| panel.chat.session.clone())
        else {
            return;
        };
        match agent::continue_in_terminal(self.board.project_of(path), &session) {
            Ok(()) => {
                let notice = format!("Opened the session of {} in Terminal", file_name(path));
                self.session.notice = Some((notice, Instant::now()));
            }
            Err(err) => self.error = Some(err),
        }
    }

    fn stop_ask(&mut self, path: &Path) {
        if self.session.asks.remove(path).is_some() {
            let notice = format!("Stopped the ask on {}", file_name(path));
            self.session.notice = Some((notice, Instant::now()));
        }
    }

    /// Hands the panel's message to the agent: the review agent for a
    /// read-only one, the build agent when it may change the project. It
    /// resumes the conversation's session when there is one.
    pub(super) fn send_ask(&mut self) {
        let Some(panel) = self.session.ask_panel.as_ref() else {
            return;
        };
        let request = panel.request.trim().to_string();
        if request.is_empty() || self.asking(&panel.path) {
            return;
        }
        let config = match panel.read_only {
            true => self.agent.clone(),
            false => self.build_agent.clone(),
        };
        if !config.configured() {
            self.open_config();
            return;
        }
        let (path, leash, resume) = (
            panel.path.clone(),
            panel.leash(),
            panel.chat.session.clone(),
        );
        let Some(content) = self.board.find(&path).map(|i| i.markdown.clone()) else {
            return;
        };
        let prompt = prompts::ask_prompt(&self.relative(&path), &content, &request, leash);
        let project = self.board.project_of(&path).to_path_buf();
        let log = build_log::path(self.board.root_of(&path), &path);
        let ends = (project.as_path(), log.as_path());
        let (answer, progress) =
            agent::start_ask(&config, &prompt, &path, ends, leash, resume.as_deref());
        let since = Instant::now();
        let run = AskRun {
            answer,
            progress,
            since,
        };
        self.session.asks.insert(path.clone(), run);
        self.add_turn(&path, "you", request, None);
        if let Some(panel) = self.session.ask_panel.as_mut() {
            panel.request.clear();
        }
    }

    /// Adds a turn to the conversation on `path`, on disk and in the panel
    /// open on it, with the session the agent ran in.
    fn add_turn(&mut self, path: &Path, who: &str, text: String, session: Option<String>) {
        let root = self.board.root_of(path).to_path_buf();
        let mut sidecar = reviews::load(&root, path);
        let chat = sidecar.chat.get_or_insert_default();
        chat.turns.push(Turn {
            who: who.to_string(),
            text,
            at: chrono::Local::now().to_rfc3339(),
        });
        if session.is_some() {
            chat.session = session;
        }
        let chat = chat.clone();
        if let Err(err) = reviews::save(&root, path, &sidecar) {
            self.error = Some(err);
        }
        if let Some(panel) = self.session.ask_panel.as_mut().filter(|p| p.path == path) {
            panel.chat = chat;
        }
    }

    /// Takes the asks that are done: the board reads the issue again, the
    /// answer joins the conversation, and the issue being read shows it in
    /// the panel; another waits on its card. Called every frame.
    pub(super) fn poll_asks(&mut self, ctx: &egui::Context) {
        let done: Vec<(PathBuf, Result<Asked, String>)> = self
            .session
            .asks
            .iter()
            .filter_map(|(path, run)| Some((path.clone(), run.answer.answer()?)))
            .collect();
        if !self.session.asks.is_empty() {
            ctx.request_repaint_after(job::POLL_EVERY);
        }
        for (path, result) in done {
            self.session.asks.remove(&path);
            self.rescan();
            match result {
                Ok(asked) => {
                    let line = asked
                        .answer
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("done");
                    let notice = format!("{}: {}", file_name(&path), line.trim());
                    self.session.notice = Some((notice, Instant::now()));
                    self.add_turn(&path, "agent", asked.answer, asked.session);
                }
                Err(err) => {
                    self.add_turn(&path, "agent", format!("**Failed:** {err}"), None);
                    self.error = Some(format!("Ask on {} failed: {err}", file_name(&path)));
                }
            }
            let reading = matches!(&self.session.screen, Screen::Detail(open) if *open == path);
            if !reading || self.session.review.is_some() {
                self.session.answered.insert(path);
            } else if !self.ask_open(&path) {
                self.open_ask(&path);
            }
        }
    }

    /// Ask AI…, in the Reader of an issue not done; Asking… while the agent
    /// works. Either opens or closes the panel.
    pub(super) fn ask_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let (icon, label, hint) = match self.asking(path) {
            true => (
                Icon::Thinking,
                "Asking…",
                "The agent works on your ask: show the conversation",
            ),
            false => (
                Icon::Review,
                "Ask AI…",
                "Ask the agent for something on this issue: \"take care of point 1\"",
            ),
        };
        let kind = match self.ask_open(path) {
            true => ButtonKind::Open,
            false => ButtonKind::Review,
        };
        if widgets::button(ui, kind, Some(icon), label, Some("⌘K"))
            .on_hover_text(hint)
            .clicked()
        {
            self.toggle_ask(path);
        }
    }

    /// The panel: the conversation, the agent at work, then the message to
    /// send, its leash, and Send or Stop. `⌘↩` sends.
    pub(super) fn ask_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let Some(path) = self
            .session
            .ask_panel
            .as_ref()
            .map(|panel| panel.path.clone())
        else {
            return;
        };
        let running = self.session.asks.get(&path).map(|run| {
            let step = run.progress.lock().ok().and_then(|step| step.clone());
            (run.since.elapsed(), step)
        });
        let Some(panel) = self.session.ask_panel.as_mut() else {
            return;
        };
        let detail = match (&running, &panel.chat.session) {
            (Some((elapsed, _)), _) => format!("running · {}", stopwatch(*elapsed)),
            (None, Some(_)) => "goes on in its session".to_string(),
            (None, None) => "new conversation".to_string(),
        };
        let mut action = None;
        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                review_head(ui, &format!("Ask AI on {}", file_name(&path)), &detail);
                let height = (ui.available_height() - COMPOSER_HEIGHT).max(80.0);
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .id_salt("ask_chat")
                        .max_height(height)
                        .content_margin(theme::scroll_margin(theme::SPACE_4, theme::SPACE_4))
                        .stick_to_bottom(true)
                        .auto_shrink(false),
                    true,
                    |ui| {
                        chat_turns(ui, &panel.chat.turns, &mut panel.cache);
                        if let Some((elapsed, step)) = running.clone() {
                            running_line(ui, elapsed, step);
                        }
                    },
                );
                Frame::new()
                    .inner_margin(Margin::same(theme::SPACE_4 as i8))
                    .show(ui, |ui| action = composer(ui, panel, running.is_some()));
            });
        match action {
            Some(PanelAction::Send) => self.send_ask(),
            Some(PanelAction::Stop) => self.stop_ask(&path),
            Some(PanelAction::Terminal) => self.continue_in_terminal(&path),
            None => {}
        }
    }
}

/// Each turn under who said it, as rendered markdown.
fn chat_turns(ui: &mut egui::Ui, turns: &[Turn], cache: &mut CommonMarkCache) {
    let p = theme::palette(ui);
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Body, theme::read_body());
    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
    if turns.is_empty() {
        ui.label(
            RichText::new("Ask the agent for something on this issue. Each answer goes on in the same session.")
                .color(p.ink_muted),
        );
    }
    for (at, turn) in turns.iter().enumerate() {
        let you = turn.who == "you";
        if at > 0 {
            ui.add_space(theme::SPACE_3);
        }
        let (who, color) = match you {
            true => ("You", p.ink_muted),
            false => ("Agent", p.review),
        };
        ui.label(RichText::new(who).font(theme::meta()).color(color).strong());
        ui.push_id(("turn", at), |ui| {
            CommonMarkViewer::new().show(ui, cache, &turn.text);
        });
    }
}

/// The message, its leash, and Send or Stop.
fn composer(ui: &mut egui::Ui, panel: &mut AskPanel, running: bool) -> Option<PanelAction> {
    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
    let hint = match panel.chat.turns.is_empty() {
        true => "Take care of point 1…",
        false => "Answer the agent…",
    };
    let field = ui.add(
        egui::TextEdit::multiline(&mut panel.request)
            .hint_text(hint)
            .desired_rows(3)
            .desired_width(ui.available_width()),
    );
    if std::mem::take(&mut panel.focus) {
        field.request_focus();
    }
    widgets::checkbox(
        ui,
        &mut panel.read_only,
        "Read-only: answer, change nothing",
    );
    widgets::checkbox(ui, &mut panel.no_commit, "Prevent commit / push");
    widgets::checkbox(ui, &mut panel.no_deploy, "Prevent deploy");
    let ready = !running && !panel.request.trim().is_empty();
    let mut action = None;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let send = ui
            .add_enabled_ui(ready, |ui| {
                widgets::button(ui, ButtonKind::Send, None, "Send", Some("⌘↩"))
            })
            .inner;
        if send.clicked() {
            action = Some(PanelAction::Send);
        }
        if running && widgets::button(ui, ButtonKind::Secondary, None, "Stop", None).clicked() {
            action = Some(PanelAction::Stop);
        }
        let resumable = !running && panel.chat.session.is_some();
        let terminal = ui
            .add_enabled_ui(resumable, |ui| {
                widgets::button(
                    ui,
                    ButtonKind::Secondary,
                    None,
                    "Continue in terminal",
                    None,
                )
            })
            .inner
            .on_hover_text("Go on with this Claude Code session in Terminal, from the project")
            .on_disabled_hover_text("Once the agent answered, with Claude Code");
        if terminal.clicked() {
            action = Some(PanelAction::Terminal);
        }
    });
    if ready && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter)) {
        action = Some(PanelAction::Send);
    }
    action
}
