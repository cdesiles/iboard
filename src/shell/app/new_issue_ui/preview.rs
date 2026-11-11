//! NewIssueModal's preview: the Reader sheet showing the file as `↩` will
//! write it, and its "Not written yet" ghost sections.

use super::super::compose::{Drafted, TitleStage};
use super::super::reader::{column_mark, sheet_rule};
use super::composer::title_row;
use super::{
    Align, ButtonKind, Color32, CommonMarkCache, CommonMarkViewer, ComposeAction, ComposeView,
    Composer, CornerRadius, Frame, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2, files,
    labels, theme, widgets,
};
use crate::new_issue;
use crate::shell::theme::Icon;

/// From here the drafting line says it is slow.
const STILL_WORKING_AFTER: u64 = 20;

/// The `bg` well with the Reader sheet: the file as it will be written.
pub(super) fn issue_pane(
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
        theme::pane_scroll("new-issue-sheet").content_margin(theme::scroll_margin(gutter, gutter)),
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
