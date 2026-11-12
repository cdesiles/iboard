//! The agent log's two views: the timeline of runs, messages and folded
//! activity, and the raw log with its line numbers.

use std::time::Instant;

use egui::text::{LayoutJob, TextWrapping};
use egui::{Align, Align2, Color32, CornerRadius, RichText, Sense, Stroke, Vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use super::super::theme;
use crate::build_log::{self, Entry, Item, RunLog, Segment};
use crate::diff::{FileDiff, FileStatus};
use crate::format::clock;

/// Where the live run's newest step, edit or thought sits in `entries`: none
/// before the run's heading or an outcome counts.
fn live_at(entries: &[Entry<'_>]) -> Option<usize> {
    let run = entries
        .iter()
        .rposition(|entry| matches!(entry, Entry::Run(_) | Entry::Outcome { .. }))
        .map_or(0, |at| at + 1);
    entries[run..]
        .iter()
        .rposition(|entry| {
            matches!(
                entry,
                Entry::Step(_) | Entry::Edit(_) | Entry::Thought { .. }
            )
        })
        .map(|at| run + at)
}

/// The step the live run is on, as the Now strip reads it: an edit's verb and
/// file, a tool's own verb and object, or "Thinking" with no object while the
/// model reasons. Before the first step it is the issue.
pub(super) fn live(text: &str) -> (String, String) {
    let entries = build_log::parse(text);
    let (verb, object) = match live_at(&entries).map(|at| &entries[at]) {
        Some(Entry::Edit(path)) => ("Editing", *path),
        Some(Entry::Step(step)) => build_log::step_parts(step),
        Some(Entry::Thought { .. }) => ("Thinking", ""),
        _ => ("Reading", "the issue"),
    };
    (verb.to_string(), object.to_string())
}

/// The Now strip: a `surface-sunken` band with the Thinking mark, the live
/// step, and its clock.
pub(super) fn now(ui: &mut egui::Ui, since: Instant, verb: &str, object: &str) {
    let p = theme::palette(ui);
    let (band, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
    let reading = format!("{verb} {object}");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, reading.trim()));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_live(egui::accesskit::Live::Polite);
    });
    ui.painter()
        .rect_filled(band, CornerRadius::same(theme::RADIUS_SM), p.surface_sunken);
    let rect = band.shrink2(Vec2::new(theme::SPACE_3, 0.0));
    let mark = egui::Rect::from_center_size(
        egui::Pos2::new(rect.left() + 7.0, rect.center().y),
        Vec2::splat(14.0),
    );
    let time = (!theme::reduced_motion(ui.ctx())).then(|| ui.input(|i| i.time));
    if time.is_some() {
        ui.ctx().request_repaint();
    }
    theme::thinking_mark(ui.painter(), mark, time, p.ink, p.accent);

    let clock_text = clock(since.elapsed());
    let clock_galley = ui
        .painter()
        .layout_no_wrap(clock_text, theme::meta(), p.ink_muted);
    let clock_left = rect.right() - clock_galley.size().x;
    ui.painter().galley(
        egui::Pos2::new(clock_left, rect.center().y - clock_galley.size().y / 2.0),
        clock_galley,
        p.ink_muted,
    );

    let mut x = rect.left() + 14.0 + theme::SPACE_2;
    let verb_galley = ui
        .painter()
        .layout_no_wrap(verb.to_string(), theme::meta(), p.ink_muted);
    let verb_top = rect.center().y - verb_galley.size().y / 2.0;
    ui.painter().galley(
        egui::Pos2::new(x, verb_top),
        verb_galley.clone(),
        p.ink_muted,
    );
    x += verb_galley.size().x + theme::SPACE_2;
    let width = (clock_left - theme::SPACE_3 - x).max(0.0);
    let galley = truncated(ui, object, theme::meta(), p.ink, width);
    ui.painter()
        .galley(egui::Pos2::new(x, verb_top), galley, p.ink);
}

/// A run's Changes row pressed: the file to show, and the run's files to
/// scope the Diff screen to.
pub(super) struct OpenDiff {
    pub path: String,
    pub files: Vec<String>,
}

