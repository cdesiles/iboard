//! The composer's side of NewIssueModal: your notes, the title row and the
//! foot's buttons.

use super::super::compose::{Composer, ComposerField, TitleStage};
use super::{
    Align, ButtonKind, CCursor, CCursorRange, Color32, ComposeAction, ComposeView, CornerRadius,
    DESCRIPTION_ID, FOOT_H, Frame, Key, Labels, Layout, Margin, Pos2, RichText, Sense, Shape,
    Stroke, TITLE_ID, Vec2, labels, sheet, theme, titles, widgets,
};
use crate::shell::theme::Icon;

/// `↩` adds from either field, `⌘↩` adds and opens the Editor, and
/// `⇧↩` is the notes' new line (or a step down from the title). Taken
/// before the fields see the key.
pub(super) fn composer_enter(
    ui: &mut egui::Ui,
    composer: &mut Composer,
    actions: &mut Vec<ComposeAction>,
) {
    let in_title = ui.memory(|m| m.has_focus(egui::Id::unique(TITLE_ID)));
    let in_notes = ui.memory(|m| m.has_focus(egui::Id::unique(DESCRIPTION_ID)));
    if !in_title && !in_notes {
        return;
    }
    let (mut add, mut edit, mut new_line) = (false, false, false);
    ui.input_mut(|i| {
        i.events.retain_mut(|event| match event {
            egui::Event::Key {
                key: Key::Enter,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command => {
                edit = true;
                false
            }
            egui::Event::Key {
                key: Key::Enter,
                pressed: true,
                modifiers,
                ..
            } if modifiers.shift => {
                new_line = true;
                in_notes
            }
            egui::Event::Key {
                key: Key::Enter,
                pressed: true,
                ..
            } => {
                add = true;
                false
            }
            _ => true,
        })
    });
    if add || edit {
        actions.push(ComposeAction::Submit { edit });
    }
    if in_title && new_line {
        composer.focus = Some(ComposerField::Description);
    }
}

/// "Your notes", their word count, and one text area that fills the pane,
/// read-only while Review drafts.
pub(super) fn notes_pane(ui: &mut egui::Ui, composer: &mut Composer, height: f32) {
    let p = theme::palette(ui);
    let drafting = composer.drafting.is_some();
    let (fill, ink) = if drafting {
        (p.surface_sunken, p.ink_muted)
    } else {
        (p.surface, p.ink)
    };
    let focused = !drafting && ui.memory(|m| m.has_focus(egui::Id::unique(DESCRIPTION_ID)));
    Frame::new()
        .fill(p.surface)
        .inner_margin(theme::SPACE_5)
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(
                ui.available_width(),
                height - 2.0 * theme::SPACE_5,
            ));
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            ui.horizontal(|ui| {
                let label = if drafting {
                    "YOUR NOTES · READ-ONLY WHILE REVIEW DRAFTS"
                } else {
                    "YOUR NOTES"
                };
                ui.label(
                    RichText::new(label)
                        .font(theme::ui_label())
                        .extra_letter_spacing(theme::LABEL_TRACKING)
                        .color(p.ink_muted),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let words = sheet::word_count(&composer.description);
                    let noun = if words == 1 { "word" } else { "words" };
                    ui.label(
                        RichText::new(format!("{words} {noun}"))
                            .font(theme::meta())
                            .color(p.ink_muted),
                    );
                });
            });
            let margin = Vec2::new(theme::SPACE_4, 10.0);
            let room = ui.available_height() - 2.0 * margin.y - 2.0;
            let area = Frame::new()
                .fill(fill)
                .stroke(Stroke::new(
                    1.0,
                    if focused { p.accent } else { p.line_strong },
                ))
                .corner_radius(CornerRadius::same(theme::RADIUS_SM))
                .inner_margin(margin)
                .show(ui, |ui| notes_field(ui, composer, ink, room));
            if focused {
                widgets::paint_focus_halo(ui, area.response.rect, theme::RADIUS_SM);
            }
        });
}

