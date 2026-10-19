//! Refinement: a conversation with an agent that challenges an issue until it
//! is a clear spec worth building, or plainly not worth building.

use super::review::running_line;
use super::*;
use iboard::markdown;

/// The panel's widest, on a large window.
const PANEL_WIDTH: f32 = 1080.0;

/// Under this width the panes stack and the reasoning folds under the TL;DR.
const NARROW_BELOW: f32 = 820.0;

/// The width of the choices beside the reasoning.
const PICK_WIDTH: std::ops::RangeInclusive<f32> = 320.0..=420.0;

/// Past this height the reasoning scrolls on its own.
const WHY_MAX_H: f32 = 520.0;

/// About 68 characters of `read-body`: the longest line worth reading.
const MEASURE: f32 = 560.0;

/// The head and the foot around the panes.
const CHROME_H: f32 = 36.0 + 41.0;

const NUMBERS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

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
    /// The challenger's advice at a glance, above its turn.
    pub(super) tldr: Option<String>,
    pub(super) text: String,
    /// The resolutions the challenger offers, each `title: detail`.
    pub(super) options: Vec<String>,
    /// The refined issue the challenger proposed with this turn.
    pub(super) proposal: Option<String>,
}

impl Turn {
    /// The turn as the agent reads it again.
    fn said(&self) -> String {
        match &self.tldr {
            Some(tldr) => format!("TLDR: {tldr}\n\n{}", self.text),
            None => self.text.clone(),
        }
    }
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
    running: Option<(Job<Result<String, String>>, agent::Progress)>,
    error: Option<String>,
    /// Tucked away to read the page; the conversation goes on behind.
    pub(super) hidden: bool,
}

impl Refinement {
    pub(super) fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// The resolutions the challenger's last turn offers, while it waits.
    pub(super) fn options(&self) -> &[String] {
        match self.turns.last() {
            Some(turn) if turn.by == Speaker::Challenger && !self.is_running() => &turn.options,
            _ => &[],
        }
    }

    /// The latest refined issue the challenger proposed.
    pub(super) fn proposal(&self) -> Option<&str> {
        self.turns
            .iter()
            .rev()
            .find_map(|turn| turn.proposal.as_deref())
    }

    /// What Send hands the challenger: the choice's full text, then the note.
    fn answer(&self) -> Option<String> {
        let note = self.note.trim();
        match (self.choice.and_then(|at| self.options().get(at)), note) {
            (Some(choice), "") => Some(choice.clone()),
            (Some(choice), note) => Some(format!("{choice}\n\n{note}")),
            (None, "") => None,
            (None, note) => Some(note.to_string()),
        }
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
            note: String::new(),
            choice: None,
            cursor: None,
            reasoning_open: false,
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
        let said: Vec<(&str, String)> = refine
            .turns
            .iter()
            .map(|turn| (turn.by.name(), turn.said()))
            .collect();
        let conversation: Vec<prompts::Turn> =
            said.iter().map(|(by, text)| (*by, text.as_str())).collect();
        let prompt = prompts::refine_prompt(&content, &conversation, &notes);
        refine.error = None;
        refine.running = Some(agent::start_review(
            &self.agent,
            &prompt,
            &path,
            ReviewMode::Quick,
        ));
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
        refine.turns.push(Turn {
            by: Speaker::Author,
            tldr: None,
            text: answer,
            options: Vec::new(),
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
                let challenge = agent::parse_refinement(&raw);
                refine.reasoning_open = false;
                refine.turns.push(Turn {
                    by: Speaker::Challenger,
                    tldr: challenge.tldr,
                    text: challenge.text,
                    options: challenge.options,
                    proposal: challenge.proposal,
                });
            }
            Err(error) => refine.error = Some(error),
        }
    }

