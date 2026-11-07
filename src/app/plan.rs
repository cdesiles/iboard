//! Tools › Plan…: one open conversation with the agent that can end in
//! tickets, kept as scratch in the cache folder while it lasts.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Align, Key, Layout, Modifiers, RichText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use iboard::prompts;

use super::IboardApp;
use crate::agent::{self, Chat, Tool};
use crate::files::atomic_write;
use crate::issue;
use crate::job::{self, Job};
use crate::plan::{self, Kind, Plan, Speaker};
use crate::theme;
use crate::widgets::{self, ButtonKind, MenuAction};

/// What the panel asks the app to do after a frame.
enum Action {
    Keep,
    Close,
    Send,
    Tickets,
    Stop,
    Continue,
    Fresh,
}

/// The agent's next turn: none asked, one on its way, or failed.
enum Next {
    Idle,
    Waiting {
        job: Job<Result<Chat, String>>,
        progress: agent::Progress,
        tickets: bool,
    },
    Failed(String),
}

/// The Plan panel: the conversation, the field under it, and the run in flight.
pub(super) struct Panel {
    pub(super) plan: Plan,
    /// Where the plan is kept; `None` in tests, which never touch the cache.
    file: Option<PathBuf>,
    input: String,
    /// A saved plan was found: continue it or start fresh, never resume alone.
    resume: bool,
    next: Next,
    pub(super) status: Option<String>,
    cache: CommonMarkCache,
}

impl Panel {
    pub(super) fn new(saved: Option<Plan>, file: Option<PathBuf>) -> Self {
        let resume = saved.as_ref().is_some_and(|plan| !plan.turns.is_empty());
        Panel {
            plan: saved.unwrap_or_else(Plan::fresh),
            file,
            input: String::new(),
            resume,
            next: Next::Idle,
            status: None,
            cache: CommonMarkCache::default(),
        }
    }

    fn is_running(&self) -> bool {
        matches!(self.next, Next::Waiting { .. })
    }

    #[cfg(test)]
    pub(super) fn is_resuming(&self) -> bool {
        self.resume
    }

    fn save(&self) {
        if let Some(file) = &self.file {
            let _ = plan::save(file, &self.plan);
        }
    }

    fn continue_plan(&mut self) {
        self.resume = false;
        self.save();
    }

    fn fresh(&mut self) {
        self.plan = Plan::fresh();
        self.input.clear();
        self.status = None;
        self.next = Next::Idle;
        self.resume = false;
        if let Some(file) = &self.file {
            plan::forget(file);
        }
        self.save();
    }

    fn stop(&mut self) {
        self.next = Next::Idle;
    }

    fn show(&mut self, ctx: &egui::Context, agent: &str) -> Option<Action> {
        let mut action = None;
        egui::Window::new("Plan")
            .collapsible(false)
            .resizable(true)
            .default_width(720.0)
            .default_height(620.0)
            .show(ctx, |ui| {
                action = self.body(ui, agent);
            });
        action
    }

