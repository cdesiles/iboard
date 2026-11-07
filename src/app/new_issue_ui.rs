//! NewIssueModal's look: a Modal XL with your notes on the left and the issue
//! file, exactly as `↩` will write it, on the right. See
//! `design/components/NewIssueModal`.

use super::board::status_color;
use super::compose::{Composer, ComposerField, Drafted, TitleStage};
use super::reader::{column_mark, image_base_uri, sheet_rule};
use super::{
    Align, ButtonKind, CCursor, CCursorRange, Color32, CommonMarkCache, CommonMarkViewer,
    CornerRadius, Engine, Frame, IboardApp, Key, Labels, Layout, Margin, Path, Pos2, Rect,
    RichText, Sense, Shape, Stroke, Vec2, files, labels, sheet, theme, titles, widgets,
};
use crate::theme::Icon;
use iboard::{general, new_issue};

pub(super) const TITLE_ID: &str = "composer";
pub(super) const DESCRIPTION_ID: &str = "composer-description";

/// From here the drafting line says it is slow.
const STILL_WORKING_AFTER: u64 = 20;
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
            self.remember_section(general::keep_notes_section(self.prefs.keep_notes));
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
    /// `⌘⇧R` (draft), `⌘Z` (undo the draft), `End` (follow the draft), and
    /// `↩` / `⌘↩` from a button.
    pub(super) fn new_issue_keys(&mut self, ctx: &egui::Context) {
        let in_notes = ctx.memory(|m| m.has_focus(egui::Id::new(DESCRIPTION_ID)));
        let typing = ctx.memory(|m| m.focused().is_some());
        let take = |modifiers, key| ctx.input_mut(|i| i.consume_key(modifiers, key));
        if take(egui::Modifiers::NONE, Key::Escape) {
            self.composer_escape();
        } else if take(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::R) {
            self.start_draft();
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
        ui.horizontal_top(|ui| {
            let panes = Rect::from_min_size(ui.cursor().min, body);
            let default = (body.x * 0.6).round();
            let range = 360.0..=(body.x - NOTES_MIN);
            let split = theme::pane_split_id("new-issue");
            let issue_w = theme::pane_split(ui.ctx(), split, default)
                .clamp(*range.start(), range.end().max(*range.start()));
            let notes_w = body.x - issue_w;
            ui.allocate_ui_with_layout(
                Vec2::new(notes_w, body.y),
                Layout::top_down(Align::Min),
                |ui| notes_pane(ui, composer, body.y),
            );
            let x = ui.cursor().min.x;
            let gutter =
                Rect::from_x_y_ranges(x - theme::SPACE_3..=x + theme::SPACE_3, panes.y_range());
            let divider = theme::pane_divider(ui, split, panes, gutter, range, default, false);
            ui.allocate_ui_with_layout(
                Vec2::new(issue_w, body.y),
                Layout::top_down(Align::Min),
                |ui| {
                    issue_pane(
                        ui,
                        Vec2::new(issue_w, body.y),
                        composer,
                        view,
                        cache,
                        &mut actions,
                    )
                },
            );
            divider.paint(ui);
        });
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

/// `↩` adds from either field, `⌘↩` adds and opens the Editor, and
/// `⇧↩` is the notes' new line (or a step down from the title). Taken
/// before the fields see the key.
fn composer_enter(ui: &mut egui::Ui, composer: &mut Composer, actions: &mut Vec<ComposeAction>) {
    let in_title = ui.memory(|m| m.has_focus(egui::Id::new(TITLE_ID)));
    let in_notes = ui.memory(|m| m.has_focus(egui::Id::new(DESCRIPTION_ID)));
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
fn notes_pane(ui: &mut egui::Ui, composer: &mut Composer, height: f32) {
    let p = theme::palette(ui);
    let drafting = composer.drafting.is_some();
    let (fill, ink) = if drafting {
        (p.surface_sunken, p.ink_muted)
    } else {
        (p.surface, p.ink)
    };
    let focused = !drafting && ui.memory(|m| m.has_focus(egui::Id::new(DESCRIPTION_ID)));
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
        egui::ScrollArea::vertical()
            .id_salt("new-issue-notes")
            .max_height(room)
            .auto_shrink([false, false]),
        |ui| {
            let rows = (room / 24.0).floor().max(3.0) as usize;
            ui.add_enabled(
                !drafting,
                egui::TextEdit::multiline(&mut composer.description)
                    .id(egui::Id::new(DESCRIPTION_ID))
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

/// The `bg` well with the Reader sheet: the file as it will be written.
fn issue_pane(
    ui: &mut egui::Ui,
    size: Vec2,
    composer: &mut Composer,
    view: &mut ComposeView,
    cache: &mut CommonMarkCache,
    actions: &mut Vec<ComposeAction>,
) {
    let p = theme::palette(ui);
    let (well, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(well, CornerRadius::ZERO, p.bg);
    let mut pane = ui.new_child(egui::UiBuilder::new().max_rect(well));
    let drafting = composer.drafting.is_some();
    if drafting
        && pane.rect_contains_pointer(well)
        && pane.input(|i| i.smooth_scroll_delta.y != 0.0)
    {
        composer.follow = false;
    }
    let gutter = theme::SPACE_5;
    let page_w = (size.x - 2.0 * gutter).min(theme::READER_MAX);
    let mut writing_at: Option<Rect> = None;
    let scroll = theme::scroll_area(
        &mut pane,
        egui::ScrollArea::vertical()
            .id_salt("new-issue-sheet")
            .content_margin(theme::scroll_margin(gutter, gutter))
            .auto_shrink([false, false]),
        |ui| {
            ui.vertical_centered(|ui| {
                ui.set_max_width(page_w);
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.line_soft))
                    .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                    .inner_margin(Vec2::new(theme::SPACE_7, theme::SPACE_6))
                    .show(ui, |ui| {
                        ui.set_width(page_w - 2.0 * theme::SPACE_7);
                        ui.set_min_height(size.y - 2.0 * gutter - 2.0 * theme::SPACE_6 - 2.0);
                        ui.with_layout(Layout::top_down(Align::Min), |ui| {
                            writing_at = sheet(ui, composer, view, cache, actions);
                        });
                    });
                unwritten(ui, composer, view, actions);
            });
            if let Some(rect) = writing_at.filter(|_| drafting && composer.follow) {
                let below = Rect::from_min_size(
                    Pos2::new(rect.min.x, rect.max.y),
                    Vec2::new(1.0, size.y / 3.0),
                );
                ui.scroll_to_rect(below, Some(Align::Max));
            }
        },
    );
    let _ = scroll;
    if drafting && !composer.follow {
        let at = egui::Pos2::new(well.center().x, well.max.y - theme::SPACE_5);
        let mut pill = pane.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_center_size(
                    at,
                    Vec2::new(200.0, theme::CONTROL_H),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        if widgets::button(
            &mut pill,
            ButtonKind::Secondary,
            None,
            "Follow the draft ↓",
            Some("End"),
        )
        .clicked()
        {
            actions.push(ComposeAction::Follow);
        }
    }
}

/// The sheet: its head, the draft's receipt, labels, the title row and the
/// body. Returns where the line being written ends.
fn sheet(
    ui: &mut egui::Ui,
    composer: &mut Composer,
    view: &mut ComposeView,
    cache: &mut CommonMarkCache,
    actions: &mut Vec<ComposeAction>,
) -> Option<Rect> {
    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
    sheet_head(ui, composer, view);
    receipt(ui, composer, actions);
    if let Some(drafted) = &composer.draft
        && !drafted.draft.labels.is_empty()
    {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_2;
            for label in &drafted.draft.labels {
                widgets::label_pill(ui, label, view.labels.style(label));
            }
        });
    }
    let started = composer.drafting.as_ref().map(|run| run.job.elapsed());
    let skeleton = started.is_some_and(|elapsed| elapsed >= theme::SKELETON_AFTER);
    let title_pending = composer
        .draft
        .as_ref()
        .is_some_and(|d| d.draft.title.is_none())
        && started.is_some();
    if title_pending {
        if skeleton {
            widgets::skeleton_bar(ui, 0.6, 12.0);
        }
    } else {
        title_row(ui, composer, view.labels);
    }
    match &composer.draft {
        None => {
            mirror(ui, &composer.description, &view.images, cache);
            None
        }
        Some(drafted) => {
            let at = drafted_body(
                ui,
                drafted,
                view.sections,
                started.is_some(),
                skeleton,
                &view.images,
                cache,
            );
            if *view.keep_notes && started.is_none() && !composer.description.trim().is_empty() {
                section_heading(ui, "Notes", false);
                body_text(ui, "kept-notes", &composer.description, &view.images, cache);
            }
            at
        }
    }
}

/// `● TODO · todo/<slug>.md`, or `todo/` and how the title will come, over a
/// rule; while Review drafts, the Thinking mark and its clock on the right.
fn sheet_head(ui: &mut egui::Ui, composer: &Composer, view: &ComposeView) {
    let p = theme::palette(ui);
    let meta = |text: String, color: Color32| RichText::new(text).font(theme::meta()).color(color);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        column_mark(ui, view.column, view.status);
        ui.label(meta("·".into(), p.ink_faint));
        ui.spacing_mut().item_spacing.x = 0.0;
        let title = composer.written_title();
        let display = labels::display_title(&title);
        if let Some(error) = &composer.error {
            ui.add(egui::Label::new(meta(error.clone(), p.danger)).truncate());
        } else if display.is_empty() {
            ui.label(meta(view.folder.to_string(), p.ink_muted));
            if composer.engine.is_some() {
                ui.label(meta(" named in the background".into(), p.ink_faint));
            }
        } else {
            let (name, taken) = match view.dir {
                Some(dir) => crate::issue::new_file_name(dir, &title),
                None => (format!("{}.md", files::slugify(&display)), false),
            };
            let color = if taken {
                p.status_doing
            } else if composer.stage == TitleStage::Drafted {
                p.status_done
            } else {
                p.ink_muted
            };
            ui.add(egui::Label::new(meta(format!("{}{name}", view.folder), color)).truncate());
        }
        if let Some(run) = &composer.drafting {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                let elapsed = run.job.elapsed();
                let slow = elapsed.as_secs() >= STILL_WORKING_AFTER;
                let status = if slow {
                    "Still working"
                } else {
                    "Review is drafting"
                };
                widgets::announce_thinking(ui, status, elapsed);
                let clock = crate::format::clock(elapsed);
                let text = if slow {
                    format!("{status} · {clock} · esc stops")
                } else if elapsed >= theme::CLOCK_AFTER {
                    format!("{status} · {clock}")
                } else {
                    status.to_string()
                };
                ui.label(meta(text, p.review));
                widgets::thinking(ui, 14.0);
            });
        }
    });
    ui.add_space(theme::SPACE_2);
    sheet_rule(ui);
    ui.add_space(theme::SPACE_3);
}

