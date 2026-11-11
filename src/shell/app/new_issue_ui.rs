//! NewIssueModal's look: a Modal XL with your notes on the left and the issue
//! file, exactly as `↩` will write it, on the right. See
//! `design/components/NewIssueModal`. The composer's side lives in `composer`,
//! the file's preview in `preview`.

use super::board::status_color;
use super::compose::Composer;
use super::reader::image_base_uri;
use super::{
    Align, ButtonKind, CCursor, CCursorRange, Color32, CommonMarkCache, CommonMarkViewer,
    CornerRadius, Engine, Frame, IboardApp, Key, Labels, Layout, Margin, Path, Pos2, Rect,
    RichText, Sense, Shape, Stroke, Vec2, files, labels, settings, sheet, theme, titles, widgets,
};
use crate::general;

mod composer;
mod preview;

use composer::{composer_enter, foot, notes_pane};
use preview::issue_pane;

pub(super) const TITLE_ID: &str = "composer";
pub(super) const DESCRIPTION_ID: &str = "composer-description";

/// The notes pane's narrowest.
const NOTES_MIN: f32 = 320.0;
const FOOT_H: f32 = 40.0;

/// What a click or a key in the modal asked for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ComposeAction {
    Submit {
        edit: bool,
    },
    /// Add it and open the Editor on this standard section, by its place.
    SubmitForSection(usize),
    Close,
    Discard,
    Draft,
    UndoDraft,
    TitleSettings,
    Follow,
}

/// What the modal shows besides the composer itself.
struct ComposeView<'a> {
    /// The column the issue lands in, for the sheet's head.
    column: &'a str,
    status: Color32,
    /// The folder the issue lands in, and the file it will be.
    folder: &'a str,
    dir: Option<&'a Path>,
    /// The `file://` base the pasted images' links resolve against.
    images: String,
    sections: &'a [String],
    labels: &'a Labels,
    keep_notes: &'a mut bool,
    /// Why Draft with Review is off, if it is.
    blocked: Option<&'static str>,
}

impl IboardApp {
    /// NewIssueModal over the board, while it is open.
    pub(super) fn new_issue_modal(&mut self, ui: &egui::Ui) {
        if !self.composer_open() {
            return;
        }
        let p = theme::palette(ui);
        let Some((todo, column, status)) = self
            .board
            .todo_column()
            .map(|c| (c.dir.clone(), c.name.clone(), status_color(p, c)))
        else {
            return;
        };
        let images = image_base_uri(&todo.join("new.md"));
        let dir = self.compose_preview(&todo).unwrap_or(todo);
        let folder = self.compose_folder(&dir);
        let blocked = self.draft_blocked();
        let keep_notes = self.prefs.keep_notes;
        let size = theme::modal_size(theme::ModalSize::Xl, ui.ctx().content_rect());
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        let mut view = ComposeView {
            column: &column,
            status,
            folder: &folder,
            dir: Some(dir.as_path()).filter(|dir| dir.is_dir()),
            images,
            sections: &self.board.reader_sections,
            labels: &self.prefs.labels,
            keep_notes: &mut self.prefs.keep_notes,
            blocked,
        };
        let cache = &mut self.reader.cache;
        let actions = widgets::modal(ui, "new-issue", theme::ModalSize::Xl, "New issue")
            .show(ui.ctx(), |ui| {
                modal_ui(ui, size, composer, &mut view, cache)
            })
            .inner;
        if self.prefs.keep_notes != keep_notes {
            self.remember_section(settings::Section::new(
                "new_issue",
                &general::keep_notes_section(self.prefs.keep_notes),
            ));
        }
        for action in actions {
            self.compose_action(action);
        }
    }

    pub(super) fn compose_action(&mut self, action: ComposeAction) {
        match action {
            ComposeAction::Submit { edit } => self.submit_composer(edit),
            ComposeAction::SubmitForSection(at) => self.submit_for_section(at),
            ComposeAction::Close => self.close_composer(),
            ComposeAction::Discard => self.discard_composer(),
            ComposeAction::Draft => self.start_draft(),
            ComposeAction::UndoDraft => self.undo_draft(),
            ComposeAction::TitleSettings => self.open_title_settings(),
            ComposeAction::Follow => {
                if let Some(composer) = self.session.composer.as_mut() {
                    composer.follow = true;
                }
            }
        }
    }