/// The timeline: each run under its head, messages as the document, steps
/// folded into activity, and the run's changes before its outcome.
pub(super) fn timeline(
    ui: &mut egui::Ui,
    text: &str,
    cache: &mut CommonMarkCache,
    files: &[FileDiff],
    running: bool,
    open_diff: &mut Option<OpenDiff>,
) {
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Body, theme::read_body());
    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
    let runs = build_log::runs(build_log::segments(entries(text, running)));
    let last = runs.len().saturating_sub(1);
    for (at, run) in runs.iter().enumerate() {
        if at > 0 {
            ui.add_space(theme::SPACE_3);
            rule(ui);
        }
        run_head(ui, at + 1, run.head);
        run_body(ui, run, cache, at);
        let changes = changes(&run.edits, files);
        if !changes.is_empty() {
            changes_block(ui, &changes, running && at == last, at, open_diff);
        }
        if let Some((ok, text)) = run.outcome {
            outcome(ui, ok, text);
        }
    }
}

/// The hairline between two runs.
fn rule(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, p.line_soft),
    );
}

/// The entries a timeline shows: while a run is live, the step the Now strip
/// shows leaves the fold. An edit is not drawn either way, and it still
/// counts toward the run's changes.
fn entries(text: &str, running: bool) -> Vec<Entry<'_>> {
    let mut entries = build_log::parse(text);
    if running
        && let Some(at) = live_at(&entries)
        && matches!(entries[at], Entry::Step(_) | Entry::Thought { .. })
    {
        entries.remove(at);
    }
    entries
}

/// "Run 2" on the left, "today 14:02 · opencode" on the right.
fn run_head(ui: &mut egui::Ui, at: usize, head: &str) {
    let p = theme::palette(ui);
    let (when, agent) = build_log::run_parts(head);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Run {at}"))
                .font(theme::meta())
                .color(p.ink_muted)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_1;
            if !agent.is_empty() {
                ui.label(RichText::new(agent).font(theme::meta()).color(p.ink_muted));
                ui.label(RichText::new("·").font(theme::meta()).color(p.ink_faint));
            }
            if !head.is_empty() {
                ui.label(
                    RichText::new(build_log::when_label(when))
                        .font(theme::meta())
                        .color(p.ink_faint),
                );
            }
        });
    });
}

fn run_body(ui: &mut egui::Ui, run: &RunLog<'_>, cache: &mut CommonMarkCache, run_at: usize) {
    for (at, segment) in run.body.iter().enumerate() {
        match segment {
            Segment::Message(text) => prose(ui, (run_at, at), text, cache),
            Segment::Author(text) => author(ui, text),
            Segment::Activity(items) => activity(ui, (run_at, at), items),
            Segment::Step(step) => step_row(ui, step, false),
            Segment::Thought { secs, text } => {
                let id = ui.make_persistent_id(("thought", (run_at, at)));
                thought_row(ui, id, *secs, text, false);
            }
            Segment::Run(_) | Segment::Edit(_) | Segment::Outcome { .. } => {}
        }
    }
}

/// The agent's prose is the document here: no bubble, no avatar.
fn prose(ui: &mut egui::Ui, at: (usize, usize), text: &str, cache: &mut CommonMarkCache) {
    let width = theme::READ_MEASURE.min(ui.available_width());
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.push_id(("message", at), |ui| {
            CommonMarkViewer::new().show(ui, cache, text);
        });
    });
}

/// The author's instructions: the timeline's only tinted block.
fn author(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.with_layout(egui::Layout::right_to_left(Align::Min), |ui| {
        egui::Frame::new()
            .fill(p.accent_soft)
            .corner_radius(CornerRadius::same(theme::RADIUS_SM))
            .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
            .show(ui, |ui| {
                ui.set_max_width((ui.available_width() * 0.86).max(120.0));
                ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                ui.label(
                    RichText::new("You")
                        .font(theme::meta())
                        .color(p.accent)
                        .strong(),
                );
                ui.add(
                    egui::Label::new(RichText::new(text).font(theme::ui_small()).color(p.ink))
                        .wrap(),
                );
            });
    });
}