/// Under the sheet head: who drafted it, that the notes changed since, or
/// that Review couldn't draft it.
fn receipt(ui: &mut egui::Ui, composer: &Composer, actions: &mut Vec<ComposeAction>) {
    let p = theme::palette(ui);
    let small =
        |text: &str, color: Color32| RichText::new(text).font(theme::ui_small()).color(color);
    let line = |ui: &mut egui::Ui, fill: Color32, content: &mut dyn FnMut(&mut egui::Ui)| {
        Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(theme::RADIUS_SM))
            .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                    content(ui);
                });
            });
    };
    if let Some(error) = composer
        .draft_error
        .as_ref()
        .filter(|_| composer.draft.is_none())
    {
        line(ui, p.danger_soft, &mut |ui| {
            ui.label(small("Review couldn't draft it", p.danger).strong());
            ui.label(small(error, p.ink));
            if widgets::button(ui, ButtonKind::Secondary, None, "Retry", None).clicked() {
                actions.push(ComposeAction::Draft);
            }
        });
        return;
    }
    let Some(drafted) = composer
        .draft
        .as_ref()
        .filter(|_| composer.drafting.is_none())
    else {
        return;
    };
    if drafted.notes_hash != new_issue::notes_hash(&composer.description) {
        line(ui, p.surface_sunken, &mut |ui| {
            ui.label(small("Your notes changed since this draft", p.status_doing));
            if widgets::button(
                ui,
                ButtonKind::ReviewHairline,
                Some(Icon::Review),
                "Redraft",
                None,
            )
            .clicked()
            {
                actions.push(ComposeAction::Draft);
            }
        });
        return;
    }
    line(ui, p.review_soft, &mut |ui| {
        let what = if drafted.stopped {
            "Stopped · what arrived stays"
        } else {
            "Drafted by Review"
        };
        ui.label(small(what, p.review).strong());
        ui.label(small("from your notes", p.ink_muted));
        if let Some(receipt) = &drafted.receipt {
            ui.label(
                RichText::new(receipt)
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        }
        if widgets::button(ui, ButtonKind::Ghost, None, "Undo draft", Some("⌘Z")).clicked() {
            actions.push(ComposeAction::UndoDraft);
        }
    });
}

