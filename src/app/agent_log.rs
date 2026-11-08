//! The agent log beside an issue: what the build agent does on it while it
//! runs, and what it did once it is done.

use super::side_panel::side_panel;
use super::{
    ButtonKind, CommonMarkCache, CommonMarkViewer, CornerRadius, Frame, IboardApp, Instant, Key,
    Path, PathBuf, RichText, Vec2, clock, file_name, theme, widgets,
};
use crate::build_log::{self, Entry};

/// How the panel reads the log: the agent's messages as a conversation, or
/// every run, step and message as it was written.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LogMode {
    Conversation,
    Log,
}

/// The log open beside the reader, read again whenever its file grows.
pub(super) struct LogView {
    pub(super) path: PathBuf,
    file: PathBuf,
    len: Option<u64>,
    pub(super) text: String,
    cache: CommonMarkCache,
    pub(super) mode: LogMode,
    /// What the author is writing to the agent.
    pub(super) reply: String,
}

impl LogView {
    fn reload(&mut self) {
        let len = std::fs::metadata(&self.file).ok().map(|meta| meta.len());
        if len != self.len {
            self.len = len;
            self.text = build_log::load(&self.file).unwrap_or_default();
        }
    }
}

impl IboardApp {
    /// Whether the build agent works on `path` now or ever did.
    pub(super) fn has_agent_log(&self, path: &Path) -> bool {
        self.building_since(path).is_some()
            || build_log::path(self.board.root_of(path), path).is_file()
    }

    pub(super) fn agent_log_open(&self, path: &Path) -> bool {
        self.session
            .agent_log
            .as_ref()
            .is_some_and(|log| log.path == path)
    }

    /// Opens the issue with its agent log beside it; or says there is none.
    pub(super) fn show_agent_log(&mut self, path: PathBuf) {
        if !self.has_agent_log(&path) {
            self.session.notice = Some((
                format!("No agent built {}", file_name(&path)),
                Instant::now(),
            ));
            return;
        }
        self.open_issue(path.clone());
        if !self.agent_log_open(&path) {
            self.toggle_agent_log(&path);
        }
    }

    pub(super) fn toggle_agent_log(&mut self, path: &Path) {
        if self.agent_log_open(path) {
            if self.question_for(path).is_some() {
                self.session.notice = Some((
                    "Answer the build's question first".to_string(),
                    Instant::now(),
                ));
                return;
            }
            self.session.agent_log = None;
        } else {
            self.session.agent_log = Some(LogView {
                path: path.to_path_buf(),
                file: build_log::path(self.board.root_of(path), path),
                len: None,
                text: String::new(),
                cache: CommonMarkCache::default(),
                mode: LogMode::Conversation,
                reply: String::new(),
            });
        }
    }

    /// The log's panel: a waiting question first, then the conversation or
    /// the raw log, then a box to tell the agent what to do next.
    pub(super) fn agent_log_panel(&mut self, ui: &mut egui::Ui) {
        let Some(mut log) = self.session.agent_log.take() else {
            return;
        };
        log.reload();
        let path = log.path.clone();
        let question = self.question_for(&path);
        let running = self.building_since(&path);
        let detail = match running {
            Some(since) => format!("running · {}", clock(since.elapsed())),
            None => "not running".to_string(),
        };
        let title = format!("Agent log of {}", file_name(&path));
        let mut send = None;
        side_panel(ui, &title, &detail, |ui| {
            if let Some(question) = &question {
                let start = ui.cursor().min;
                let width = ui.available_width();
                let mut question_ui = ui.new_child(egui::UiBuilder::new().max_rect(
                    egui::Rect::from_min_size(start, Vec2::new(width, ui.available_height())),
                ));
                question_ui.set_min_size(Vec2::ZERO);
                self.question_panel(&mut question_ui, &path, question);
                let height = question_ui.min_rect().height();
                ui.add_space(height + theme::SPACE_4);
            }
            if let Some(mode) = widgets::segmented(
                ui,
                log.mode,
                &[
                    (LogMode::Conversation, "Conversation"),
                    (LogMode::Log, "Log"),
                ],
            ) {
                log.mode = mode;
            }
            ui.add_space(theme::SPACE_2);
            let height = (ui.available_height() - 78.0).max(80.0);
            theme::scroll_area(
                ui,
                egui::ScrollArea::vertical()
                    .id_salt("agent_log")
                    .max_height(height)
                    .content_margin(theme::scroll_margin(theme::SPACE_4, theme::SPACE_4))
                    .stick_to_bottom(true)
                    .auto_shrink(false),
                |ui| {
                    if log.text.is_empty() {
                        ui.label(
                            RichText::new("The agent has not said anything yet.")
                                .color(theme::palette(ui).ink_muted),
                        );
                    }
                    match log.mode {
                        LogMode::Conversation => {
                            conversation_entries(ui, &log.text, &mut log.cache)
                        }
                        LogMode::Log => log_entries(ui, &log.text, &mut log.cache),
                    }
                },
            );
            ui.add_space(theme::SPACE_2);
            send = reply_box(ui, &mut log.reply, question.is_some(), running.is_some());
        });
        if let Some(text) = send {
            log.reply.clear();
            self.send_instruction(path, text);
        }
        self.session.agent_log = Some(log);
    }

