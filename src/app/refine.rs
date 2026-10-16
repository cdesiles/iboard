//! Refinement: a conversation with an agent that challenges an issue until it
//! is a clear spec worth building, or plainly not worth building.

use super::review::{review_head, running_line};
use super::*;

/// Room the head, the reply field and the buttons keep around the
/// conversation.
const REPLY_ROOM: f32 = 230.0;

/// The popin's widest, on a large window.
const POPIN_WIDTH: f32 = 720.0;

/// Who said a turn of a refinement.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Speaker {
    Challenger,
    Author,
}

impl Speaker {
    fn name(self) -> &'static str {
        match self {
            Speaker::Challenger => "Challenger",
            Speaker::Author => "Author",
        }
    }
}

pub(super) struct Turn {
    pub(super) by: Speaker,
    pub(super) text: String,
    /// The refined issue the challenger proposed with this turn.
    pub(super) proposal: Option<String>,
}

/// The refinement of one issue: what was said, and the turn on its way.
pub(super) struct Refinement {
    pub(super) path: PathBuf,
    /// The configured tool (and model), shown in the panel header.
    agent: String,
    pub(super) turns: Vec<Turn>,
    /// The author's answer being typed.
    pub(super) reply: String,
    running: Option<(Job<Result<String, String>>, agent::Progress)>,
    error: Option<String>,
    /// Tucked away to read the page; the conversation goes on behind.
    pub(super) hidden: bool,
}

impl Refinement {
    pub(super) fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// The latest refined issue the challenger proposed.
    pub(super) fn proposal(&self) -> Option<&str> {
        self.turns
            .iter()
            .rev()
            .find_map(|turn| turn.proposal.as_deref())
    }
}

/// What a click or a key in the refinement popin asks for.
enum RefineAction {
    Send,
    Apply,
    Stop,
    Retry,
    Hide,
    Show,
    Dismiss,
}

impl IboardApp {
    /// Opens a refinement of a Todo issue: the challenger speaks first. A
    /// conversation already held on it comes back instead.
    pub(super) fn start_refinement(&mut self, path: &Path) {
        if !self.agent.configured() {
            self.open_config();
            return;
        }
        if !self.is_todo_issue(path) {
            return;
        }
        if let Some(refine) = self.session.refine.as_mut().filter(|r| r.path == path) {
            refine.hidden = false;
            return;
        }
        self.session.review = None;
        self.session.refine = Some(Refinement {
            path: path.to_path_buf(),
            agent: self.agent.label(),
            turns: Vec::new(),
            reply: String::new(),
            running: None,
            error: None,
            hidden: false,
        });
        self.ask_challenger();
    }

    /// Hands the issue, as it reads now (the editor draft when it is open),
    /// and the conversation so far to the challenger.
    fn ask_challenger(&mut self) {
        let Some(path) = self.session.refine.as_ref().map(|r| r.path.clone()) else {
            return;
        };
        let Some(content) = self
            .review_content(&path, true)
            .or_else(|| self.review_content(&path, false))
        else {
            return;
        };
        let notes = self.reviewer_notes(&path);
        self.mark_notes_sent(&path);
        let Some(refine) = self.session.refine.as_mut() else {
            return;
        };
        let conversation: Vec<prompts::Turn> = refine
            .turns
            .iter()
            .map(|turn| (turn.by.name(), turn.text.as_str()))
            .collect();
        let prompt = prompts::refine_prompt(&content, &conversation, &notes);
        refine.error = None;
        refine.running = Some(agent::start_review(
            &self.agent,
            &prompt,
            &path,
            ReviewMode::Quick,
        ));
    }

    /// Sends the author's answer, once the challenger is done speaking.
    pub(super) fn send_reply(&mut self) {
        let Some(refine) = self.session.refine.as_mut() else {
            return;
        };
        let reply = refine.reply.trim().to_string();
        if reply.is_empty() || refine.is_running() {
            return;
        }
        refine.reply.clear();
        refine.turns.push(Turn {
            by: Speaker::Author,
            text: reply,
            proposal: None,
        });
        self.ask_challenger();
    }

