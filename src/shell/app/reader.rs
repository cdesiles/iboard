//! The reader: an issue's page and what it still lacks. The blocks it draws
//! live in `sections`, their lines in `lines`, and the page's chrome in
//! `chrome`.

use std::ops::Range;

use super::comments::{NoteAction, Notes, block_notes, outdated_notes};
use super::groups;
use super::intent::Intent;
use super::prose as prose_ui;
use super::{
    Align, ButtonKind, Color32, CommonMarkCache, CommonMarkViewer, CornerRadius, Frame, HashSet,
    IboardApp, Icon, Labels, Layout, Path, PathBuf, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2, images, settings, sheet, theme, widgets,
};
use crate::preferences;

mod chrome;
mod lines;
mod sections;

use chrome::{
    NotWritten, REST_HINT, ReaderLayout, label_row, page, paint_edit_outline, paint_rest_hint,
};
pub(super) use chrome::{column_mark, page_column, sheet_rule};
use lines::{BlockLines, restored_text};
pub(super) use sections::image_base_uri;
use sections::issue_blocks;

/// The narrowest a pane beside the Reader leaves it.
const PANE_MIN: f32 = 280.0;

/// Width of the AI review panel beside the reader or editor page.
pub(super) const REVIEW_PANEL_WIDTH: f32 = 360.0;

/// The pairs of panes whose split is kept in `[splits]`.
const PANE_SPLITS: [&str; 4] = ["review", "log", "refine", "new-issue"];

/// A child of `ui` filling and clipped to `rect`, whose scroll offsets stay
/// the same wherever `rect` is.
fn pane_ui(ui: &mut egui::Ui, salt: &str, rect: Rect) -> egui::Ui {
    let mut pane = ui.new_child(egui::UiBuilder::new().id_salt(salt).max_rect(rect));
    pane.shrink_clip_rect(rect);
    pane
}

/// The reader's own state.
pub(super) struct Reader {
    pub(super) cache: CommonMarkCache,
    /// The rendered issue content, used to tell a double-click on the page
    /// from one on the header when deciding whether to start editing.
    pub(super) preview_rect: Rect,
    /// `preview_rect` where it shows: a long page runs on under the bars and
    /// the "Not written yet" block.
    pub(super) sheet_rect: Rect,
    /// The part of the rest area, the page below the text, that shows.
    pub(super) rest_rect: Rect,
    /// When the pointer last went down off the sheet.
    pub(super) stray_press: f64,
    /// What a key asked of the page's scroll, and whether down.
    pub(super) scroll: Option<(Stride, bool)>,
    /// Where each block of the issue was drawn, for a double-click to find
    /// its line.
    blocks: Vec<BlockLines>,
    /// The images already reported missing, by issue and link.
    pub(super) reported: HashSet<(PathBuf, String)>,
    images: images::ImageChecks,
    /// The issue whose `read-h1` has scrolled out of view: its title joins
    /// the breadcrumb.
    pub(super) title_away: Option<PathBuf>,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            cache: CommonMarkCache::default(),
            preview_rect: Rect::NOTHING,
            sheet_rect: Rect::NOTHING,
            rest_rect: Rect::NOTHING,
            stray_press: f64::NEG_INFINITY,
            scroll: None,
            blocks: Vec::new(),
            reported: HashSet::new(),
            images: images::ImageChecks::default(),
            title_away: None,
        }
    }
}

impl Reader {
    /// Whether a click just now could pair up with a press off the page, as
    /// the click that closed a menu: no double-click on what it uncovered.
    pub(super) fn stray_double_click(&self, ctx: &egui::Context) -> bool {
        let delay = ctx.options(|o| o.input_options.max_double_click_delay);
        ctx.input(|i| i.time - self.stray_press <= delay)
    }
}

/// How far a key scrolls the Reader's page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stride {
    /// `↑ ↓`: [`LINE_STRIDE`].
    Line,
    /// `space`, `fn ↑ ↓`: [`PAGE_STRIDE`] of the pane, as a click in the
    /// scrollbar's strip.
    Page,
}

