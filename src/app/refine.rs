//! Refinement: a conversation with an agent that challenges an issue until it
//! is a clear spec worth building, or plainly not worth building.

use super::{
    Align, ButtonKind, CommonMarkCache, CommonMarkViewer, CornerRadius, Duration, Frame, IboardApp,
    Job, Key, Layout, Margin, Path, PathBuf, Pos2, Rect, ReviewMode, RichText, Screen, Sense,
    Stroke, Vec2, agent, job, prompts, side_panel, theme, widgets,
};
use iboard::markdown;

mod keys;
mod pick;
mod thinking;
mod why;

use keys::{NUMBERS, refine_keys};
use pick::pick_pane;
use thinking::{thinking_pick, thinking_why};
use why::{refine_head, why_pane};

/// Under this width the panes stack and the reasoning folds under the TL;DR.
const NARROW_BELOW: f32 = theme::MODAL_XL_STACK_BELOW;

/// The width of the choices beside the reasoning.
const PICK_WIDTH: std::ops::RangeInclusive<f32> = 320.0..=420.0;

/// The narrowest the reasoning gets beside the choices (`column-min`).
const WHY_MIN: f32 = 280.0;

/// The divider's hit area, centred on its 1pt line.
const INLINE_HIT: f32 = 9.0;

/// The reasoning's scrollbar keeps this far from the divider.
const SCROLL_STEP_IN: f32 = 4.0;

/// About 68 characters of `read-body`: the longest line worth reading.
const MEASURE: f32 = 560.0;

/// Stacked panes fit their content from this height, and scroll past the
/// modal's.
const STACKED_MIN_H: f32 = 320.0;

/// The head and the foot around the panes.
const CHROME_H: f32 = 36.0 + 41.0;

/// One turn of a refinement: the challenger's, or the author's answer.
pub(super) enum Turn {
    Challenger(agent::Challenge),
    /// The full text of the choice picked, if any, and the author's own words.
    Author {
        choice: Option<String>,
        note: String,
    },
}

impl Turn {
    fn speaker(&self) -> &'static str {
        match self {
            Turn::Challenger(_) => "Challenger",
            Turn::Author { .. } => "Author",
        }
    }

    /// The turn as the agent reads it again.
    pub(super) fn said(&self) -> String {
        match self {
            Turn::Challenger(challenge) => match &challenge.tldr {
                Some(tldr) => format!("TLDR: {tldr}\n\n{}", challenge.text),
                None => challenge.text.clone(),
            },
            Turn::Author { choice, note } => match (choice, note.as_str()) {
                (Some(choice), "") => choice.clone(),
                (Some(choice), note) => format!("{choice}\n\n{note}"),
                (None, note) => note.to_string(),
            },
        }
    }

    pub(super) fn challenge(&self) -> Option<&agent::Challenge> {
        match self {
            Turn::Challenger(challenge) => Some(challenge),
            Turn::Author { .. } => None,
        }
    }
}

/// The challenger's next turn, until it is in.
enum NextTurn {
    Waiting(Job<Result<String, String>>),
    Failed(String),
}

/// The refinement of one issue: what was said, and the turn on its way.
pub(super) struct Refinement {
    pub(super) path: PathBuf,
    /// The configured tool (and model), shown in the panel header.
    agent: String,
    pub(super) turns: Vec<Turn>,
    /// The author's own words: a note to the choice, or the whole answer.
    pub(super) note: String,
    /// The choice picked among the challenger's last options.
    pub(super) choice: Option<usize>,
    /// The choice the arrow keys are on.
    cursor: Option<usize>,
    /// On a narrow panel, the reasoning unfolded under the TL;DR.
    reasoning_open: bool,
    next: Option<NextTurn>,
    /// Tucked away to read the page; the conversation goes on behind.
    pub(super) hidden: bool,
}

impl Refinement {
    pub(super) fn is_running(&self) -> bool {
        matches!(self.next, Some(NextTurn::Waiting(_)))
    }

    /// The round on screen: one more than the answers sent, counting the
    /// one the challenger is reading.
    fn round(&self) -> usize {
        let sent = self
            .turns
            .iter()
            .filter(|turn| matches!(turn, Turn::Author { .. }))
            .count();
        if self.is_running() {
            sent.max(1)
        } else {
            sent + 1
        }
    }

