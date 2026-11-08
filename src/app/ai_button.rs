//! The header's AI split button: its face follows the column, its chevron
//! opens what the agent can do on the issue. Refine and Review belong to Todo;
//! Done has none.

use super::review::{REVIEW_KEY, REVIEW_RUNNING};
use super::{ButtonKind, IboardApp, Icon, Path, ReviewMode, theme, widgets};
use widgets::{ActionTone, MenuAction};

const AI_MENU_WIDTH: f32 = 260.0;

/// What a row of the AI menu asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AiPick {
    Refine,
    Review,
    OtherScope,
    Ask,
    Settings,
}

/// What the split button's main part says and does.
struct Face {
    icon: Icon,
    label: &'static str,
    hint: &'static str,
    key: &'static str,
    enabled: bool,
}

impl IboardApp {
    /// The face while a review works on `path`: a click or its key cancels it.
    fn running_face(&self, path: &Path) -> Option<Face> {
        self.session.review_runs.is_running(path).then_some(Face {
            icon: Icon::Thinking,
            label: REVIEW_RUNNING,
            hint: "The review runs in the background: cancel it (⌘⇧R)",
            key: REVIEW_KEY,
            enabled: true,
        })
    }

    /// The face on a Todo issue: Refine, or what the running review or the
    /// hidden refinement is doing.
    fn refine_face(&self, path: &Path) -> Face {
        if let Some(face) = self.running_face(path) {
            return face;
        }
        let configured = self.prefs.agent.configured();
        let refine = self.session.refine.as_ref();
        let hidden = refine.filter(|refine| refine.path == path && refine.hidden);
        let review = (Icon::Review, "⌘R");
        let (icon, label, hint, key) = match hidden {
            Some(refine) if refine.is_running() => (
                Icon::Thinking,
                "Refining…",
                "The challenger is answering: back to the refinement (⌘R)",
                "⌘R",
            ),
            Some(_) => (
                review.0,
                "Refinement waiting",
                "The challenger waits for your answer: back to the refinement (⌘R)",
                review.1,
            ),
            None if configured => (
                review.0,
                "Refine",
                "Refine this issue in a conversation with a challenger (⌘R)",
                review.1,
            ),
            None => (
                review.0,
                "Refine",
                "No agent is set up yet: choose one in Settings › AI review (⌘, or the chevron)",
                review.1,
            ),
        };
        Face {
            icon,
            label,
            hint,
            key,
            enabled: configured,
        }
    }

    /// The split button of the issue at `path`, then what its runs have to
    /// say. An issue past Todo has no split button, only a run that was
    /// already going when it got there.
    pub(super) fn ai_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        if !self.is_todo_issue(path) {
            self.stray_run_button(ui, path);
            self.run_buttons(ui, path);
            return;
        }
        let face = self.refine_face(path);
        let (main, caret) = widgets::split_button(
            ui,
            egui::Id::unique("ai-button"),
            ButtonKind::Review,
            Some(face.icon),
            face.label,
            Some(face.key),
            face.enabled,
        );
        if main.on_hover_text(face.hint).clicked() {
            if self.review_is_running(path) {
                self.cancel_review_run(path);
            } else {
                self.start_refinement(path);
            }
        }
        let name = "AI actions";
        let caret = caret.on_hover_text("Refine, Review, Ask");
        caret.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), name));
        let other_scope = match self.prefs.agent.scope {
            ReviewMode::Quick => ReviewMode::Deep,
            ReviewMode::Deep => ReviewMode::Quick,
        };
        let runs_with = if self.prefs.agent.configured() {
            self.prefs.agent.label()
        } else {
            "No agent set up yet".to_string()
        };
        let menu = AiMenu {
            scope: other_scope,
            asking: self.asking(path),
            runs_with: &runs_with,
        };
        let picked = menu.show(&caret);
        self.apply_ai_pick(path, picked, other_scope);
        self.run_buttons(ui, path);
    }

    /// A run still working on an issue the split button left: a click cancels.
    fn stray_run_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let Some(face) = self.running_face(path) else {
            return;
        };
        let kind = ButtonKind::Review;
        if widgets::button(ui, kind, Some(face.icon), face.label, Some(face.key))
            .on_hover_text(face.hint)
            .clicked()
        {
            self.cancel_review_run(path);
        }
    }

    fn run_buttons(&mut self, ui: &mut egui::Ui, path: &Path) {
        self.ready_button(ui, path);
        self.asking_button(ui, path);
    }

    /// Runs the AI menu's pick. While a review runs, the rows that would
    /// start another do nothing; with no agent set up, they open Settings as
    /// the last row does.
    fn apply_ai_pick(&mut self, path: &Path, picked: Option<AiPick>, other_scope: ReviewMode) {
        let Some(picked) = picked else {
            return;
        };
        let running = self.review_is_running(path);
        let configured = self.prefs.agent.configured();
        let draft = self.session.editing().is_some();
        match picked {
            AiPick::Ask => self.toggle_ask(path),
            AiPick::Settings => self.open_config(),
            _ if running => {}
            _ if !configured => self.open_config(),
            AiPick::Refine => self.start_refinement(path),
            AiPick::Review => self.start_review(path, draft),
            AiPick::OtherScope => self.start_review_in(path, draft, other_scope),
        }
    }
}

/// What the AI menu lists.
struct AiMenu<'a> {
    /// The review scope the issue is not set to, as a one-off.
    scope: ReviewMode,
    asking: bool,
    runs_with: &'a str,
}

impl AiMenu<'_> {
    fn show(&self, caret: &egui::Response) -> Option<AiPick> {
        egui::Popup::menu(caret)
            .width(AI_MENU_WIDTH)
            .align(egui::RectAlign::BOTTOM_END)
            .show(|ui| {
                ui.set_min_width(AI_MENU_WIDTH);
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                self.rows(ui)
            })
            .and_then(|inner| inner.inner)
    }

    fn rows(&self, ui: &mut egui::Ui) -> Option<AiPick> {
        let mut picked = None;
        let mut row = |ui: &mut egui::Ui, pick: AiPick, action: MenuAction<'_>| {
            if widgets::menu_action(ui, action).clicked() {
                picked = Some(pick);
            }
        };
        let ai = |icon: Icon, name: &'static str, key: &'static str| MenuAction {
            icon: Some(icon),
            name,
            key,
            tone: ActionTone::Review,
            ..MenuAction::default()
        };
        let ask = ai(
            Icon::Bubble,
            if self.asking { "Stop asking" } else { "Ask…" },
            "⌘K",
        );
        let refine = ai(Icon::Narrow, "Refine with a challenger", "⌘R");
        row(ui, AiPick::Refine, refine);
        let review = ai(Icon::Review, "Review this issue", "⌘⇧R");
        row(ui, AiPick::Review, review);
        let (name, note) = match self.scope {
            ReviewMode::Deep => ("Deep review", "reads the project"),
            ReviewMode::Quick => ("Quick review", "ticket only"),
        };
        let other = MenuAction {
            note,
            ..ai(Icon::Review, name, "")
        };
        row(ui, AiPick::OtherScope, other);
        widgets::menu_separator(ui);
        row(ui, AiPick::Ask, ask);
        widgets::menu_separator(ui);
        row(
            ui,
            AiPick::Settings,
            MenuAction {
                name: self.runs_with,
                key: "⌘,",
                tone: ActionTone::Quiet,
                ..MenuAction::default()
            },
        );
        ui.add_space(theme::SPACE_1);
        picked
    }
}