/// Two lines of `read-body`.
const LINE_STRIDE: f32 = 48.0;
const PAGE_STRIDE: f32 = 0.9;

impl IboardApp {
    pub(super) fn detail_ui(&mut self, ui: &mut egui::Ui, path: &Path) {
        if self.board.find(path).is_none() {
            page(ui, |ui| {
                ui.label(
                    RichText::new(
                        "This issue no longer exists. It may have been moved or deleted.",
                    )
                    .color(theme::palette(ui).ink_muted),
                );
            });
            return;
        }

        let reviewed = self
            .session
            .review
            .as_ref()
            .is_some_and(|review| review.path == path);
        let refined = self
            .session
            .refine
            .as_ref()
            .is_some_and(|refine| refine.path == path);
        let reviewing = !reviewed && self.review_is_running(path);
        let logged = self.agent_log_open(path);
        let panes = ui.max_rect();
        let side = reviewed || reviewing || logged;
        let stacked = side && panes.width() < PANE_MIN + REVIEW_PANEL_WIDTH;
        let widest = (panes.width() - PANE_MIN).max(REVIEW_PANEL_WIDTH);
        let range = REVIEW_PANEL_WIDTH..=widest;
        let split = theme::pane_split_id(if reviewed || reviewing {
            "review"
        } else {
            "log"
        });
        let (issue_rect, side_rect) = if stacked {
            panes.split_top_bottom_at_fraction(0.5)
        } else if side {
            let width = theme::pane_split(ui.ctx(), split, REVIEW_PANEL_WIDTH)
                .clamp(*range.start(), *range.end());
            panes.split_left_right_at_x(panes.right() - width)
        } else {
            (panes, Rect::NOTHING)
        };

        // Each pane in its own Ui, the same one with or without a side panel,
        // so opening one keeps the issue's scroll offset. The issue comes
        // first: tab goes from its content to the divider to the panel.
        let mut issue_ui = pane_ui(ui, "issue-pane", issue_rect);
        if self.session.editing().is_some() {
            self.detail_edit(&mut issue_ui, path);
        } else {
            self.detail_preview(&mut issue_ui, path);
        }
        if side {
            let gutter = theme::SPACE_5;
            let divider = (!stacked).then(|| {
                let strip = side_rect.with_max_x(side_rect.left() + gutter);
                theme::pane_divider(ui, split, panes, strip, range, REVIEW_PANEL_WIDTH, false)
            });
            let mut side_ui = pane_ui(ui, "side-pane", side_rect);
            Frame::new()
                .fill(theme::palette(ui).bg)
                .inner_margin(gutter)
                .show(&mut side_ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    if reviewed {
                        self.review_panel(ui);
                    } else if reviewing {
                        self.running_review_panel(ui, path);
                    } else {
                        self.agent_log_panel(ui);
                    }
                });
            if let Some(divider) = divider {
                divider.paint(ui);
            }
        }
        if refined {
            self.refine_popin(ui);
        }
    }

    /// Puts the splits a previous run saved back in place.
    pub(super) fn restore_splits(&self, ctx: &egui::Context) {
        for (name, width) in &self.prefs.splits {
            theme::restore_pane_split(ctx, theme::pane_split_id(name), *width as f32);
        }
    }

    /// Saves a split that changed in `[splits]`, once the mouse lets go. It
    /// is view state: a file that cannot be written just keeps the old one.
    pub(super) fn remember_splits(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let mut changed = false;
        for name in PANE_SPLITS {
            let id = theme::pane_split_id(name);
            let Some(width) = theme::stored_pane_split(ctx, id) else {
                continue;
            };
            let width = width.round() as u32;
            if self.prefs.splits.get(name) != Some(&width) {
                self.prefs.splits.insert(name.to_string(), width);
                changed = true;
            }
        }
        if changed {
            self.remember_section(preferences::splits_section(&self.prefs.splits));
        }
    }

    /// Writes view state, what iboard keeps for you between runs: a file that
    /// cannot be written just keeps the old one.
    pub(super) fn remember_section(&mut self, section: Result<settings::Section, String>) {
        let Some(file) = self.config_file.clone() else {
            return;
        };
        if let Ok(section) = section
            && settings::save(&file, &[section]).is_ok()
        {
            self.config_stamp = groups::modified(&file);
        }
    }

    /// The reader sheet: a `surface` page that reaches down to the status bar.
    /// The labels, the rendered markdown, ghost blocks for missing standard
    /// sections, and the empty rest of the sheet, which edits at the end of
    /// the file. Checkboxes are live: ticking one writes the file.
    /// Double-clicking the text opens the editor at that line.
    pub(super) fn detail_preview(&mut self, ui: &mut egui::Ui, path: &Path) {
        let Some(issue) = self.board.find(path) else {
            return;
        };
        // The title's `[label]` groups show as tags above it, not as text.
        let (text, title_line) = sheet::hide_title_labels(&issue.markdown);
        let todo_issue = self
            .board
            .column_of(path)
            .is_some_and(|column| column.accepts_new_issues());
        let sections = &self.board.reader_sections;
        // Only Todo issues are still being written: Ready, Doing and Done
        // ones never get the "Not written yet" block.
        let missing: Vec<String> = if todo_issue {
            sheet::missing_sections(&text, sections)
                .into_iter()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let layout = ReaderLayout::new(
            ui.available_rect_before_wrap(),
            missing.len(),
            !missing.is_empty() && missing.len() == sections.len(),
        );
        let sheet = SheetView {
            path,
            labels: issue.labels.clone(),
            active_labels: self.filter_bar.filter.tokens.clone(),
            text: &text,
            // Doing and Done tickets open read-only: no click-to-edit affordance.
            editable: self.can_edit(path),
            // Only Todo issues are reviewed, so only they take comments.
            comments: todo_issue,
        };

        // The rendered reader is not a text surface: keep labels inert so a
        // double-click reliably opens the editor instead of selecting a word.
        ui.style_mut().interaction.selectable_labels = false;
        let mut intents = Vec::new();
        let outcome = self.reader_sheet(ui, &sheet, &layout, &mut intents);
        if let Some(picked) = layout.not_written(ui, &missing, self.prefs.agent.configured()) {
            intents.push(match picked {
                NotWritten::Section(section) => Intent::AddSection {
                    path: path.to_path_buf(),
                    section,
                },
                NotWritten::Review => Intent::Review(path.to_path_buf()),
            });
        }
        let outline = outcome.body.expand(theme::SPACE_3);
        let hovered = sheet.editable && ui.rect_contains_pointer(outline);
        let fade = widgets::hover_fade(ui.ctx(), egui::Id::unique("reader_edit_outline"), hovered);
        if fade > 0.0 {
            let mut faded = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
            faded.multiply_opacity(fade);
            paint_edit_outline(&faded, outline);
        }
        self.reader.preview_rect = outline;
        self.reader.sheet_rect = outline.intersect(outcome.view);
        self.reader.rest_rect = outcome.rest.intersect(outcome.view);
        let away = outcome.title_away.then(|| path.to_path_buf());
        if self.reader.title_away != away {
            self.reader.title_away = away;
            ui.ctx().request_repaint();
        }
        self.reader.blocks = outcome.blocks;

        self.report_missing_images(path, &outcome.missing);
        if !outcome.edits.is_empty() {
            let text = restored_text(&text, title_line.as_ref(), outcome.edits);
            self.write_issue(path, &text);
        }
        // One of them at most: they all come from clicks.
        intents.truncate(1);
        self.apply(intents);
    }

    /// The scrolling sheet: the labels, the issue's blocks with
    /// their comments, and the empty rest that edits at the end of the file.
    fn reader_sheet(
        &mut self,
        ui: &mut egui::Ui,
        sheet: &SheetView,
        layout: &ReaderLayout,
        intents: &mut Vec<Intent>,
    ) -> SheetOutcome {
        let path = sheet.path;
        let mut notes = if sheet.comments {
            self.notes_for(path);
            self.session.notes.take()
        } else {
            None
        };
        let mut outcome = SheetOutcome {
            body: Rect::NOTHING,
            view: Rect::NOTHING,
            rest: Rect::NOTHING,
            edits: Vec::new(),
            missing: Vec::new(),
            title_away: false,
            blocks: Vec::new(),
        };
        let mut note_actions = Vec::new();
        let stray = self.reader.stray_double_click(ui.ctx());
        let scroll = self.reader.scroll.take().map(|(stride, down)| {
            let by = match stride {
                Stride::Line => LINE_STRIDE,
                Stride::Page => PAGE_STRIDE * layout.scroll_height,
            };
            if down { by } else { -by }
        });
        let cache = &mut self.reader.cache;
        let images = &mut self.reader.images;
        images.renew(self.board_generation);
        let labels = &self.prefs.labels;
        outcome.view = theme::scroll_area(
            ui,
            egui::ScrollArea::vertical()
                .id_salt(("detail", path))
                .auto_shrink([false, false])
                .max_height(layout.scroll_height),
            |ui| {
                page(ui, |ui| {
                    let top = ui.cursor().min.y;
                    let inner_height = layout.sheet_height - 2.0 * theme::SPACE_7;
                    if let Some(label) = label_row(ui, sheet, labels) {
                        intents.push(Intent::ShowLabel(label));
                    }
                    let body = ui.vertical(|ui| {
                        // The body spans the sheet, so a short issue still
                        // gets an outline wide enough for its legend.
                        ui.set_min_width(ui.available_width());
                        // What the markdown viewer draws, in the reading sizes.
                        let styles = &mut ui.style_mut().text_styles;
                        styles.insert(egui::TextStyle::Body, theme::read_body());
                        styles.insert(egui::TextStyle::Heading, theme::read_h1());
                        styles.insert(
                            egui::TextStyle::Monospace,
                            egui::FontId::monospace(prose_ui::CODE_SIZE),
                        );
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let drawn =
                            issue_blocks(ui, cache, images, path, sheet.text, notes.as_mut());
                        outcome.edits = drawn.edits;
                        outcome.missing = drawn.missing;
                        let view_top = ui.clip_rect().top();
                        outcome.title_away =
                            drawn.title_bottom.is_none_or(|bottom| bottom < view_top);
                        note_actions = drawn.actions;
                        outcome.blocks = drawn.blocks;
                    });
                    outcome.body = body.response.rect;

                    // The rest of the sheet: empty, and a double-click edits
                    // at the end of the file.
                    let used = ui.cursor().min.y - top;
                    let rest_height = (inner_height - used).max(56.0);
                    let (rest, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), rest_height),
                        Sense::click(),
                    );
                    outcome.rest = rest;
                    if sheet.editable {
                        let response = response.on_hover_cursor(egui::CursorIcon::Text);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, REST_HINT)
                        });
                        if response.double_clicked() && !stray {
                            intents.push(Intent::EditAtEnd(path.to_path_buf()));
                        }
                        paint_rest_hint(ui, rest);
                    }
                });
                if let Some(by) = scroll {
                    let delta = Vec2::new(0.0, -by);
                    ui.scroll_with_delta_animation(delta, egui::style::ScrollAnimation::none());
                }
            },
        )
        .inner_rect;
        if notes.is_some() {
            self.session.notes = notes;
        }
        for action in note_actions {
            self.apply_note_action(path, action);
        }
        outcome
    }
}

/// What the reader sheet shows of one issue.
struct SheetView<'a> {
    path: &'a Path,
    labels: Vec<String>,
    /// The labels the board is filtered by: they show as active.
    active_labels: Vec<String>,
    /// The issue's text, its title's labels hidden.
    text: &'a str,
    editable: bool,
    /// The issue takes margin comments.
    comments: bool,
}

/// What one frame of the sheet did.
struct SheetOutcome {
    /// Where the issue's text was drawn, for the edit outline.
    body: Rect,
    /// The part of the pane the page scrolls in.
    view: Rect,
    /// The empty rest of the sheet, below the text.
    rest: Rect,
    /// The blocks a ticked checkbox changed.
    edits: Vec<(Range<usize>, String)>,
    /// The links of the images that could not be shown.
    missing: Vec<String>,
    /// The page's title is out of view, scrolled away or never there.
    title_away: bool,
    blocks: Vec<BlockLines>,
}
