//! Comments in the reader's margin: a note on one block of the issue, kept in
//! its sidecar and sent to the next review.

use super::*;
use crate::reviews::Comment;

/// The comments of the issue in the reader.
pub(super) struct Notes {
    pub(super) path: PathBuf,
    pub(super) comments: Vec<Comment>,
    /// The note being written, and the block it goes on.
    pub(super) draft: Option<NoteDraft>,
}

pub(super) struct NoteDraft {
    pub(super) anchor: String,
    pub(super) text: String,
    /// Focus the field on the frame after it opens.
    pub(super) focus: bool,
}

/// What a click in the margin or on a note asks for.
pub(super) enum NoteAction {
    Open(String),
    Save,
    Cancel,
    Resolve(u32),
}

impl IboardApp {
    /// The comments of `path`, read from its sidecar when another issue was
    /// open before.
    pub(super) fn notes_for(&mut self, path: &Path) -> &mut Notes {
        let notes = self
            .session
            .notes
            .take()
            .filter(|notes| notes.path == path)
            .unwrap_or_else(|| Notes {
                path: path.to_path_buf(),
                comments: reviews::load(self.board.root_of(path), path).comments,
                draft: None,
            });
        self.session.notes.insert(notes)
    }

    /// Writes the comments of the open issue back into its sidecar.
    fn save_notes(&mut self) {
        let Some(notes) = self.session.notes.as_ref() else {
            return;
        };
        let root = self.board.root_of(&notes.path);
        let mut sidecar = reviews::load(root, &notes.path);
        sidecar.comments = notes.comments.clone();
        if let Err(err) = reviews::save(root, &notes.path, &sidecar) {
            self.error = Some(err);
        }
    }

    pub(super) fn apply_note_action(&mut self, path: &Path, action: NoteAction) {
        let notes = self.notes_for(path);
        match action {
            NoteAction::Open(anchor) => {
                notes.draft = Some(NoteDraft {
                    anchor,
                    text: String::new(),
                    focus: true,
                });
                return;
            }
            NoteAction::Cancel => {
                notes.draft = None;
                return;
            }
            NoteAction::Save => {
                let Some(draft) = notes.draft.take() else {
                    return;
                };
                if draft.text.trim().is_empty() {
                    return;
                }
                let id = notes.comments.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                notes.comments.push(Comment {
                    id,
                    anchor: draft.anchor,
                    note: draft.text.trim().to_string(),
                    at: chrono::Local::now().to_rfc3339(),
                    sent: false,
                });
            }
            NoteAction::Resolve(id) => notes.comments.retain(|c| c.id != id),
        }
        self.save_notes();
    }

    /// The comments a review of `path` carries, as (passage, note) pairs.
    pub(super) fn reviewer_notes(&mut self, path: &Path) -> Vec<(String, String)> {
        self.notes_for(path)
            .comments
            .iter()
            .map(|c| (c.anchor.clone(), c.note.clone()))
            .collect()
    }

    /// Once a review carries them, the comments of `path` say so.
    pub(super) fn mark_notes_sent(&mut self, path: &Path) {
        let notes = self.notes_for(path);
        if notes.comments.iter().all(|c| c.sent) {
            return;
        }
        for comment in &mut notes.comments {
            comment.sent = true;
        }
        self.save_notes();
    }

    /// Whether a note is being written, which then owns esc and double-clicks.
    pub(super) fn writing_note(&self) -> bool {
        self.session
            .notes
            .as_ref()
            .is_some_and(|notes| notes.draft.is_some())
    }
}

/// Everything that hangs on one block: its margin button, its comments, and
/// the note being written on it.
pub(super) fn block_notes(
    ui: &mut egui::Ui,
    notes: &mut Notes,
    anchor: &str,
    block: Rect,
    index: usize,
) -> Vec<NoteAction> {
    let mut actions = Vec::new();
    let on_block: Vec<&Comment> = notes
        .comments
        .iter()
        .filter(|c| c.anchor == anchor)
        .collect();
    if margin_button(ui, block, index, on_block.len()) {
        actions.push(NoteAction::Open(anchor.to_string()));
    }
    actions.extend(
        on_block
            .into_iter()
            .filter_map(|comment| note_card(ui, comment)),
    );
    if let Some(draft) = notes.draft.as_mut().filter(|draft| draft.anchor == anchor) {
        actions.extend(note_composer(ui, draft));
    }
    actions
}