/// A burst of steps and thoughts in one row: "Thought 2 times, read 5 files,
/// searched 2", opening in place.
fn activity(ui: &mut egui::Ui, at: (usize, usize), items: &[Item<'_>]) {
    let p = theme::palette(ui);
    let summary = build_log::summary(items);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.make_persistent_id(("activity", at)),
        false,
    );
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
    if response.clicked() {
        state.set_open(!state.is_open());
    }
    let open = state.is_open();
    state.store(ui.ctx());
    let state_name = if open { "expanded" } else { "collapsed" };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            ui.is_enabled(),
            format!("{summary}, {state_name}"),
        )
    });
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
    }
    chevron(ui.painter(), rect.left_center(), open, p.ink_muted);
    ui.painter().text(
        egui::Pos2::new(rect.left() + 20.0, rect.center().y),
        Align2::LEFT_CENTER,
        &summary,
        theme::meta(),
        p.ink_muted,
    );
    if open {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let top = ui.cursor().min.y + theme::SPACE_1;
            for (item_at, item) in items.iter().enumerate() {
                match item {
                    Item::Step(step) => step_row(ui, step, true),
                    Item::Thought { secs, text } => {
                        let id = ui.make_persistent_id(("thought", at, item_at));
                        thought_row(ui, id, *secs, text, true);
                    }
                }
            }
            let bottom = ui.cursor().min.y - theme::SPACE_1;
            ui.painter().vline(
                rect.left() + 10.0,
                top..=bottom,
                Stroke::new(1.0, p.line_soft),
            );
        });
    }
}

/// One step's verb in ink-muted and its object in ink; commands in mono.
fn step_row(ui: &mut egui::Ui, step: &str, in_activity: bool) {
    let p = theme::palette(ui);
    let (verb, object) = build_log::step_parts(step);
    let command = build_log::kind_of(step) == build_log::Kind::Command;
    let indent = if in_activity { 20.0 } else { 0.0 };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        ui.set_min_height(24.0);
        ui.add_space(indent);
        ui.label(
            RichText::new(verb)
                .font(theme::ui_small())
                .color(p.ink_muted),
        );
        if !object.is_empty() {
            let font = if command {
                theme::edit_body()
            } else {
                theme::ui_small()
            };
            ui.add(egui::Label::new(RichText::new(object).font(font).color(p.ink)).truncate())
                .on_hover_text(object);
        }
    });
}

/// A thought: "Thinking" over the row in a burst, or "Thought for 6s" alone,
/// opening onto its whole text.
fn thought_row(ui: &mut egui::Ui, id: egui::Id, secs: u64, text: &str, in_activity: bool) {
    let p = theme::palette(ui);
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
    if response.clicked() {
        state.set_open(!state.is_open());
    }
    let open = state.is_open();
    state.store(ui.ctx());
    let state_name = if open { "expanded" } else { "collapsed" };
    let reading = if secs > 0 {
        format!("Thinking, {secs} seconds, {state_name}")
    } else {
        format!("Thinking, {state_name}")
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), reading.as_str())
    });
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
    }
    let chevron_x = if in_activity {
        rect.right() - 14.0
    } else {
        rect.left()
    };
    chevron(
        ui.painter(),
        egui::Pos2::new(chevron_x, rect.center().y),
        open,
        p.ink_muted,
    );
    let clue = if in_activity {
        "Thinking".to_string()
    } else if secs > 0 {
        format!("Thought for {secs}s")
    } else {
        "Thought".to_string()
    };
    let mut x = rect.left() + 20.0;
    let verb = ui
        .painter()
        .layout_no_wrap(clue, theme::ui_small(), p.ink_muted);
    let top = rect.center().y - verb.size().y / 2.0;
    ui.painter()
        .galley(egui::Pos2::new(x, top), verb.clone(), p.ink_muted);
    x += verb.size().x + theme::SPACE_3;

    let right = rect.right() - if in_activity { 26.0 } else { theme::SPACE_4 };
    let mut line_right = right;
    if secs > 0 && in_activity {
        let time = ui
            .painter()
            .layout_no_wrap(format!("{secs}s"), theme::meta(), p.ink_faint);
        let time_x = right - time.size().x;
        ui.painter().galley(
            egui::Pos2::new(time_x, rect.center().y - time.size().y / 2.0),
            time,
            p.ink_faint,
        );
        line_right = time_x - theme::SPACE_3;
    }
    if in_activity && !open {
        let first = text.lines().next().unwrap_or_default();
        let line = truncated(
            ui,
            first,
            theme::ui_small(),
            p.ink_faint,
            (line_right - x).max(0.0),
        );
        ui.painter()
            .galley(egui::Pos2::new(x, top), line, p.ink_faint);
    }
    if open {
        open_thought(ui, rect, text, in_activity);
    }
}