    /// Tells the running build's session what to do next, or starts an Ask
    /// when no agent is at work on the issue.
    pub(super) fn send_instruction(&mut self, path: PathBuf, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        if self.building_since(&path).is_some() {
            let Some((project, session)) = self.build_service_run(&path) else {
                self.session.notice = Some((
                    "This build runs as a child process and cannot take a reply".to_string(),
                    Instant::now(),
                ));
                return;
            };
            match crate::agent::service::say(&project, &session, &text) {
                Ok(()) => {
                    let log = build_log::path(self.board.root_of(&path), &path);
                    let _ = build_log::append(&log, &build_log::author(&text));
                    self.session.notice = Some((
                        format!("Told the agent on {}", file_name(&path)),
                        Instant::now(),
                    ));
                }
                Err(error) => self.error = Some(error),
            }
            return;
        }
        if !self.prefs.build_agent.configured() {
            self.open_config();
            return;
        }
        let log = build_log::path(self.board.root_of(&path), &path);
        let _ = build_log::append(&log, &build_log::author(&text));
        self.session.ask_draft = Some(super::ask::AskDraft::instruction(path, text));
        self.send_ask();
    }
}

/// The log read as what it is: each run under its heading, the agent's
/// messages as rendered markdown, its steps and outcomes as quiet lines.
fn log_entries(ui: &mut egui::Ui, text: &str, cache: &mut CommonMarkCache) {
    let p = theme::palette(ui);
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Body, theme::read_body());
    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
    for (at, entry) in build_log::parse(text).into_iter().enumerate() {
        match entry {
            Entry::Run(run) => {
                if at > 0 {
                    ui.add_space(theme::SPACE_3);
                    ui.separator();
                }
                ui.label(
                    RichText::new(run)
                        .font(theme::meta())
                        .color(p.review)
                        .strong(),
                );
            }
            Entry::Step(step) => {
                ui.label(
                    RichText::new(format!("→ {step}"))
                        .font(theme::meta())
                        .color(p.ink_faint),
                );
            }
            Entry::Outcome { ok, text } => {
                let (mark, color) = if ok {
                    ("✓", p.status_done)
                } else {
                    ("✗", p.danger)
                };
                ui.label(
                    RichText::new(format!("{mark} {text}"))
                        .color(color)
                        .strong(),
                );
            }
            Entry::Author(text) => {
                ui.label(
                    RichText::new(format!("You: {text}"))
                        .font(theme::ui_small())
                        .color(p.accent),
                );
            }
            Entry::Message(message) => {
                ui.push_id(("message", at), |ui| {
                    CommonMarkViewer::new().show(ui, cache, &message);
                });
            }
        }
    }
}

/// The conversation: what the agent said, what you told it, and the quiet
/// outcomes; the steps stay in the raw log.
fn conversation_entries(ui: &mut egui::Ui, text: &str, cache: &mut CommonMarkCache) {
    let p = theme::palette(ui);
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Body, theme::read_body());
    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
    let entries = build_log::parse(text);
    if let Some(step) = entries.iter().rev().find_map(|entry| match entry {
        Entry::Step(step) => Some(*step),
        _ => None,
    }) {
        ui.label(
            RichText::new(format!("→ {step}"))
                .font(theme::meta())
                .color(p.ink_faint),
        );
    }
    for (at, entry) in entries.into_iter().enumerate() {
        match entry {
            Entry::Message(message) => {
                ui.push_id(("message", at), |ui| {
                    CommonMarkViewer::new().show(ui, cache, &message);
                });
            }
            Entry::Author(text) => {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    Frame::new()
                        .fill(p.accent_soft)
                        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
                        .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
                        .show(ui, |ui| {
                            ui.set_max_width((ui.available_width() * 0.86).max(120.0));
                            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                            ui.label(
                                RichText::new("You")
                                    .font(theme::meta())
                                    .color(p.accent)
                                    .strong(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(text).font(theme::ui_small()).color(p.ink),
                                )
                                .wrap(),
                            );
                        });
                });
            }
            Entry::Outcome { ok, text } => {
                let (mark, color) = if ok {
                    ("✓", p.status_done)
                } else {
                    ("✗", p.danger)
                };
                ui.label(
                    RichText::new(format!("{mark} {text}"))
                        .font(theme::meta())
                        .color(color),
                );
            }
            Entry::Run(_) | Entry::Step(_) => {}
        }
    }
}

/// The box under the conversation: tell the agent what to do next.
fn reply_box(ui: &mut egui::Ui, text: &mut String, locked: bool, running: bool) -> Option<String> {
    let p = theme::palette(ui);
    let hint = if locked {
        "Answer the question first"
    } else if running {
        "Reply to the agent…"
    } else {
        "Ask the agent to do more…"
    };
    widgets::text_entry(
        ui,
        egui::TextEdit::multiline(text)
            .hint_text(hint)
            .desired_rows(2)
            .interactive(!locked),
    );
    let ready = !locked && !text.trim().is_empty();
    let mut send = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("⌘↩ sends")
                .font(theme::meta())
                .color(p.ink_faint),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            send = ui
                .add_enabled_ui(ready, |ui| {
                    widgets::button(ui, ButtonKind::Primary, None, "Send", Some("⌘↩"))
                })
                .inner
                .clicked();
        });
    });
    if ready
        && ui
            .ctx()
            .input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter))
    {
        send = true;
    }
    send.then(|| text.trim().to_string())
}