    /// Adopts the challenger's turn once it is in; called every frame.
    pub(super) fn poll_refinement(&mut self) {
        let Some(refine) = self.session.refine.as_mut() else {
            return;
        };
        let Some(result) = refine
            .running
            .as_ref()
            .and_then(|(answer, _)| answer.answer())
        else {
            return;
        };
        refine.running = None;
        match result {
            Ok(raw) => {
                let (text, proposal) = agent::parse_refinement(&raw);
                refine.turns.push(Turn {
                    by: Speaker::Challenger,
                    text,
                    proposal,
                });
            }
            Err(error) => refine.error = Some(error),
        }
    }

    /// Stops the challenger's turn (dropping the job stops the agent): the
    /// author's last answer goes back into the field. Stopped before its
    /// first word, the refinement closes.
    pub(super) fn stop_refinement(&mut self) {
        let Some(refine) = self.session.refine.as_mut() else {
            return;
        };
        refine.running = None;
        if refine
            .turns
            .last()
            .is_some_and(|turn| turn.by == Speaker::Author)
            && let Some(turn) = refine.turns.pop()
        {
            refine.reply = turn.text;
        }
        if refine.turns.is_empty() {
            self.session.refine = None;
        }
    }

    /// Whether the popin is up, on the issue in the reader.
    pub(super) fn refinement_shown(&self) -> bool {
        self.session.refine.as_ref().is_some_and(|refine| {
            !refine.hidden
                && matches!(&self.session.screen, Screen::Detail(path) if *path == refine.path)
        })
    }

    /// `esc`: stops a turn on its way, else closes the conversation, unless
    /// a field has the keyboard (the reply field gives it up instead).
    pub(super) fn escape_refinement(&mut self, ctx: &egui::Context) {
        if self
            .session
            .refine
            .as_ref()
            .is_some_and(Refinement::is_running)
        {
            self.stop_refinement();
        } else if ctx.memory(|m| m.focused().is_none()) {
            self.session.refine = None;
        }
    }

    /// The challenger's latest proposal, in the editor as an unsaved draft.
    /// The popin tucks away to show it; the conversation reads it next.
    pub(super) fn apply_refinement(&mut self) {
        let Some(refine) = self.session.refine.as_ref() else {
            return;
        };
        let path = refine.path.clone();
        let Some(proposal) = refine.proposal().map(str::to_string) else {
            return;
        };
        self.start_editing(&path);
        if let Some(editor) = self.session.editor.as_mut()
            && editor.path == path
        {
            editor.text = proposal;
            editor.last_edit = Some(Instant::now());
        }
        if let Some(refine) = self.session.refine.as_mut() {
            refine.hidden = true;
        }
    }

    /// The refinement over the page, as a popin: the conversation, the reply
    /// field (`⌘↩` sends) and the actions on it. Tucked away, a pill at the
    /// foot of the page brings it back.
    pub(super) fn refine_popin(&mut self, ui: &mut egui::Ui) {
        let Some(mut refine) = self.session.refine.take() else {
            return;
        };
        let action = if refine.hidden {
            resume_pill(ui, &refine)
        } else {
            let p = theme::palette(ui);
            let window = ui.ctx().content_rect();
            let size = Vec2::new(
                POPIN_WIDTH.min(window.width() - 2.0 * theme::SPACE_7),
                window.height() * 0.8,
            );
            let modal = egui::Modal::new(egui::Id::new("refine"))
                .frame(widgets::popover_frame(ui))
                .backdrop_color(p.scrim)
                .show(ui.ctx(), |ui| {
                    refine_ui(ui, size, &mut refine, &mut self.reader.cache)
                });
            modal.inner.or_else(|| {
                modal
                    .backdrop_response
                    .clicked()
                    .then_some(RefineAction::Hide)
            })
        };
        self.session.refine = Some(refine);
        match action {
            Some(RefineAction::Send) => self.send_reply(),
            Some(RefineAction::Apply) => self.apply_refinement(),
            Some(RefineAction::Stop) => self.stop_refinement(),
            Some(RefineAction::Retry) => self.ask_challenger(),
            Some(RefineAction::Hide | RefineAction::Show) => {
                if let Some(refine) = self.session.refine.as_mut() {
                    refine.hidden = matches!(action, Some(RefineAction::Hide));
                }
            }
            Some(RefineAction::Dismiss) => self.session.refine = None,
            None => {}
        }
    }
}