    fn body(&mut self, ui: &mut egui::Ui, agent: &str) -> Option<Action> {
        let mut action = None;
        self.head(ui, agent, &mut action);
        ui.separator();
        if self.resume {
            return self.resume_choice(ui);
        }
        widgets::mark_busy(ui, self.is_running());
        if !self.is_running() && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter)) {
            action = Some(Action::Send);
        }
        self.transcript(ui);
        self.progress(ui);
        ui.separator();
        self.composer(ui, &mut action);
        action
    }

    fn head(&mut self, ui: &mut egui::Ui, agent: &str, action: &mut Option<Action>) {
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Plan").font(theme::ui_title()).color(p.ink));
            let changed = kind_chip(ui, &mut self.plan);
            ui.label(RichText::new(agent).font(theme::meta()).color(p.ink_muted));
            if changed {
                self.save();
                *action = Some(Action::Keep);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, ButtonKind::Ghost, None, "Close", None).clicked() {
                    *action = Some(Action::Close);
                }
            });
        });
        if let Some(kind) = self.plan.kind {
            ui.label(
                RichText::new(kind.blurb())
                    .font(theme::meta())
                    .color(p.ink_faint),
            );
        }
    }

    fn resume_choice(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let p = theme::palette(ui);
        let turns = self.plan.turns.len();
        ui.add_space(theme::SPACE_5);
        ui.label(
            RichText::new("A plan is waiting here")
                .font(theme::ui_strong())
                .color(p.ink),
        );
        ui.label(
            RichText::new(format!(
                "{turns} messages about {}.",
                self.plan.topic().unwrap_or("this work")
            ))
            .font(theme::ui_body())
            .color(p.ink_muted),
        );
        ui.add_space(theme::SPACE_4);
        let mut action = None;
        ui.horizontal(|ui| {
            if widgets::button(ui, ButtonKind::Primary, None, "Continue this plan", None).clicked()
            {
                action = Some(Action::Continue);
            }
            if widgets::button(ui, ButtonKind::Secondary, None, "Start fresh", None).clicked() {
                action = Some(Action::Fresh);
            }
        });
        action
    }

    fn transcript(&mut self, ui: &mut egui::Ui) {
        let height = (ui.available_height() - 170.0).max(160.0);
        let area = egui::ScrollArea::vertical()
            .id_salt("plan-transcript")
            .max_height(height)
            .auto_shrink([false, false]);
        theme::scroll_area(ui, area, |ui| {
            if self.plan.turns.is_empty() {
                ui.label(
                    RichText::new("What should we plan?")
                        .font(theme::ui_body())
                        .color(theme::palette(ui).ink_faint),
                );
                return;
            }
            let mut cache = std::mem::take(&mut self.cache);
            for turn in &self.plan.turns {
                turn_ui(ui, turn, &mut cache);
            }
            self.cache = cache;
        });
    }

    fn progress(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        if let Next::Waiting { progress, .. } = &self.next {
            let step = agent::step(progress);
            let text = match step {
                Some(step) => format!("Thinking… {step}"),
                None => "Thinking…".to_string(),
            };
            ui.label(RichText::new(text).font(theme::meta()).color(p.review));
        }
        if let Next::Failed(error) = &self.next {
            ui.label(RichText::new(error).font(theme::meta()).color(p.danger));
        }
        if let Some(status) = &self.status {
            ui.label(RichText::new(status).font(theme::meta()).color(p.ink_muted));
        }
    }

    fn composer(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        let p = theme::palette(ui);
        ui.add_space(theme::SPACE_3);
        widgets::text_entry(
            ui,
            egui::TextEdit::multiline(&mut self.input)
                .hint_text("Tell the agent what to think about, or ask the next question")
                .desired_width(f32::INFINITY)
                .desired_rows(3),
        );
        let running = self.is_running();
        let ready = !running && !self.input.trim().is_empty();
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if running {
                    if widgets::button(ui, ButtonKind::Secondary, None, "Cancel", None).clicked() {
                        *action = Some(Action::Stop);
                    }
                } else {
                    ui.add_enabled_ui(ready, |ui| {
                        widgets::action_button(
                            ui,
                            action,
                            ButtonKind::Primary.when(ready),
                            "Send",
                            Some("⌘↩"),
                            Action::Send,
                        );
                    });
                    let can_file = self.plan.has_exchange();
                    ui.add_enabled_ui(can_file, |ui| {
                        widgets::action_button(
                            ui,
                            action,
                            ButtonKind::Secondary.when(can_file),
                            "Turn into tickets",
                            None,
                            Action::Tickets,
                        );
                    });
                }
            });
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let hint = if running {
                    "The agent is reading"
                } else {
                    "Conversation only: nothing is filed until you ask"
                };
                ui.label(RichText::new(hint).font(theme::meta()).color(p.ink_faint));
            });
        });
    }
}