/// The whole text of an open thought: `ui-small` under the row, on the rail.
fn open_thought(ui: &mut egui::Ui, rect: egui::Rect, text: &str, in_activity: bool) {
    let p = theme::palette(ui);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(theme::SPACE_1);
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            let width = theme::READ_MEASURE.min(ui.available_width());
            ui.scope(|ui| {
                ui.set_max_width(width);
                ui.add(
                    egui::Label::new(
                        RichText::new(text)
                            .font(theme::ui_small())
                            .color(p.ink_muted),
                    )
                    .wrap(),
                );
            });
        });
        ui.add_space(theme::SPACE_2);
    });
    if !in_activity {
        ui.painter().vline(
            rect.left() + 10.0,
            rect.bottom()..=(ui.cursor().min.y - theme::SPACE_2),
            Stroke::new(1.0, p.line_soft),
        );
    }
}

/// One file a run touched, with its working-tree diff when the file still
/// shows one.
struct Change<'a> {
    path: &'a str,
    diff: Option<&'a FileDiff>,
}

/// The changes of `edits`, in the order the run made them, deduplicated and
/// matched to the working tree by their tail.
fn changes<'a>(edits: &[&'a str], files: &'a [FileDiff]) -> Vec<Change<'a>> {
    let mut changes: Vec<Change> = Vec::new();
    for edit in edits {
        let diff = files.iter().find(|file| same_path(file.path(), edit));
        let path = diff.map_or(*edit, |file| file.path());
        if !changes.iter().any(|change| change.path == path) {
            changes.push(Change { path, diff });
        }
    }
    changes
}

fn same_path(full: &str, edit: &str) -> bool {
    full == edit
        || full
            .strip_suffix(edit)
            .is_some_and(|head| head.ends_with('/'))
}

/// A run's receipt: how many files it changed, their diffstat, and a row per
/// file opening the Diff screen.
fn changes_block(
    ui: &mut egui::Ui,
    changes: &[Change<'_>],
    running: bool,
    run_at: usize,
    open_diff: &mut Option<OpenDiff>,
) {
    let p = theme::palette(ui);
    let files = changes.len();
    let noun = if files == 1 { "file" } else { "files" };
    let title = if running {
        format!("{files} {noun} changed so far")
    } else {
        format!("{files} {noun} changed")
    };
    let known = changes.iter().any(|change| change.diff.is_some());
    let added: usize = changes
        .iter()
        .filter_map(|change| change.diff)
        .map(|diff| diff.added)
        .sum();
    let removed: usize = changes
        .iter()
        .filter_map(|change| change.diff)
        .map(|diff| diff.removed)
        .sum();
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.make_persistent_id(("changes", run_at)),
        false,
    );
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
    if response.clicked() {
        state.set_open(!state.is_open());
    }
    let open = state.is_open();
    state.store(ui.ctx());
    let state_name = if open { "expanded" } else { "collapsed" };
    let stats = if known {
        format!("+{added} −{removed}")
    } else {
        String::new()
    };
    let reading = if stats.is_empty() {
        format!("{title}, {state_name}")
    } else {
        format!("{title}, {stats}, {state_name}")
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), reading.as_str())
    });
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_sunken);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    chevron(ui.painter(), rect.left_center(), open, p.ink_muted);
    ui.painter().text(
        egui::Pos2::new(rect.left() + 20.0, rect.center().y),
        Align2::LEFT_CENTER,
        &title,
        theme::ui_strong(),
        p.ink,
    );
    if known {
        diffstat(ui, rect, added, removed);
    }
    if open {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let paths: Vec<String> = changes
                .iter()
                .map(|change| change.path.to_string())
                .collect();
            for change in changes {
                change_row(ui, change, &paths, open_diff);
            }
        });
    }
}

/// `+added −removed`, right-aligned in `rect`.
fn diffstat(ui: &egui::Ui, rect: egui::Rect, added: usize, removed: usize) {
    let p = theme::palette(ui);
    let minus = format!("−{removed}");
    let plus = format!("+{added}");
    let minus = ui.painter().layout_no_wrap(minus, theme::meta(), p.danger);
    let plus = ui
        .painter()
        .layout_no_wrap(plus, theme::meta(), p.status_done);
    let right = rect.right() - theme::SPACE_4;
    let top = |galley: &egui::Galley| rect.center().y - galley.size().y / 2.0;
    let minus_x = right - minus.size().x;
    ui.painter()
        .galley(egui::Pos2::new(minus_x, top(&minus)), minus, p.danger);
    let plus_x = minus_x - theme::SPACE_3 - plus.size().x;
    ui.painter()
        .galley(egui::Pos2::new(plus_x, top(&plus)), plus, p.status_done);
}