    /// Stops the challenger's turn (dropping the job stops the agent): the
    /// author's last answer comes back, its choice picked again. Stopped
    /// before its first word, the refinement closes.
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
            let chosen = refine
                .options()
                .iter()
                .position(|option| turn.text.starts_with(option.as_str()));
            refine.note = match chosen {
                Some(at) => turn.text[refine.options()[at].len()..].trim().to_string(),
                None => turn.text,
            };
            refine.choice = chosen;
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
            let p = theme::palette(ui);
            let window = ui.ctx().content_rect();
            let size = Vec2::new(
                PANEL_WIDTH.min(window.width() - 2.0 * theme::SPACE_7),
                window.height() * 0.8,
            );
            let shown = self.relative(&refine.path);
            let modal = egui::Modal::new(egui::Id::new("refine"))
                .frame(widgets::popover_frame(ui).inner_margin(Margin::ZERO))
                .backdrop_color(p.scrim)
                .show(ui.ctx(), |ui| {
                    refine_ui(ui, size, &shown, &mut refine, &mut self.reader.cache)
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
                .inner_margin(Margin::same(theme::SPACE_2 as i8))
                .show(ui, |ui| {
                    resume = widgets::button(ui, ButtonKind::Review, None, label, None).clicked();
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
    let p = theme::palette(ui);
    let keyed = refine_keys(ui, refine);
    ui.set_width(size.x);
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    refine_head(ui, shown, &refine.agent);
    let pane_h = (size.y - CHROME_H).max(4.0 * theme::CONTROL_H);
    let narrow = size.x < NARROW_BELOW;
    let mut unfold = false;
    if narrow {
        egui::ScrollArea::vertical()
            .id_salt("refine-panes")
            .max_height(pane_h)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                unfold = why_pane(ui, refine, cache, true);
                widgets::rule(ui);
                pick_pane(ui, refine, 0.0);
            });
    } else {
        ui.horizontal_top(|ui| {
            let pick_w = (size.x * 0.38).clamp(*PICK_WIDTH.start(), *PICK_WIDTH.end());
            let why = ui.allocate_ui_with_layout(
                Vec2::new(size.x - pick_w, pane_h),
                Layout::top_down(Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("refine-why")
                        .max_height(pane_h.min(WHY_MAX_H))
                        .auto_shrink([false, true])
                        .show(ui, |ui| why_pane(ui, refine, cache, false))
                        .inner
                },
            );
            unfold = why.inner;
            let pick = ui.allocate_ui_with_layout(
                Vec2::new(pick_w, pane_h),
                Layout::top_down(Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("refine-pick")
                        .max_height(pane_h)
                        .auto_shrink([false, true])
                        .show(ui, |ui| pick_pane(ui, refine, why.response.rect.height()));
                },
            );
            let rect = pick.response.rect;
            ui.painter()
                .vline(rect.min.x, rect.y_range(), egui::Stroke::new(1.0, p.line));
        });
    }
    if unfold {
        refine.reasoning_open = !refine.reasoning_open;
    }
    let foot = refine_footer(ui, refine);
    foot.or(keyed)
}

/// "Refining `todo/x.md`", the role as a pill, and the agent at the right,
/// which truncates first.
fn refine_head(ui: &mut egui::Ui, shown: &str, agent: &str) {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.review_soft)
        // Flush with the panel's rounded top: see `review_head`.
        .corner_radius(CornerRadius {
            nw: theme::RADIUS_MD - 1,
            ne: theme::RADIUS_MD - 1,
            sw: 0,
            se: 0,
        })
        .inner_margin(Margin::symmetric(theme::SPACE_5 as i8, 6))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), theme::CONTROL_H),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    let review = |text: &str, font| RichText::new(text).font(font).color(p.review);
                    ui.label(review("Refining", theme::ui_strong()));
                    let mono = egui::FontId::new(12.0, theme::edit_body().family);
                    ui.label(review(shown, mono));
                    role_pill(ui, "Challenger");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(agent).font(theme::meta()).color(p.ink_muted),
                            )
                            .truncate(),
                        );
                    });
                },
            );
        });
}

/// The agent's role: `review` text and edge on `surface`, 18pt tall.
fn role_pill(ui: &mut egui::Ui, role: &str) {
    let p = theme::palette(ui);
    let font = egui::FontId::new(11.0, theme::ui_button().family);
    let text = ui
        .painter()
        .layout_no_wrap(role.to_string(), font, p.review);
    let size = Vec2::new(text.size().x + 14.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, role));
    ui.painter().rect(
        rect,
        CornerRadius::same(theme::RADIUS_PILL),
        p.surface,
        egui::Stroke::new(1.0, p.review),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - text.size() / 2.0, text, p.review);
}