/// The tucked-away refinement, floating at the foot of the window.
fn resume_pill(ui: &mut egui::Ui, refine: &Refinement) -> Option<RefineAction> {
    let label = if refine.is_running() {
        "Refining…"
    } else {
        "Back to the refinement"
    };
    let mut resume = false;
    egui::Area::new(egui::Id::new("refine-resume"))
        .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -theme::SPACE_7))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            widgets::popover_frame(ui)
                .inner_margin(Margin::same(theme::SPACE_2 as i8))
                .show(ui, |ui| {
                    resume = widgets::button(ui, ButtonKind::Review, None, label, None).clicked();
                });
        });
    resume.then_some(RefineAction::Show)
}

fn refine_ui(
    ui: &mut egui::Ui,
    size: Vec2,
    refine: &mut Refinement,
    cache: &mut CommonMarkCache,
) -> Option<RefineAction> {
    let p = theme::palette(ui);
    let mut action = None;
    ui.set_width(size.x);
    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
    let title = format!("Refining {}", file_name(&refine.path));
    review_head(ui, &title, &format!("{} · challenger", refine.agent));
    Frame::new()
        .inner_margin(Margin::same(theme::SPACE_4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .max_height((size.y - REPLY_ROOM).max(theme::CONTROL_H))
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show(ui, |ui| conversation(ui, refine, cache));
            if let Some(error) = &refine.error {
                ui.label(RichText::new(error).color(p.danger));
            }
            ui.add_space(theme::SPACE_3);
            let field = ui.add(
                egui::TextEdit::multiline(&mut refine.reply)
                    .hint_text(RichText::new("Answer the challenger…").color(p.ink_faint))
                    .font(theme::ui_body())
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            if field.has_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter))
            {
                action = Some(RefineAction::Send);
            }
        });
    Frame::new()
        .inner_margin(Margin::symmetric(
            theme::SPACE_4 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            action = refine_footer(ui, refine).or(action.take());
        });
    action
}

/// Each turn under its speaker; the challenger's in markdown, with a note
/// when it came with a refined issue.
fn conversation(ui: &mut egui::Ui, refine: &Refinement, cache: &mut CommonMarkCache) {
    let p = theme::palette(ui);
    for (index, turn) in refine.turns.iter().enumerate() {
        let (who, color) = match turn.by {
            Speaker::Challenger => ("Challenger", p.review),
            Speaker::Author => ("You", p.ink_muted),
        };
        ui.label(RichText::new(who).font(theme::meta()).color(color));
        match turn.by {
            Speaker::Challenger => {
                ui.push_id(("turn", index), |ui| {
                    CommonMarkViewer::new().show(ui, cache, &turn.text);
                });
            }
            Speaker::Author => {
                ui.label(RichText::new(&turn.text).color(p.ink));
            }
        }
        if turn.proposal.is_some() {
            ui.label(
                RichText::new("Proposed a refined issue: Apply to draft to read it.")
                    .font(theme::meta())
                    .color(p.review),
            );
        }
        ui.add_space(theme::SPACE_4);
    }
    if let Some((answer, progress)) = &refine.running {
        let step = agent::step(progress).or(Some("The challenger is thinking…".into()));
        running_line(ui, answer.elapsed(), step);
    }
}

/// The panel's buttons, right to left: the main action first.
fn refine_footer(ui: &mut egui::Ui, refine: &Refinement) -> Option<RefineAction> {
    let mut action = None;
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), theme::CONTROL_H),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            let mut button = |ui: &mut egui::Ui, kind, label: &str, key, picked| {
                if widgets::button(ui, kind, None, label, key).clicked() {
                    action = Some(picked);
                }
            };
            if refine.is_running() {
                button(
                    ui,
                    ButtonKind::Secondary,
                    "Stop",
                    Some("esc"),
                    RefineAction::Stop,
                );
                return;
            }
            let send = if refine.reply.trim().is_empty() {
                ButtonKind::Disabled
            } else {
                ButtonKind::Primary
            };
            button(ui, send, "Send", Some("⌘↩"), RefineAction::Send);
            if refine.error.is_some() {
                button(
                    ui,
                    ButtonKind::Secondary,
                    "Retry",
                    None,
                    RefineAction::Retry,
                );
            }
            if refine.proposal().is_some() {
                button(
                    ui,
                    ButtonKind::Review,
                    "Apply to draft",
                    None,
                    RefineAction::Apply,
                );
            }
            button(
                ui,
                ButtonKind::Ghost,
                "Dismiss",
                None,
                RefineAction::Dismiss,
            );
            button(ui, ButtonKind::Ghost, "Hide", None, RefineAction::Hide);
        },
    );
    action
}