/// One file of a run: its status letter, path, and diffstat; a button onto
/// the Diff screen when the working tree still has it.
fn change_row(
    ui: &mut egui::Ui,
    change: &Change<'_>,
    files: &[String],
    open_diff: &mut Option<OpenDiff>,
) {
    let p = theme::palette(ui);
    let sense = if change.diff.is_some() {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), sense);
    let response = response.on_hover_text(change.path);
    if response.hovered() && change.diff.is_some() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
    }
    let mut x = rect.left() + theme::SPACE_4;
    if let Some(diff) = change.diff {
        let letter = match diff.status() {
            FileStatus::Added => Some("A"),
            FileStatus::Deleted => Some("D"),
            FileStatus::Renamed => Some("R"),
            FileStatus::Modified => None,
        };
        if let Some(letter) = letter {
            ui.painter().text(
                egui::Pos2::new(x, rect.center().y),
                Align2::LEFT_CENTER,
                letter,
                theme::meta(),
                p.ink_faint,
            );
            x += 12.0;
        }
    }
    let room = (rect.right() - theme::SPACE_4 - x - 68.0).max(0.0);
    let path = truncated(ui, change.path, theme::ui_small(), p.ink, room);
    ui.painter().galley(
        egui::Pos2::new(x, rect.center().y - path.size().y / 2.0),
        path,
        p.ink,
    );
    match change.diff {
        Some(diff) => {
            diffstat(ui, rect, diff.added, diff.removed);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    ui.is_enabled(),
                    format!(
                        "Open the diff of {}, +{} −{}",
                        change.path, diff.added, diff.removed
                    ),
                )
            });
            if response.clicked() {
                *open_diff = Some(OpenDiff {
                    path: change.path.to_string(),
                    files: files.to_vec(),
                });
            }
        }
        None => {
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), change.path)
            });
        }
    }
}

/// How a run ended: the check and its duration, or the reason it stopped.
fn outcome(ui: &mut egui::Ui, ok: bool, text: &str) {
    let p = theme::palette(ui);
    if ok {
        let rest = text.strip_prefix("Finished").unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("✓")
                    .font(theme::ui_small())
                    .color(p.status_done),
            );
            ui.label(
                RichText::new("Finished")
                    .font(theme::meta())
                    .color(p.ink)
                    .strong(),
            );
            if !rest.trim().is_empty() {
                ui.label(
                    RichText::new(rest.trim_start())
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
            }
        });
    } else {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("✗").font(theme::ui_small()).color(p.danger));
            ui.label(
                RichText::new("Stopped:")
                    .font(theme::meta())
                    .color(p.danger)
                    .strong(),
            );
            ui.label(RichText::new(text).font(theme::ui_small()).color(p.danger));
        });
    }
}

/// The exact log: numbered rows in mono, colouring only the marks.
pub(super) fn raw(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.spacing_mut().item_spacing.y = 0.0;
    for (at, line) in text.lines().enumerate() {
        let color = if line.starts_with("── ") {
            p.review
        } else if line.starts_with("▸ ") {
            p.accent
        } else if line.starts_with("∴ ") {
            p.ink_muted
        } else if line.starts_with("✓ ") {
            p.status_done
        } else if line.starts_with("✗ ") {
            p.danger
        } else {
            p.ink
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            ui.add_sized(
                [28.0, 20.0],
                egui::Label::new(
                    RichText::new((at + 1).to_string())
                        .font(theme::meta())
                        .color(p.ink_faint),
                )
                .halign(Align::RIGHT),
            );
            ui.add(
                egui::Label::new(RichText::new(line).font(theme::edit_body()).color(color)).wrap(),
            );
        });
    }
}

/// A 12pt chevron at `center`, pointing down when `open`.
fn chevron(painter: &egui::Painter, center: egui::Pos2, open: bool, color: Color32) {
    let center = egui::Pos2::new(center.x + 7.0, center.y);
    let points = if open {
        vec![
            egui::Pos2::new(center.x - 4.0, center.y - 2.0),
            egui::Pos2::new(center.x, center.y + 2.0),
            egui::Pos2::new(center.x + 4.0, center.y - 2.0),
        ]
    } else {
        vec![
            egui::Pos2::new(center.x - 2.0, center.y - 4.0),
            egui::Pos2::new(center.x + 2.0, center.y),
            egui::Pos2::new(center.x - 2.0, center.y + 4.0),
        ]
    };
    painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
}

/// `text` on one line in `width`, its end replaced by an ellipsis.
fn truncated(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = TextWrapping {
        max_width: width.max(0.0),
        max_rows: 1,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}