/// The earlier rounds, folded; the challenger's TL;DR, then its reasoning
/// (folded too on a narrow panel). Returns whether the fold was clicked.
fn why_pane(
    ui: &mut egui::Ui,
    refine: &Refinement,
    cache: &mut CommonMarkCache,
    narrow: bool,
) -> bool {
    let p = theme::palette(ui);
    let mut unfold = false;
    let margin = if narrow {
        Margin::symmetric(20, 16)
    } else {
        Margin::symmetric(theme::SPACE_6 as i8, 20)
    };
    Frame::new().inner_margin(margin).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = theme::SPACE_4;
        rounds(ui, refine);
        let Some((at, turn)) = refine
            .turns
            .iter()
            .enumerate()
            .rfind(|(_, turn)| turn.by == Speaker::Challenger)
        else {
            return;
        };
        if let Some(tldr) = &turn.tldr {
            tldr_block(ui, tldr);
        }
        if narrow && turn.tldr.is_some() {
            unfold = reasoning_fold(ui, &turn.text, refine.reasoning_open);
        }
        if !narrow || turn.tldr.is_none() || refine.reasoning_open {
            reasoning(ui, at, &turn.text, cache);
        }
        if turn.proposal.is_some() {
            ui.label(
                RichText::new("Proposed a refined issue: Apply to draft to read it.")
                    .font(theme::meta())
                    .color(p.review),
            );
        }
        if let Some(error) = &refine.error {
            ui.label(RichText::new(error).color(p.danger));
        }
    });
    unfold
}

/// One line per answer already sent: "Round 1: you chose Split it".
fn rounds(ui: &mut egui::Ui, refine: &Refinement) {
    let p = theme::palette(ui);
    let answers = refine
        .turns
        .iter()
        .enumerate()
        .filter(|(_, turn)| turn.by == Speaker::Author);
    for (round, (at, turn)) in answers.enumerate() {
        let offered = at
            .checked_sub(1)
            .map_or(&[][..], |before| &refine.turns[before].options);
        let line = match offered
            .iter()
            .find(|option| turn.text.starts_with(option.as_str()))
        {
            Some(option) => format!(
                "Round {}: you chose {}",
                round + 1,
                agent::choice_parts(option).0
            ),
            None => format!(
                "Round {}: you answered “{}”",
                round + 1,
                turn.text.lines().next().unwrap_or_default()
            ),
        };
        ui.add(
            egui::Label::new(RichText::new(line).font(theme::meta()).color(p.ink_muted)).truncate(),
        );
    }
}

/// The challenger's advice at a glance: `ui-title` beside a 3pt `review`
/// rule, under a `TL;DR` caption.
fn tldr_block(ui: &mut egui::Ui, tldr: &str) {
    let p = theme::palette(ui);
    let rect = Frame::new()
        .inner_margin(Margin {
            left: 15,
            right: 0,
            top: 2,
            bottom: 2,
        })
        .show(ui, |ui| {
            ui.set_max_width(MEASURE);
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            ui.label(
                RichText::new("TL;DR")
                    .font(theme::ui_label())
                    .color(p.review),
            );
            ui.label(RichText::new(tldr).font(theme::ui_title()).color(p.ink));
        })
        .response
        .rect;
    let rule = Rect::from_min_max(rect.min, Pos2::new(rect.min.x + 3.0, rect.max.y));
    ui.painter().rect_filled(rule, 0.0, p.review);
}

/// "Show the reasoning (3 paragraphs)", or "Hide the reasoning" once open.
fn reasoning_fold(ui: &mut egui::Ui, text: &str, open: bool) -> bool {
    let p = theme::palette(ui);
    let label = if open {
        "Hide the reasoning".to_string()
    } else {
        match markdown::blocks(text).len() {
            1 => "Show the reasoning (1 paragraph)".to_string(),
            count => format!("Show the reasoning ({count} paragraphs)"),
        }
    };
    ui.add(
        egui::Label::new(RichText::new(label).font(theme::ui_small()).color(p.accent))
            .sense(Sense::click()),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .clicked()
}

/// The challenger's paragraphs, in markdown at reading size and measure.
fn reasoning(ui: &mut egui::Ui, at: usize, text: &str, cache: &mut CommonMarkCache) {
    ui.scope(|ui| {
        ui.set_max_width(MEASURE);
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Body, theme::read_body());
        ui.spacing_mut().item_spacing.y = theme::SPACE_3;
        ui.push_id(("turn", at), |ui| {
            for (index, block) in markdown::blocks(text).into_iter().enumerate() {
                ui.push_id(index, |ui| {
                    CommonMarkViewer::new().show(ui, cache, &text[block]);
                });
            }
        });
    });
}