    /// The resolutions the challenger's last turn offers, while it waits.
    pub(super) fn options(&self) -> &[String] {
        match self.turns.last().and_then(Turn::challenge) {
            Some(challenge) if !self.is_running() => &challenge.options,
            _ => &[],
        }
    }

    /// The latest refined issue the challenger proposed.
    pub(super) fn proposal(&self) -> Option<&str> {
        self.turns
            .iter()
            .rev()
            .find_map(|turn| turn.challenge()?.proposal.as_deref())
    }

    /// What Send hands the challenger: the choice's full text, then the note.
    fn answer(&self) -> Option<Turn> {
        let choice = self.choice.and_then(|at| self.options().get(at)).cloned();
        let note = self.note.trim().to_string();
        (choice.is_some() || !note.is_empty()).then_some(Turn::Author { choice, note })
    }

    /// Send's label, naming what goes out.
    fn send_label(&self) -> String {
        let choice = self.choice.filter(|at| *at < self.options().len());
        match (choice, self.note.trim().is_empty()) {
            (Some(at), true) => format!("Send choice {}", at + 1),
            (Some(at), false) => format!("Send choice {} with note", at + 1),
            (None, _) => "Send answer".to_string(),
        }
    }

    /// Picks a choice, or unpicks it when it is the one picked.
    pub(super) fn toggle_choice(&mut self, index: usize) {
        if index < self.options().len() {
            self.choice = (self.choice != Some(index)).then_some(index);
        }
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
    End,
}

impl IboardApp {
    /// Opens a refinement of a Todo issue: the challenger speaks first. A
    /// conversation already held on it comes back instead.
    pub(super) fn start_refinement(&mut self, path: &Path) {
        if !self.prefs.agent.configured() {
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
            agent: self.prefs.agent.label(),
            turns: Vec::new(),
            note: String::new(),
            choice: None,
            cursor: None,
            reasoning_open: false,
            next: None,
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
        let said: Vec<(&str, String)> = refine
            .turns
            .iter()
            .map(|turn| (turn.speaker(), turn.said()))
            .collect();
        let conversation: Vec<prompts::Turn> =
            said.iter().map(|(by, text)| (*by, text.as_str())).collect();
        let prompt = prompts::refine_prompt(&content, &conversation, &notes);
        let project = Some(self.board.project_of(&path));
        let (answer, _) = agent::start_review(
            &self.prefs.agent,
            &prompt,
            &path,
            project,
            ReviewMode::Quick,
        );
        refine.next = Some(NextTurn::Waiting(answer));
    }

    /// Sends the picked choice and the note, once the challenger is done
    /// speaking.
    pub(super) fn send_reply(&mut self) {
        let Some(refine) = self.session.refine.as_mut() else {
            return;
        };
        let Some(answer) = refine.answer().filter(|_| !refine.is_running()) else {
            return;
        };
        refine.note.clear();
        refine.choice = None;
        refine.cursor = None;
        refine.turns.push(answer);
        self.ask_challenger();
    }

    /// Adopts the challenger's turn once it is in; called every frame.
    pub(super) fn poll_refinement(&mut self) -> Option<Duration> {
        let refine = self.session.refine.as_mut()?;
        let Some(NextTurn::Waiting(answer)) = &refine.next else {
            return None;
        };
        let Some(result) = answer.answer() else {
            return Some(job::POLL_EVERY);
        };
        refine.next = None;
        match result {
            Ok(raw) => {
                refine.reasoning_open = false;
                refine
                    .turns
                    .push(Turn::Challenger(agent::parse_refinement(&raw)));
            }
            Err(error) => refine.next = Some(NextTurn::Failed(error)),
        }
        None
    }

    /// Stops the challenger's turn (dropping the job stops the agent): the
    /// author's last answer comes back, its choice picked again. Stopped
    /// before its first word, the refinement closes.
    pub(super) fn stop_refinement(&mut self) {
        let Some(refine) = self.session.refine.as_mut() else {
            return;
        };
        refine.next = None;
        if let Some(Turn::Author { choice, note }) = refine
            .turns
            .pop_if(|turn| matches!(turn, Turn::Author { .. }))
        {
            refine.choice = choice
                .and_then(|choice| refine.options().iter().position(|option| *option == choice));
            refine.note = note;
        }
        if refine.turns.is_empty() {
            self.session.refine = None;
        }
    }

    /// Whether the popin is up, on the issue in the reader.
    pub(super) fn refinement_shown(&self) -> bool {
        self.session.refine.as_ref().is_some_and(|refine| {
            !refine.hidden
                && matches!(&self.session.screen, Screen::Detail(path, _) if *path == refine.path)
        })
    }

    /// `esc`: stops a turn on its way, else hides the panel, unless a field
    /// has the keyboard (the note gives it up instead).
    pub(super) fn escape_refinement(&mut self, ctx: &egui::Context) {
        if self
            .session
            .refine
            .as_ref()
            .is_some_and(Refinement::is_running)
        {
            self.stop_refinement();
        } else if ctx.memory(|m| m.focused().is_none())
            && let Some(refine) = self.session.refine.as_mut()
        {
            refine.hidden = true;
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
        self.offer_to_editor(&path, proposal);
        if let Some(refine) = self.session.refine.as_mut() {
            refine.hidden = true;
        }
    }

    /// The refinement over the page: the challenger's reasoning beside the
    /// choices it offers, the note and Send (`⌘↩`). Tucked away, a pill at
    /// the foot of the page brings it back.
    pub(super) fn refine_popin(&mut self, ui: &mut egui::Ui) {
        let Some(mut refine) = self.session.refine.take() else {
            return;
        };
        let action = if refine.hidden {
            resume_pill(ui, &refine)
        } else {
            let size = theme::modal_size(theme::ModalSize::Xl, ui.ctx().content_rect());
            let shown = self.relative(&refine.path);
            widgets::modal(ui, "refine", theme::ModalSize::Xl, "Refine")
                .show(ui.ctx(), |ui| {
                    refine_ui(ui, size, &shown, &mut refine, &mut self.reader.cache)
                })
                .inner
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
            Some(RefineAction::End) => self.session.refine = None,
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
                .inner_margin(theme::SPACE_2)
                .show(ui, |ui| {
                    resume = widgets::button(ui, ButtonKind::ReviewHairline, None, label, None)
                        .clicked();
                });
        });
    resume.then_some(RefineAction::Show)
}

fn note_id() -> egui::Id {
    egui::Id::new("refine-note")
}

/// Head, the two panes (stacked under [`NARROW_BELOW`]) and the foot.
fn refine_ui(
    ui: &mut egui::Ui,
    size: Vec2,
    shown: &str,
    refine: &mut Refinement,
    cache: &mut CommonMarkCache,
) -> Option<RefineAction> {
    let keyed = refine_keys(ui, refine);
    widgets::mark_busy(ui, refine.is_running());
    ui.set_width(size.x);
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    refine_head(ui, shown, &refine.agent, refine.round());
    let pane_h = (size.y - CHROME_H).max(4.0 * theme::CONTROL_H);
    let narrow = size.x < NARROW_BELOW;
    let mut unfold = false;
    if narrow {
        theme::scroll_area(
            ui,
            egui::ScrollArea::vertical()
                .id_salt("refine-panes")
                .max_height(pane_h)
                .auto_shrink([false, true]),
            |ui| {
                ui.set_min_height(STACKED_MIN_H.min(pane_h));
                unfold = why_pane(ui, refine, cache, true);
                widgets::rule(ui);
                pick_pane(ui, refine, 0.0);
            },
        );
    } else {
        ui.horizontal_top(|ui| {
            let panes = Rect::from_min_size(ui.cursor().min, Vec2::new(size.x, pane_h));
            let default = (size.x * 0.38).clamp(*PICK_WIDTH.start(), *PICK_WIDTH.end());
            let range = *PICK_WIDTH.start()..=(size.x - WHY_MIN).min(*PICK_WIDTH.end());
            let split = theme::pane_split_id("refine");
            let pick_w =
                theme::pane_split(ui.ctx(), split, default).clamp(*range.start(), *range.end());
            let why = ui.allocate_ui_with_layout(
                Vec2::new(size.x - pick_w - SCROLL_STEP_IN, pane_h),
                Layout::top_down(Align::Min),
                |ui| {
                    theme::scroll_area(
                        ui,
                        egui::ScrollArea::vertical()
                            .id_salt("refine-why")
                            .max_height(pane_h)
                            .auto_shrink([false, false]),
                        |ui| why_pane(ui, refine, cache, false),
                    )
                    .inner
                },
            );
            unfold = why.inner;
            ui.add_space(SCROLL_STEP_IN);
            let x = ui.cursor().min.x;
            let hit = egui::Rangef::new(x - INLINE_HIT / 2.0, x + INLINE_HIT / 2.0);
            let gutter = Rect::from_x_y_ranges(hit, why.response.rect.y_range());
            let divider = theme::pane_divider(ui, split, panes, gutter, range, default, true);
            ui.allocate_ui_with_layout(
                Vec2::new(pick_w, pane_h),
                Layout::top_down(Align::Min),
                |ui| {
                    theme::scroll_area(
                        ui,
                        egui::ScrollArea::vertical()
                            .id_salt("refine-pick")
                            .max_height(pane_h)
                            .auto_shrink([false, false]),
                        |ui| pick_pane(ui, refine, why.response.rect.height()),
                    );
                },
            );
            divider.paint(ui);
        });
    }
    if unfold {
        refine.reasoning_open = !refine.reasoning_open;
    }
    let foot = refine_footer(ui, refine);
    foot.or(keyed)
}

/// The keys that apply now on the left; on the right, Send (naming what
/// goes out), then Hide and End refining.
fn refine_footer(ui: &mut egui::Ui, refine: &Refinement) -> Option<RefineAction> {
    let p = theme::palette(ui);
    let foot = widgets::footer_bar(ui, theme::SPACE_5, |ui| {
        let action = footer_buttons(ui, refine);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_4;
            footer_hints(ui, refine);
        });
        action
    });
    let rect = foot.response.rect;
    ui.painter()
        .hline(rect.x_range(), rect.min.y, egui::Stroke::new(1.0, p.line));
    foot.inner
}

/// A footer button: its kind, label and key, and what it asks for.
type FooterButton = (ButtonKind, String, Option<&'static str>, RefineAction);

/// The footer's buttons now, right to left: the main action first.
fn footer_actions(refine: &Refinement) -> Vec<FooterButton> {
    let send = |kind: ButtonKind, label: String| (kind, label, Some("⌘↩"), RefineAction::Send);
    if refine.is_running() {
        return vec![
            send(ButtonKind::Send.when(false), "Send".into()),
            (ButtonKind::Ghost, "Cancel".into(), None, RefineAction::Stop),
        ];
    }
    let failed = matches!(refine.next, Some(NextTurn::Failed(_)));
    [
        Some(send(
            ButtonKind::Send.when(refine.answer().is_some()),
            refine.send_label(),
        )),
        failed.then(|| {
            (
                ButtonKind::Secondary,
                "Retry".into(),
                None,
                RefineAction::Retry,
            )
        }),
        refine.proposal().map(|_| {
            let label = "Apply to draft".into();
            (ButtonKind::ReviewHairline, label, None, RefineAction::Apply)
        }),
        Some((
            ButtonKind::Secondary,
            "Hide".into(),
            Some("esc"),
            RefineAction::Hide,
        )),
        Some((
            ButtonKind::Ghost,
            "End refining".into(),
            None,
            RefineAction::End,
        )),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn footer_buttons(ui: &mut egui::Ui, refine: &Refinement) -> Option<RefineAction> {
    let mut action = None;
    for (kind, label, key, then) in footer_actions(refine) {
        widgets::action_button(ui, &mut action, kind, &label, key, then);
    }
    action
}

/// The keys that apply where the keyboard is, and what they do.
fn footer_keys(refine: &Refinement, in_note: bool) -> Vec<(String, &'static str)> {
    if refine.is_running() {
        return vec![("esc".into(), "cancel")];
    }
    let count = refine.options().len();
    let mut keys = Vec::new();
    if in_note {
        keys.push(("⇧⇥".into(), "back to choices"));
    } else if count > 0 {
        keys.push((format!("1–{}", count.min(NUMBERS.len())), "choose"));
        keys.push(("⇥".into(), "note"));
    }
    keys.push(("esc".into(), "hide"));
    keys
}

/// The keys that apply where the keyboard is, while ⌘ is held.
fn footer_hints(ui: &mut egui::Ui, refine: &Refinement) {
    let keys = footer_keys(refine, ui.memory(|m| m.has_focus(note_id())));
    widgets::sliding_hints(ui, |ui| {
        for (key, what) in &keys {
            widgets::hint(ui, key, what);
        }
    });
}