/// The notes' text area, scrolling inside itself.
fn notes_field(ui: &mut egui::Ui, composer: &mut Composer, ink: Color32, room: f32) {
    let p = theme::palette(ui);
    let drafting = composer.drafting.is_some();
    let scroll = theme::scroll_area(
        ui,
        theme::pane_scroll("new-issue-notes").max_height(room),
        |ui| {
            let rows = (room / 24.0).floor().max(3.0) as usize;
            ui.add_enabled(
                !drafting,
                egui::TextEdit::multiline(&mut composer.description)
                    .id(egui::Id::unique(DESCRIPTION_ID))
                    .font(theme::read_body())
                    .text_color(ink)
                    .hint_text(
                        RichText::new("Describe the issue. Write or dictate as much as you like.")
                            .font(theme::read_body())
                            .color(p.ink_faint),
                    )
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .desired_rows(rows)
                    .desired_width(f32::INFINITY)
                    .return_key(Some(egui::KeyboardShortcut::new(
                        egui::Modifiers::SHIFT,
                        Key::Enter,
                    ))),
            )
        },
    );
    if composer.focus == Some(ComposerField::Description) && !drafting {
        composer.focus = None;
        scroll.inner.request_focus();
    }
}

/// The title row on the sheet, in `read-h1`: the tags typed, the title, the
/// character count near the limit, and the Drafted tag, over a rule, dashed
/// when an empty title can only come from the first line.
pub(super) fn title_row(ui: &mut egui::Ui, composer: &mut Composer, known: &Labels) {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::hover());
    let rule_y = row.max.y - 0.5;
    let rule = [Pos2::new(row.min.x, rule_y), Pos2::new(row.max.x, rule_y)];
    let drafted = composer.stage == TitleStage::Drafted;
    if composer.engine.is_some() || !composer.title.trim().is_empty() {
        ui.painter().line_segment(rule, Stroke::new(1.0, p.line));
    } else {
        ui.painter().extend(Shape::dashed_line(
            &rule,
            Stroke::new(1.0, p.line),
            4.0,
            3.0,
        ));
    }
    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row.with_max_y(row.max.y - 4.0))
            .layout(Layout::right_to_left(Align::Center)),
    );
    row_ui.spacing_mut().item_spacing.x = theme::SPACE_3;
    if drafted {
        widgets::review_tag(&mut row_ui, "Drafted");
    }
    let chars = composer.title.chars().count();
    if chars >= titles::MAX_TITLE_CHARS - 10 {
        row_ui.label(
            RichText::new(format!("{chars}/{}", titles::MAX_TITLE_CHARS))
                .font(theme::meta())
                .color(p.ink_muted),
        );
    }
    row_ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        for label in &composer.title_labels {
            let name = labels::normalize(label).unwrap_or_else(|| label.clone());
            widgets::label_pill(ui, &name, known.style(&name));
        }
        title_field(ui, composer);
    });
}

/// `⌫` at the very start of the title turns the last tag back into text.
fn untag_on_backspace(row: &egui::Ui, composer: &mut Composer, title_id: egui::Id) {
    if composer.title_labels.is_empty() || !row.memory(|m| m.has_focus(title_id)) {
        return;
    }
    let ctx = row.ctx();
    let range = egui::text_edit::TextEditState::load(ctx, title_id)
        .and_then(|state| state.cursor.char_range());
    let at_start = range.map_or_else(
        || composer.title.is_empty(),
        |range| range.is_empty() && range.primary.index.0 == 0,
    );
    if at_start
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Backspace))
        && let Some(cursor) = composer.untag_label()
    {
        set_title_cursor(ctx, title_id, CCursorRange::one(CCursor::new(cursor)));
    }
}

fn set_title_cursor(ctx: &egui::Context, title_id: egui::Id, range: CCursorRange) {
    let mut state = egui::text_edit::TextEditState::load(ctx, title_id).unwrap_or_default();
    state.cursor.set_char_range(Some(range));
    state.store(ctx, title_id);
}

