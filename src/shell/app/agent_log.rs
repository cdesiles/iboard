//! The agent log beside an issue: what the build agent does on it while it
//! runs, and what it did once it is done.

use super::side_panel::side_panel;
use super::{
    ButtonKind, CommonMarkCache, Frame, IboardApp, Instant, Key, MarginF32, Path, PathBuf,
    RichText, file_name, job, theme, widgets,
};
use crate::build_log;
use crate::diff::{self, FileDiff};
use crate::job::Job;

mod timeline;

/// How the panel reads the log: the runs as a timeline of what the agent
/// said and did, or every line as it was written.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LogMode {
    Timeline,
    Raw,
}

/// The log open beside the reader, read again whenever its file grows.
pub(super) struct LogView {
    pub(super) path: PathBuf,
    file: PathBuf,
    /// The project the log's changes are read from.
    dir: PathBuf,
    len: Option<u64>,
    pub(super) text: String,
    cache: CommonMarkCache,
    pub(super) mode: LogMode,
    /// The working tree, read off the UI thread.
    changes: Option<Job<Result<Vec<FileDiff>, String>>>,
    files: Vec<FileDiff>,
    /// A newer log landed while the working tree was being read.
    stale: bool,
    /// What the author is writing to the agent.
    pub(super) reply: String,
}

impl LogView {
    fn reload(&mut self) {
        let len = std::fs::metadata(&self.file).ok().map(|meta| meta.len());
        if len == self.len {
            return;
        }
        self.len = len;
        self.text = build_log::load(&self.file).unwrap_or_default();
        self.reload_changes();
    }

    /// Reads the working tree again, unless a read is already on its way.
    fn reload_changes(&mut self) {
        if self.changes.is_some() {
            self.stale = true;
            return;
        }
        let dir = self.dir.clone();
        self.changes = Some(Job::spawn(move |cancel| {
            diff::worktree(&dir, cancel).map(|set| set.files)
        }));
    }

    fn poll_changes(&mut self) {
        let Some(answer) = self.changes.as_ref().and_then(Job::answer) else {
            return;
        };
        self.changes = None;
        self.files = answer.unwrap_or_default();
        if std::mem::take(&mut self.stale) {
            self.reload_changes();
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
            self.session.agent_log = None;
        } else {
            self.session.agent_log = Some(LogView {
                path: path.to_path_buf(),
                file: build_log::path(self.board.root_of(path), path),
                dir: self.board.project_of(path).to_path_buf(),
                len: None,
                text: String::new(),
                cache: CommonMarkCache::default(),
                mode: LogMode::Timeline,
                changes: None,
                files: Vec::new(),
                stale: false,
                reply: String::new(),
            });
        }
    }

    /// The waiting question above the switch, and the room it took.
    fn question_card(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        question: Option<&crate::agent::service::Question>,
    ) {
        if let Some(question) = question {
            let cap = (ui.available_height() * 0.55).max(120.0);
            let card = ui
                .scope(|ui| self.question_panel(ui, path, question, cap))
                .response;
            ui.add_space(card.rect.height() + theme::SPACE_4);
            return;
        }
        let Some(stale) = self.stale_question(path).cloned() else {
            return;
        };
        let card = ui
            .scope(|ui| self.stale_answer_card(ui, path, &stale))
            .response;
        ui.add_space(card.rect.height() + theme::SPACE_4);
    }