/// The choices on `bg`, then the note; while the challenger answers, what
/// it is doing instead.
fn pick_pane(
    ui: &mut egui::Ui,
    refine: &mut Refinement,
    min_height: f32,
) -> egui::InnerResponse<()> {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.bg)
        .inner_margin(Margin::symmetric(theme::SPACE_5 as i8, 20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height((min_height - 40.0).max(0.0));
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            if let Some((answer, progress)) = &refine.running {
                let step = agent::step(progress).or(Some("Challenger is thinking…".into()));
                running_line(ui, answer.elapsed(), step);
                return;
            }
            let options = refine.options().to_vec();
            if !options.is_empty() {
                question_head(ui, options.len());
            }
            for (index, option) in options.iter().enumerate() {
                let picked = refine.choice == Some(index);
                let card = choice_card(ui, index, option, picked, refine.cursor == Some(index));
                if card.clicked() {
                    refine.toggle_choice(index);
                }
            }
            note_field(ui, refine, options.len());
        })
}

/// "HOW SHOULD WE RESOLVE IT?", and the count of choices at the right.
fn question_head(ui: &mut egui::Ui, count: usize) {
    let p = theme::palette(ui);
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 16.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.label(
                RichText::new("HOW SHOULD WE RESOLVE IT?")
                    .font(theme::ui_label())
                    .extra_letter_spacing(theme::LABEL_TRACKING)
                    .color(p.ink_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{count} choices"))
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
            });
        },
    );
}

/// A choice as a bordered card: its key, its title over its detail, and a
/// radio. Picked, it takes the `accent` edge, `accent-soft` fill and key.
fn choice_card(
    ui: &mut egui::Ui,
    index: usize,
    option: &str,
    picked: bool,
    cursor: bool,
) -> egui::Response {
    let p = theme::palette(ui);
    let (title, detail) = agent::choice_parts(option);
    let pad = Vec2::new(theme::SPACE_4, 10.0);
    let (lead, radio) = (30.0, 24.0);
    let width = ui.available_width();
    let wrap = width - 2.0 * pad.x - lead - radio;
    let painter = ui.painter();
    let title_text = painter.layout(title.clone(), theme::ui_strong(), p.ink, wrap);
    let detail_text = detail
        .map(|detail| painter.layout(detail.to_string(), theme::ui_small(), p.ink_muted, wrap));
    let title_h = title_text.size().y;
    let text_h = title_h + detail_text.as_ref().map_or(0.0, |text| 2.0 + text.size().y);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, text_h + 2.0 * pad.y), Sense::CLICK);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, picked, &title)
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let (fill, edge, key) = if picked {
        (p.accent_soft, egui::Stroke::new(2.0, p.accent), p.accent)
    } else if response.hovered() {
        (
            p.surface_hover,
            egui::Stroke::new(1.0, p.line_strong),
            p.ink_muted,
        )
    } else {
        (
            p.surface,
            egui::Stroke::new(1.0, p.line_strong),
            p.ink_muted,
        )
    };
    let painter = ui.painter();
    let radius = CornerRadius::same(theme::RADIUS_MD);
    painter.rect(rect, radius, fill, edge, egui::StrokeKind::Inside);
    let top = rect.min.y + pad.y;
    let first_line = title_text
        .rows
        .first()
        .map_or(title_h, |row| row.rect().height());
    let line_center = top + first_line / 2.0;
    let edge_color = if picked { p.accent } else { p.line_strong };
    let number = (index + 1).to_string();
    widgets::paint_kbd_in(
        ui,
        &number,
        Pos2::new(rect.min.x + pad.x, line_center),
        edge_color,
        key,
    );
    let text_x = rect.min.x + pad.x + lead;
    painter.galley(Pos2::new(text_x, top), title_text, p.ink);
    if let Some(text) = detail_text {
        painter.galley(Pos2::new(text_x, top + title_h + 2.0), text, p.ink_muted);
    }
    let dot = Pos2::new(rect.max.x - pad.x - 7.0, line_center);
    if picked {
        painter.circle_filled(dot, 7.0, p.accent);
        painter.circle_filled(dot, 3.0, p.surface);
    } else {
        painter.circle(dot, 6.5, p.surface, egui::Stroke::new(1.0, p.line_strong));
    }
    if cursor {
        widgets::paint_focus_ring(ui, rect, theme::RADIUS_MD);
    }
    response
}

/// The note, whose label follows the choice: a note to it, or the answer.
fn note_field(ui: &mut egui::Ui, refine: &mut Refinement, choices: usize) {
    let p = theme::palette(ui);
    let label = match (refine.choice, choices) {
        (Some(at), _) => format!("Add a note to choice {} (optional)", at + 1),
        (None, 0) => "Your answer".to_string(),
        (None, _) => "Or answer in your own words".to_string(),
    };
    ui.add_space(theme::SPACE_2);
    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
    let label = ui.label(
        RichText::new(label)
            .font(theme::ui_small())
            .color(p.ink_muted),
    );
    ui.add(
        egui::TextEdit::multiline(&mut refine.note)
            .id(note_id())
            .font(theme::ui_body())
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .lock_focus(true),
    )
    .labelled_by(label.id);
}