/// The title's text field; focused on request, with the whole suggestion
/// selected so typing replaces it. `[bug]` typed at its start becomes a tag.
fn title_field(row: &mut egui::Ui, composer: &mut Composer) {
    let p = theme::palette(row);
    let title_id = egui::Id::unique(TITLE_ID);
    untag_on_backspace(row, composer, title_id);
    let hint = match composer.engine {
        None => "Add a title",
        Some(_) => "Title is named from your notes",
    };
    let width = row.available_width();
    let output = egui::TextEdit::singleline(&mut composer.title)
        .id(title_id)
        .font(theme::read_h1())
        .hint_text(
            RichText::new(hint)
                .font(theme::read_h1())
                .color(p.ink_faint),
        )
        .char_limit(titles::MAX_TITLE_CHARS)
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .desired_width(width)
        .show(row);
    if composer.focus == Some(ComposerField::Title) {
        composer.focus = None;
        output.response.request_focus();
        if std::mem::take(&mut composer.select_title) {
            let end = CCursor::new(composer.title.chars().count());
            set_title_cursor(row.ctx(), title_id, CCursorRange::two(CCursor::new(0), end));
        }
    }
    // Editing a draft's title makes it the user's own.
    if output.response.changed() {
        composer.stage = TitleStage::Idle;
        let cursor = output.cursor_range.map(|range| range.primary.index.0);
        if let Some(cursor) = cursor.and_then(|at| composer.tag_typed_label(at)) {
            set_title_cursor(row.ctx(), title_id, CCursorRange::one(CCursor::new(cursor)));
            row.ctx().request_repaint();
        }
    }
}

/// Key hints on the left while ⌘ is held; Discard, Close, Draft with Review
/// and Add to Todo on the right, with Keep my notes before them once there is
/// a draft. Add to Todo shows `⌘S`; `↩` stays among the hints on the left.
pub(super) fn foot(
    ui: &mut egui::Ui,
    composer: &Composer,
    view: &mut ComposeView,
    actions: &mut Vec<ComposeAction>,
) {
    let drafting = composer.drafting.is_some();
    Frame::new()
        .inner_margin(Vec2::new(theme::SPACE_5, 0.0))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), FOOT_H),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    if widgets::button(ui, ButtonKind::Primary, None, "Add to Todo", Some("⌘S"))
                        .clicked()
                    {
                        actions.push(ComposeAction::Submit { edit: false });
                    }
                    let label = if drafting {
                        "Drafting…"
                    } else if composer.draft.is_some() {
                        "Redraft"
                    } else {
                        "Draft with Review"
                    };
                    let blocked = view.blocked.filter(|_| !drafting);
                    let kind = ButtonKind::ReviewHairline.when(!drafting && blocked.is_none());
                    let draft = widgets::button(ui, kind, Some(Icon::Review), label, Some("⌘⇧R"));
                    let draft = match blocked {
                        Some(why) => draft.on_hover_text(why),
                        None => draft,
                    };
                    if draft.clicked() && kind == ButtonKind::ReviewHairline {
                        actions.push(ComposeAction::Draft);
                    }
                    if widgets::button(ui, ButtonKind::Secondary, None, "Close", Some("esc"))
                        .clicked()
                    {
                        actions.push(ComposeAction::Close);
                    }
                    if !composer.is_empty()
                        && widgets::button(ui, ButtonKind::Ghost, None, "Discard", None).clicked()
                    {
                        actions.push(ComposeAction::Discard);
                    }
                    if composer.draft.is_some() && !drafting {
                        widgets::checkbox(ui, view.keep_notes, "Keep my notes under ## Notes");
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                        if composer.engine.is_none()
                            && labels::display_title(&composer.written_title()).is_empty()
                        {
                            let p = theme::palette(ui);
                            ui.label(
                                RichText::new("empty title uses the first line")
                                    .font(theme::meta())
                                    .color(p.ink_faint),
                            );
                        }
                        let hints: &[(&str, &str)] = if drafting {
                            &[("esc", "stop, keep what arrived")]
                        } else if composer.description.trim().is_empty() {
                            &[("↩", "add"), ("esc", "close")]
                        } else {
                            &[("↩", "add"), ("⇧↩", "new line"), ("⌘↩", "add & edit")]
                        };
                        widgets::sliding_hints(ui, |ui| {
                            for (key, action) in hints {
                                ui.horizontal(|ui| widgets::hint(ui, key, action));
                            }
                        });
                    });
                },
            );
        });
}