/// Before a draft, the sheet's body is your notes rendered, live.
fn mirror(ui: &mut egui::Ui, notes: &str, images: &str, cache: &mut CommonMarkCache) {
    if notes.trim().is_empty() {
        return;
    }
    body_text(ui, "mirror", notes, images, cache);
}

/// The draft at its final shape: the summary, then each standard section,
/// written, being written (with the `review` rule and caret) or waiting
/// under skeleton bars. Returns where the writing line ends.
fn drafted_body(
    ui: &mut egui::Ui,
    drafted: &Drafted,
    wanted: &[String],
    running: bool,
    skeleton: bool,
    images: &str,
    cache: &mut CommonMarkCache,
) -> Option<Rect> {
    let draft = &drafted.draft;
    let mut writing_at = None;
    if !draft.summary.is_empty() {
        let rect = body_text(ui, "summary", &draft.summary, images, cache);
        if running && draft.sections.is_empty() {
            writing_at = Some(rect);
        }
    } else if running && skeleton {
        widgets::skeleton_bar(ui, 0.9, 10.0);
        widgets::skeleton_bar(ui, 0.7, 10.0);
    }
    let mut headings: Vec<&str> = draft.sections.iter().map(|s| s.heading.as_str()).collect();
    if running {
        for heading in wanted {
            if !headings.iter().any(|h| h.eq_ignore_ascii_case(heading)) {
                headings.push(heading);
            }
        }
    }
    for (index, heading) in headings.into_iter().enumerate() {
        let section = draft
            .sections
            .iter()
            .position(|s| s.heading.eq_ignore_ascii_case(heading));
        let writing = running && section.is_some() && section == draft.writing;
        match section.map(|at| &draft.sections[at]) {
            Some(section) if !section.text.trim().is_empty() || writing => {
                let top = ui.cursor().min.y;
                section_heading(ui, heading, writing);
                let rect = body_text(ui, ("section", index), &section.text, images, cache);
                if writing {
                    writing_at = Some(rect);
                    let p = theme::palette(ui);
                    let left = ui.min_rect().min.x - theme::SPACE_4;
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            egui::Pos2::new(left, top),
                            egui::Pos2::new(left + 2.0, rect.max.y),
                        ),
                        CornerRadius::ZERO,
                        p.review,
                    );
                    let caret =
                        egui::Pos2::new(rect.max.x.min(ui.max_rect().max.x) + 2.0, rect.max.y);
                    ui.painter().line_segment(
                        [caret - Vec2::new(0.0, 16.0), caret],
                        Stroke::new(2.0, p.review),
                    );
                }
            }
            Some(_) => {}
            None if running && skeleton => {
                section_heading(ui, heading, false);
                widgets::skeleton_bar(ui, 0.95, 10.0);
                widgets::skeleton_bar(ui, 0.8, 10.0);
                widgets::skeleton_bar(ui, 0.5, 10.0);
            }
            None => {}
        }
    }
    writing_at
}