/// The panel's keys, before its widgets read the input: `1`–`9` pick or
/// unpick a choice, `↑` `↓` move between them and `space` picks, `tab` goes
/// to the note and `⇧tab` back. `⌘↩` sends from anywhere.
fn refine_keys(ui: &mut egui::Ui, refine: &mut Refinement) -> Option<RefineAction> {
    if refine.is_running() {
        return None;
    }
    let in_note = ui.memory(|m| m.has_focus(note_id()));
    let elsewhere = !in_note && ui.memory(|m| m.focused().is_some());
    let count = refine.options().len();
    let none = egui::Modifiers::NONE;
    let (mut to_note, mut from_note) = (false, false);
    let send = ui.input_mut(|i| {
        if i.consume_key(egui::Modifiers::COMMAND, Key::Enter) {
            return true;
        }
        if in_note {
            from_note = i.consume_key(egui::Modifiers::SHIFT, Key::Tab);
            i.consume_key(none, Key::Tab);
            return false;
        }
        if elsewhere {
            return false;
        }
        to_note = i.consume_key(none, Key::Tab);
        if count == 0 {
            return false;
        }
        let last = count - 1;
        if i.consume_key(none, Key::ArrowDown) {
            refine.cursor = Some(refine.cursor.map_or(0, |at| (at + 1).min(last)));
        }
        if i.consume_key(none, Key::ArrowUp) {
            refine.cursor = Some(refine.cursor.map_or(0, |at| at.saturating_sub(1)));
        }
        if let Some(at) = refine.cursor.filter(|_| i.consume_key(none, Key::Space)) {
            refine.toggle_choice(at.min(last));
        }
        if let Some(at) = NUMBERS[..count.min(NUMBERS.len())]
            .iter()
            .position(|key| i.consume_key(none, *key))
        {
            refine.toggle_choice(at);
        }
        false
    });
    if to_note {
        ui.memory_mut(|m| m.request_focus(note_id()));
    }
    if from_note {
        ui.memory_mut(|m| m.surrender_focus(note_id()));
    }
    send.then_some(RefineAction::Send)
}

/// The keys that apply now on the left; on the right, Send (naming what
/// goes out), then Hide and End refining.
fn refine_footer(ui: &mut egui::Ui, refine: &Refinement) -> Option<RefineAction> {
    let p = theme::palette(ui);
    let mut action = None;
    let foot = Frame::new()
        .inner_margin(Margin::symmetric(
            theme::SPACE_5 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), theme::CONTROL_H),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    action = footer_buttons(ui, refine);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                        footer_hints(ui, refine);
                    });
                },
            );
        });
    let rect = foot.response.rect;
    ui.painter()
        .hline(rect.x_range(), rect.min.y, egui::Stroke::new(1.0, p.line));
    action
}

/// The footer's buttons, right to left: the main action first.
fn footer_buttons(ui: &mut egui::Ui, refine: &Refinement) -> Option<RefineAction> {
    let mut action = None;
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
        button(ui, ButtonKind::Ghost, "Hide", None, RefineAction::Hide);
        return action;
    }
    let send = if refine.answer().is_some() {
        ButtonKind::Send
    } else {
        ButtonKind::Disabled
    };
    button(
        ui,
        send,
        &refine.send_label(),
        Some("⌘↩"),
        RefineAction::Send,
    );
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
        ButtonKind::Secondary,
        "Hide",
        Some("esc"),
        RefineAction::Hide,
    );
    button(
        ui,
        ButtonKind::Ghost,
        "End refining",
        None,
        RefineAction::End,
    );
    action
}

/// The keys that apply where the keyboard is.
fn footer_hints(ui: &mut egui::Ui, refine: &Refinement) {
    if refine.is_running() {
        widgets::hint(ui, "esc", "stop");
        return;
    }
    if ui.memory(|m| m.has_focus(note_id())) {
        widgets::hint(ui, "⇧tab", "back to choices");
    } else if !refine.options().is_empty() {
        let keys = format!("1–{}", refine.options().len().min(NUMBERS.len()));
        widgets::hint(ui, &keys, "choose");
        widgets::hint(ui, "tab", "note");
    }
    widgets::hint(ui, "esc", "hide");
}