fn turn_ui(ui: &mut egui::Ui, turn: &plan::Turn, cache: &mut CommonMarkCache) {
    let p = theme::palette(ui);
    ui.add_space(theme::SPACE_3);
    match turn.speaker {
        Speaker::Author => {
            ui.label(RichText::new("You").font(theme::meta()).color(p.ink_faint));
            ui.add(egui::Label::new(RichText::new(&turn.text).color(p.ink)).wrap());
        }
        Speaker::Agent => {
            ui.label(RichText::new("Agent").font(theme::meta()).color(p.review));
            let styles = &mut ui.style_mut().text_styles;
            styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
            styles.insert(egui::TextStyle::Heading, theme::ui_title());
            styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            CommonMarkViewer::new().show(ui, cache, &turn.text);
        }
    }
    ui.add_space(theme::SPACE_3);
}

/// The kind chip: the current kind, opening the four when clicked.
fn kind_chip(ui: &mut egui::Ui, plan: &mut Plan) -> bool {
    let response = widgets::chip(ui, plan.kind().name(), true).on_hover_text("Plan kind");
    let mut picked = None;
    egui::Popup::menu(&response).width(180.0).show(|ui| {
        for kind in Kind::ALL {
            let action = MenuAction {
                name: kind.name(),
                ..MenuAction::default()
            };
            if widgets::menu_action(ui, action).clicked() {
                picked = Some(kind);
            }
        }
    });
    if let Some(kind) = picked {
        plan.set_kind(kind);
        true
    } else {
        false
    }
}

impl IboardApp {
    /// Tools › Plan…: the project's plan, or the offer to continue the one
    /// left here.
    pub(super) fn open_plan(&mut self) {
        if self.welcoming() || self.session.plan.is_some() {
            return;
        }
        self.project_menu = None;
        if self.showing_group() {
            let notice = "Plan works on one project: open it from the project menu";
            self.session.notice = Some((notice.to_string(), Instant::now()));
            return;
        }
        if !self.prefs.agent.configured() {
            self.open_config();
            return;
        }
        let file = self
            .plan_dir
            .as_ref()
            .map(|dir| plan::path_in(dir, &self.located.project_dir));
        let saved = file.as_deref().and_then(plan::load);
        self.session.plan = Some(Panel::new(saved, file));
    }

    /// The Plan window, over the board.
    pub(super) fn plan_window(&mut self, ui: &mut egui::Ui) {
        let Some(mut panel) = self.session.plan.take() else {
            return;
        };
        let action = panel.show(ui.ctx(), &self.prefs.agent.label());
        let keep = self.handle_plan_action(&mut panel, action);
        if keep {
            self.session.plan = Some(panel);
        }
    }

    fn handle_plan_action(&mut self, panel: &mut Panel, action: Option<Action>) -> bool {
        match action {
            None | Some(Action::Keep) => true,
            Some(Action::Close) => false,
            Some(Action::Send) => {
                self.send_plan(panel);
                true
            }
            Some(Action::Tickets) => {
                self.turn_plan_into_tickets(panel);
                true
            }
            Some(Action::Stop) => {
                panel.stop();
                true
            }
            Some(Action::Continue) => {
                panel.continue_plan();
                true
            }
            Some(Action::Fresh) => {
                panel.fresh();
                true
            }
        }
    }

    /// Sends what the author wrote, once the agent is free.
    fn send_plan(&mut self, panel: &mut Panel) {
        let text = panel.input.trim().to_string();
        if text.is_empty() || panel.is_running() {
            return;
        }
        panel.input.clear();
        panel.plan.ask(&text);
        panel.save();
        self.ask_plan(panel, false);
    }

    /// Asks the agent to turn the conversation into tickets.
    fn turn_plan_into_tickets(&mut self, panel: &mut Panel) {
        if panel.is_running() || !panel.plan.has_exchange() {
            return;
        }
        self.ask_plan(panel, true);
    }

