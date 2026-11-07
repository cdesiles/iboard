//! Refinement: a conversation with an agent that challenges an issue until it
//! is a clear spec worth building, or plainly not worth building.

use super::{
    Align, ButtonKind, CommonMarkCache, CommonMarkViewer, CornerRadius, Duration, Frame, IboardApp,
    Key, Layout, Margin, Path, Pos2, Rect, ReviewMode, RichText, Screen, Sense, Stroke, Vec2,
    agent, prompts, side_panel, theme, widgets,
};
use iboard::markdown;
pub(super) use iboard::refinement::{NextTurn, Refinement, Turn};

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

/// About 68 characters of `read-body`: the longest line worth reading.
const MEASURE: f32 = 560.0;

/// Stacked panes fit their content from this height, and scroll past the
/// modal's.
const STACKED_MIN_H: f32 = 320.0;

/// The head and the foot around the panes.
const CHROME_H: f32 = 36.0 + 41.0;

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
        self.session.refine = Some(Refinement::new(
            path.to_path_buf(),
            self.prefs.agent.label(),
        ));
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
        refine.next = NextTurn::Waiting(answer);
    }

    /// Sends the picked choice and the note, once the challenger is done
    /// speaking.
    pub(super) fn send_reply(&mut self) {
        if self
            .session
            .refine
            .as_mut()
            .is_some_and(Refinement::take_answer)
        {
            self.ask_challenger();
        }
    }

    /// Adopts the challenger's turn once it is in; called every frame.
    pub(super) fn poll_refinement(&mut self) -> Option<Duration> {
        self.session.refine.as_mut()?.adopt()
    }

    /// Stops the challenger's turn; stopped before its first word, the
    /// refinement closes.
    pub(super) fn stop_refinement(&mut self) {
        if self.session.refine.as_mut().is_some_and(Refinement::stop) {
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
        let split = theme::Split {
            id: theme::pane_split_id("refine"),
            size: Vec2::new(size.x, pane_h),
            default: (size.x * 0.38).clamp(*PICK_WIDTH.start(), *PICK_WIDTH.end()),
            range: *PICK_WIDTH.start()..=(size.x - WHY_MIN).min(*PICK_WIDTH.end()),
            inline: true,
        };
        (unfold, _) = theme::split_panes(
            ui,
            split,
            (&mut *refine, cache),
            |ui, (refine, cache)| {
                let why = theme::pane_scroll("refine-why");
                theme::scroll_area(ui, why, |ui| why_pane(ui, refine, cache, false)).inner
            },
            |ui, (refine, _)| {
                let pick = theme::pane_scroll("refine-pick");
                theme::scroll_area(ui, pick, |ui| pick_pane(ui, refine, pane_h));
            },
        );
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
    let failed = matches!(refine.next, NextTurn::Failed(_));
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