/// A `##` heading in `read-h2`; "writing" beside the one Review writes.
fn section_heading(ui: &mut egui::Ui, heading: &str, writing: bool) {
    let p = theme::palette(ui);
    ui.add_space(theme::SPACE_2);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        ui.label(RichText::new(heading).font(theme::ui_title()).color(p.ink));
        if writing {
            ui.label(RichText::new("writing").font(theme::meta()).color(p.review));
        }
    });
}

/// Markdown at reading size; its rect.
fn body_text(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
    images: &str,
    cache: &mut CommonMarkCache,
) -> Rect {
    ui.push_id(id, |ui| {
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Body, theme::read_body());
        ui.vertical(|ui| {
            CommonMarkViewer::new()
                .default_implicit_uri_scheme(images)
                .show(ui, cache, text);
        })
        .response
        .rect
    })
    .inner
}

/// Under the sheet, a ghost tile for each standard section a finished or
/// stopped draft left unwritten; a click adds the issue and opens the Editor
/// on that section.
fn unwritten(
    ui: &mut egui::Ui,
    composer: &Composer,
    view: &ComposeView,
    actions: &mut Vec<ComposeAction>,
) {
    let Some(drafted) = composer
        .draft
        .as_ref()
        .filter(|_| composer.drafting.is_none())
    else {
        return;
    };
    let missing = drafted.draft.unwritten(view.sections);
    if missing.is_empty() {
        return;
    }
    let hint = if drafted.stopped {
        "Stopped before it was written"
    } else {
        "Not written yet"
    };
    ui.add_space(theme::SPACE_4);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_3);
        let width = ((ui.available_width() - theme::SPACE_3) / 2.0).max(160.0);
        for heading in missing {
            ui.allocate_ui(Vec2::new(width, 64.0), |ui| {
                if widgets::ghost_section(ui, heading, Some(hint)).clicked()
                    && let Some(at) = view.sections.iter().position(|s| s == heading)
                {
                    actions.push(ComposeAction::SubmitForSection(at));
                }
            });
        }
    });
}

/// The title row on the sheet, in `read-h1`: the tags typed, the title, the
/// character count near the limit, and the Drafted tag, over a rule, dashed
/// when an empty title can only come from the first line.
fn title_row(ui: &mut egui::Ui, composer: &mut Composer, known: &Labels) {
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
    let title_id = egui::Id::new(TITLE_ID);
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
/// a draft.
fn foot(
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
                    if widgets::button(ui, ButtonKind::Primary, None, "Add to Todo", Some("↩"))
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