    /// One plan turn: the tool continues its session when it can, else the
    /// transcript goes out again.
    fn ask_plan(&mut self, panel: &mut Panel, tickets: bool) {
        if panel.is_running() {
            return;
        }
        panel.status = None;
        let continues = self.prefs.agent.continues()
            && (self.prefs.agent.tool == Tool::Custom || panel.plan.session.is_some());
        let history = !continues || panel.plan.turns.len() <= 1;
        let prompt = prompts::plan_prompt(&panel.plan, tickets, history);
        let session = match self.prefs.agent.tool {
            Tool::Custom => Some(panel.plan.id.as_str()),
            _ => panel.plan.session.as_deref(),
        };
        let (job, progress) = agent::start_plan(
            &self.prefs.agent,
            &prompt,
            Some(&self.located.project_dir),
            session,
            tickets,
        );
        panel.next = Next::Waiting {
            job,
            progress,
            tickets,
        };
    }

    /// Adopts the agent's turn once it lands; files the tickets when that was
    /// the ask.
    pub(super) fn poll_plan(&mut self) -> Option<Duration> {
        let (tickets, result) = {
            let panel = self.session.plan.as_ref()?;
            let Next::Waiting { job, tickets, .. } = &panel.next else {
                return None;
            };
            let Some(result) = job.answer() else {
                return Some(job::POLL_EVERY);
            };
            (*tickets, result)
        };
        let panel = self.session.plan.as_mut()?;
        panel.next = Next::Idle;
        match result {
            Ok(chat) => {
                panel.plan.answer(&chat.answer);
                if self.prefs.agent.tool != Tool::Custom
                    && let Some(session) = chat.session
                {
                    panel.plan.session = Some(session);
                }
                panel.save();
                if tickets {
                    let message = self.file_plan_tickets(&chat.answer);
                    if let Some(panel) = self.session.plan.as_mut() {
                        panel.status = Some(message);
                    }
                }
            }
            Err(error) => panel.next = Next::Failed(error),
        }
        None
    }

    /// Writes the parsed tickets in Todo. Returns what to show the author.
    fn file_plan_tickets(&mut self, answer: &str) -> String {
        let Some(drafts) = agent::parse_split(answer) else {
            return "The agent's answer had no tickets: ask again, or keep talking.".to_string();
        };
        let Some(column) = self.board.todo_column() else {
            return "This board has no Todo column.".to_string();
        };
        let dir = match self.compose_dir(&column.dir) {
            Ok(dir) => dir,
            Err(err) => return err,
        };
        let mut written = Vec::new();
        for draft in &drafts {
            let title = draft.title.trim();
            if title.is_empty() {
                continue;
            }
            let (name, _) = issue::new_file_name(&dir, title);
            let path = dir.join(name);
            let text = format!("# {title}\n\n{}\n", draft.body.trim());
            if let Err(err) = atomic_write(&path, text) {
                for path in &written {
                    let _ = std::fs::remove_file(path);
                }
                return format!("Couldn't file {title}: {err}");
            }
            written.push(path);
        }
        if written.is_empty() {
            return "The agent's answer had no tickets to file.".to_string();
        }
        self.refresh();
        self.session.selected = written.first().cloned();
        self.session.scroll_to_selected = true;
        format!("Filed {} tickets in Todo.", written.len())
    }
}

#[cfg(test)]
impl IboardApp {
    /// Types `text` and presses Send, as the panel does.
    pub(super) fn send_plan_text(&mut self, text: &str) {
        let Some(mut panel) = self.session.plan.take() else {
            return;
        };
        panel.input = text.to_string();
        self.send_plan(&mut panel);
        self.session.plan = Some(panel);
    }

    /// Presses Turn into tickets.
    pub(super) fn ask_plan_tickets(&mut self) {
        let Some(mut panel) = self.session.plan.take() else {
            return;
        };
        self.turn_plan_into_tickets(&mut panel);
        self.session.plan = Some(panel);
    }
}