    /// The modal's keys outside its fields: `esc` (stop, or close and park),
    /// `⌘⇧R` (draft), `⌘S` (add as it is), `⌘Z` (undo the draft), `End`
    /// (follow the draft), and `↩` / `⌘↩` from a button.
    pub(super) fn new_issue_keys(&mut self, ctx: &egui::Context) {
        let in_notes = ctx.memory(|m| m.has_focus(egui::Id::unique(DESCRIPTION_ID)));
        let typing = ctx.memory(|m| m.focused().is_some());
        let take = |modifiers, key| ctx.input_mut(|i| i.consume_key(modifiers, key));
        if take(egui::Modifiers::NONE, Key::Escape) {
            self.composer_escape();
        } else if take(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::R) {
            self.start_draft();
        } else if take(egui::Modifiers::COMMAND, Key::S) {
            self.submit_composer(false);
        } else if !in_notes && take(egui::Modifiers::COMMAND, Key::Z) {
            self.undo_draft();
        } else if !in_notes && take(egui::Modifiers::NONE, Key::End) {
            self.compose_action(ComposeAction::Follow);
        } else if !typing && take(egui::Modifiers::COMMAND, Key::Enter) {
            self.submit_composer(true);
        } else if !typing && take(egui::Modifiers::NONE, Key::Enter) {
            self.submit_composer(false);
        }
    }
}

fn modal_ui(
    ui: &mut egui::Ui,
    size: Vec2,
    composer: &mut Composer,
    view: &mut ComposeView,
    cache: &mut CommonMarkCache,
) -> Vec<ComposeAction> {
    let mut actions = Vec::new();
    composer_enter(ui, composer, &mut actions);
    ui.set_width(size.x);
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    ui.style_mut().interaction.selectable_labels = false;
    head(ui, composer, view, &mut actions);
    widgets::rule(ui);
    let body = Vec2::new(size.x, size.y - theme::BAR_H - FOOT_H - 2.0);
    if size.x < theme::MODAL_XL_STACK_BELOW {
        let notes_h = (body.y * 0.4).round();
        ui.allocate_ui_with_layout(
            Vec2::new(body.x, notes_h),
            Layout::top_down(Align::Min),
            |ui| notes_pane(ui, composer, notes_h),
        );
        widgets::rule(ui);
        issue_pane(
            ui,
            Vec2::new(body.x, body.y - notes_h - 1.0),
            composer,
            view,
            cache,
            &mut actions,
        );
    } else {
        let split = theme::Split {
            id: theme::pane_split_id("new-issue"),
            size: body,
            default: (body.x * 0.6).round(),
            range: 360.0..=(body.x - NOTES_MIN),
            inline: false,
        };
        theme::split_panes(
            ui,
            split,
            (&mut *composer, &mut *view, &mut *cache, &mut actions),
            |ui, (composer, ..)| notes_pane(ui, composer, body.y),
            |ui, (composer, view, cache, actions)| {
                let size = Vec2::new(ui.available_width(), body.y);
                issue_pane(ui, size, composer, view, cache, actions);
            },
        );
    }
    widgets::rule(ui);
    foot(ui, composer, view, &mut actions);
    actions
}

/// "New issue", `in todo/`, the title engine's tag, and ×.
fn head(
    ui: &mut egui::Ui,
    composer: &Composer,
    view: &ComposeView,
    actions: &mut Vec<ComposeAction>,
) {
    let p = theme::palette(ui);
    let close = widgets::modal_header(ui, "New issue", |ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        ui.label(
            RichText::new(format!("in {}", view.folder))
                .font(theme::meta())
                .color(p.ink_muted),
        );
        let tag = match composer.engine {
            Some(engine) => engine.tag(),
            None => Engine::Off.tag(),
        };
        if widgets::tag(ui, tag)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Title inference (⌘, › Titles)")
            .clicked()
        {
            actions.push(ComposeAction::TitleSettings);
        }
    });
    if close {
        actions.push(ComposeAction::Close);
    }
}