    /// The log's panel: a waiting question first, the switch and the live
    /// step, the timeline or the raw log, then a box to tell the agent what
    /// to do next.
    pub(super) fn agent_log_panel(&mut self, ui: &mut egui::Ui) {
        let Some(mut log) = self.session.agent_log.take() else {
            return;
        };
        log.reload();
        log.poll_changes();
        if log.changes.is_some() {
            ui.ctx().request_repaint_after(job::POLL_EVERY);
        }
        let path = log.path.clone();
        let question = self.question_for(&path);
        let running = self.building_since(&path);
        let replyable = match running {
            None => true,
            Some(_) => self
                .build_service_run(&path)
                .is_some_and(|(_, service, _)| service.takes_reply()),
        };
        let waits_here = question
            .as_ref()
            .is_some_and(|question| question.answerable());
        let state = if running.is_some() {
            Composer::Running
        } else {
            match build_log::timeline(&log.text)
                .last()
                .and_then(|run| run.outcome)
            {
                Some((false, _)) => Composer::Stopped,
                _ => Composer::Finished,
            }
        };
        let detail = if question.is_some() {
            "waiting on you"
        } else if running.is_some() {
            "running"
        } else {
            "not running"
        };
        let title = format!("Agent log of {}", file_name(&path));
        let mut send = None;
        let mut open_diff = None;
        side_panel(ui, &title, detail, |ui| {
            self.question_card(ui, &path, question.as_ref());
            ui.spacing_mut().item_spacing.y = 0.0;
            inset(ui, theme::SPACE_3, 0.0, |ui| {
                if let Some(mode) = widgets::segmented(
                    ui,
                    log.mode,
                    &[(LogMode::Timeline, "Timeline"), (LogMode::Raw, "Raw")],
                ) {
                    log.mode = mode;
                }
            });
            if let (Some(since), None) = (running, question.as_ref()) {
                let (verb, object) = timeline::live(&log.text);
                inset(ui, theme::SPACE_3, 0.0, |ui| {
                    timeline::now(ui, since, &verb, &object);
                });
            }
            let foot = egui::Id::unique("agent_log_foot");
            let foot_height = ui.data(|d| d.get_temp::<f32>(foot)).unwrap_or(FOOT_H);
            let height = (ui.available_height() - foot_height).max(80.0);
            log_body(ui, &mut log, running.is_some(), height, &mut open_diff);
            let top = ui.cursor().top();
            widgets::rule(ui);
            inset(ui, theme::SPACE_3, theme::SPACE_3, |ui| {
                ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                if waits_here {
                    ui.label(
                        RichText::new("Answer the question above to continue.")
                            .font(theme::meta())
                            .color(theme::palette(ui).ink_faint),
                    );
                } else {
                    send = reply_box(ui, &mut log.reply, state, replyable);
                }
            });
            let used = ui.cursor().top() - top;
            ui.data_mut(|d| d.insert_temp(foot, used));
        });
        if let Some(text) = send {
            log.reply.clear();
            self.send_instruction(path.clone(), text);
        }
        self.session.agent_log = Some(log);
        if let Some(open) = open_diff {
            self.open_worktree_diff(&path, Some(open.path), open.files);
        }
    }

    /// Tells the running build's session what to do next, or starts an Ask
    /// when no agent is at work on the issue.
    pub(super) fn send_instruction(&mut self, path: PathBuf, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        if self.building_since(&path).is_some() {
            let Some((project, service, session)) = self.build_service_run(&path) else {
                self.session.notice = Some((
                    "This build runs as a child process and cannot take a reply".to_string(),
                    Instant::now(),
                ));
                return;
            };
            if !service.takes_reply() {
                self.session.notice =
                    Some(("This build cannot take a reply".to_string(), Instant::now()));
                return;
            }
            match service.say(&project, &session, &text) {
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

/// The foot's height before it was ever drawn: the rule, a two-row field and
/// the Send row between their margins.
const FOOT_H: f32 = 101.0;

/// The reply field's least height: two rows of `ui-body` in their padding.
const REPLY_MIN_H: f32 = 56.0;

/// A row of the panel between its `space-4` sides.
fn inset(ui: &mut egui::Ui, top: f32, bottom: f32, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .inner_margin(MarginF32 {
            left: theme::SPACE_4,
            right: theme::SPACE_4,
            top,
            bottom,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// The one part that scrolls, `height` tall: the timeline or the raw log.
fn log_body(
    ui: &mut egui::Ui,
    log: &mut LogView,
    running: bool,
    height: f32,
    open_diff: &mut Option<timeline::OpenDiff>,
) {
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
                LogMode::Timeline => timeline::timeline(
                    ui,
                    &log.text,
                    &mut log.cache,
                    &log.files,
                    running,
                    open_diff,
                ),
                LogMode::Raw => timeline::raw(ui, &log.text),
            }
        },
    );
}

/// What the composer says it will do, from the run's state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Composer {
    Running,
    Finished,
    Stopped,
}

/// The box under the timeline: tell the agent what to do next.
fn reply_box(
    ui: &mut egui::Ui,
    text: &mut String,
    state: Composer,
    replyable: bool,
) -> Option<String> {
    let p = theme::palette(ui);
    let hint = match state {
        Composer::Running => "Reply to the agent…",
        Composer::Finished => "Ask for another pass…",
        Composer::Stopped => "Ask it to fix what failed…",
    };
    if !replyable {
        ui.label(
            RichText::new("This build cannot take a reply.")
                .font(theme::ui_small())
                .color(p.ink_muted),
        );
    }
    let field = widgets::text_entry(
        ui,
        egui::TextEdit::multiline(text)
            .hint_text(RichText::new(hint).color(p.ink_faint))
            .font(theme::ui_body())
            .margin(egui::Vec2::new(theme::SPACE_3, theme::SPACE_2))
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .min_size(egui::Vec2::new(0.0, REPLY_MIN_H))
            .interactive(replyable),
    );
    let ready = replyable && !text.trim().is_empty();
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
        && field.has_focus()
        && ui
            .ctx()
            .input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter))
    {
        send = true;
    }
    send.then(|| text.trim().to_string())
}