/// The comments whose block was since edited away, at the end of the page.
pub(super) fn outdated_notes(
    ui: &mut egui::Ui,
    notes: &Notes,
    text: &str,
    blocks: &[std::ops::Range<usize>],
) -> Vec<NoteAction> {
    let outdated: Vec<&Comment> = notes
        .comments
        .iter()
        .filter(|c| {
            !blocks
                .iter()
                .any(|range| text[range.clone()].trim() == c.anchor)
        })
        .collect();
    if outdated.is_empty() {
        return Vec::new();
    }
    let p = theme::palette(ui);
    ui.add_space(theme::SPACE_4);
    ui.label(
        RichText::new("OUTDATED COMMENTS")
            .font(theme::ui_label())
            .extra_letter_spacing(theme::LABEL_TRACKING)
            .color(p.ink_muted),
    );
    outdated
        .into_iter()
        .filter_map(|comment| {
            ui.label(
                RichText::new(excerpt(&comment.anchor))
                    .font(theme::meta())
                    .color(p.ink_faint),
            );
            note_card(ui, comment)
        })
        .collect()
}

/// The first line of a passage, cut to fit a meta line.
fn excerpt(anchor: &str) -> String {
    let line = anchor.lines().next().unwrap_or_default();
    match line.char_indices().nth(80) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_string(),
    }
}

/// The comment button in the right margin of a block: shown while the block
/// is hovered, and always, in `review`, once it has comments.
fn margin_button(ui: &mut egui::Ui, block: Rect, index: usize, count: usize) -> bool {
    let p = theme::palette(ui);
    let gutter = theme::SPACE_7;
    let row = Rect::from_x_y_ranges(block.left()..=block.right() + gutter, block.y_range());
    let size = theme::ICON_SIZE + theme::SPACE_3;
    let button = Rect::from_min_size(
        Pos2::new(block.right() + (gutter - size) / 2.0, block.top()),
        Vec2::splat(size),
    );
    let response = ui
        .interact(button, ui.id().with(("comment", index)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Comment on this for the review");
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Add a comment"));
    let color = if count > 0 {
        p.review
    } else if response.hovered() {
        p.ink
    } else {
        p.ink_faint
    };
    if count > 0 || ui.rect_contains_pointer(row) {
        if response.hovered() {
            ui.painter()
                .rect_filled(button, CornerRadius::same(theme::RADIUS_SM), p.review_soft);
        }
        let icon = Rect::from_center_size(button.center(), Vec2::splat(theme::ICON_SIZE));
        theme::paint_icon(ui.painter(), Icon::Review, icon, color);
        if count > 1 {
            ui.painter().text(
                Pos2::new(button.center().x, button.bottom() + theme::SPACE_1),
                egui::Align2::CENTER_TOP,
                count.to_string(),
                theme::meta(),
                p.review,
            );
        }
    }
    response.clicked()
}

/// A saved comment under its block: the note, whether a review saw it, and
/// Resolve.
fn note_card(ui: &mut egui::Ui, comment: &Comment) -> Option<NoteAction> {
    let p = theme::palette(ui);
    let mut action = None;
    Frame::new()
        .fill(p.review_soft)
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Margin::symmetric(
            theme::SPACE_4 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(&comment.note)
                    .font(theme::ui_body())
                    .color(p.ink),
            );
            ui.horizontal(|ui| {
                let state = if comment.sent {
                    "seen by a review"
                } else {
                    "for the next review"
                };
                ui.label(RichText::new(state).font(theme::meta()).color(p.ink_muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, ButtonKind::Ghost, None, "Resolve", None).clicked() {
                        action = Some(NoteAction::Resolve(comment.id));
                    }
                });
            });
        });
    action
}

/// The note being written: its field, Comment (`⌘↩`) and Cancel (`esc`).
fn note_composer(ui: &mut egui::Ui, draft: &mut NoteDraft) -> Option<NoteAction> {
    let p = theme::palette(ui);
    let mut action = None;
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.review))
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Margin::same(theme::SPACE_3 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let field = widgets::text_entry(
                ui,
                egui::TextEdit::multiline(&mut draft.text)
                    .hint_text(RichText::new("What should the review know?").color(p.ink_faint))
                    .font(theme::ui_body())
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut draft.focus) {
                field.request_focus();
            }
            let submit = field.has_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter));
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), theme::CONTROL_H),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    let kind = if draft.text.trim().is_empty() {
                        ButtonKind::Disabled
                    } else {
                        ButtonKind::Review
                    };
                    if widgets::button(ui, kind, None, "Comment", Some("⌘↩")).clicked() || submit
                    {
                        action = Some(NoteAction::Save);
                    }
                    if widgets::button(ui, ButtonKind::Ghost, None, "Cancel", Some("esc")).clicked()
                    {
                        action = Some(NoteAction::Cancel);
                    }
                },
            );
        });
    action
}
