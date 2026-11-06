//! The reader: an issue's page, its sheet head, and what it still lacks.

use std::ops::Range;

use super::board::status_color;
use super::comments::{NoteAction, Notes, block_notes, outdated_notes};
use super::groups;
use super::intent::Intent;
use super::prose as prose_ui;
use super::{
    Align, ButtonKind, Color32, CommonMarkCache, CommonMarkViewer, CornerRadius, Frame, HashSet,
    IboardApp, Icon, Labels, Layout, Path, PathBuf, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2, images, relative_time, settings, sheet, theme, widgets,
};
use iboard::{markdown, preferences, prose};

/// The narrowest a pane beside the Reader leaves it (`column-min`).
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
    /// The images already reported missing, by issue and link.
    pub(super) reported: HashSet<(PathBuf, String)>,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            cache: CommonMarkCache::default(),
            preview_rect: Rect::NOTHING,
            reported: HashSet::new(),
        }
    }
}

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
    /// A meta head (column, path, modified, checklist progress or words), the
    /// rendered markdown, ghost blocks for missing standard sections, and the
    /// empty rest of the sheet, which edits at the end of the file.
    /// Checkboxes are live: ticking one writes the file. Double-clicking the
    /// text opens the editor.
    pub(super) fn detail_preview(&mut self, ui: &mut egui::Ui, path: &Path) {
        let Some(issue) = self.board.find(path) else {
            return;
        };
        // The title's `[label]` groups show as tags above it, not as text.
        let (text, title_line) = sheet::hide_title_labels(&issue.markdown);
        let p = theme::palette(ui);
        let (column, status, todo_issue) = self
            .board
            .column_of(path)
            .map(|column| {
                (
                    column.name.clone(),
                    status_color(p, column),
                    column.accepts_new_issues(),
                )
            })
            .unwrap_or_else(|| (String::new(), p.status_todo, false));
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
            head: SheetHead {
                column: &column,
                status,
                path: &self.relative(path),
                modified: issue.modified,
                checklist: sheet::checklist(&text),
                words: sheet::word_count(&text),
            },
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
        if sheet.editable && ui.rect_contains_pointer(outline) {
            paint_edit_outline(ui, outline);
        }
        self.reader.preview_rect = outline;

        self.report_missing_images(path, &outcome.missing);
        if !outcome.edits.is_empty() {
            let mut text = text.clone();
            for (range, block) in outcome.edits.into_iter().rev() {
                text.replace_range(range, &block);
            }
            if let Some((index, line)) = &title_line {
                text = sheet::restore_line(&text, *index, line);
            }
            self.write_issue(path, &text);
        }
        // One of them at most: they all come from clicks.
        intents.truncate(1);
        self.apply(intents);
    }

    /// The scrolling sheet: its head, the labels, the issue's blocks with
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
            edits: Vec::new(),
            missing: Vec::new(),
        };
        let mut note_actions = Vec::new();
        let cache = &mut self.reader.cache;
        let labels = &self.prefs.labels;
        theme::scroll_area(
            ui,
            egui::ScrollArea::vertical()
                .id_salt(("detail", path))
                .auto_shrink([false, false])
                .max_height(layout.scroll_height),
            |ui| {
                page(ui, |ui| {
                    let top = ui.cursor().min.y;
                    let inner_height = layout.sheet_height - 2.0 * theme::SPACE_6 - 2.0;
                    sheet_head(ui, sheet.head);
                    if let Some(label) = label_row(ui, sheet, labels) {
                        intents.push(Intent::ShowLabel(label));
                    }
                    let body = ui.vertical(|ui| {
                        // The body spans the sheet, so a short issue still
                        // gets an outline wide enough for its legend.
                        ui.set_min_width(ui.available_width());
                        // Reading sizes: `read-body` 14, `read-h1` 20.
                        let styles = &mut ui.style_mut().text_styles;
                        styles.insert(egui::TextStyle::Body, theme::read_body());
                        styles.insert(egui::TextStyle::Heading, theme::read_h1());
                        styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.5));
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let drawn = issue_blocks(ui, cache, path, sheet.text, notes.as_mut());
                        outcome.edits = drawn.edits;
                        outcome.missing = drawn.missing;
                        note_actions = drawn.actions;
                    });
                    outcome.body = body.response.rect;

                    // The rest of the sheet: empty, and a click edits at the
                    // end of the file.
                    let used = ui.cursor().min.y - top;
                    let rest_height = (inner_height - used).max(56.0);
                    let (rest, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), rest_height),
                        Sense::click(),
                    );
                    if sheet.editable {
                        let response = response.on_hover_cursor(egui::CursorIcon::Text);
                        if response.clicked() {
                            intents.push(Intent::EditAtEnd(path.to_path_buf()));
                        }
                        paint_rest_hint(ui, rest);
                    }
                });
            },
        );
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
    head: SheetHead<'a>,
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
    /// The blocks a ticked checkbox changed.
    edits: Vec<(Range<usize>, String)>,
    /// The links of the images that could not be shown.
    missing: Vec<String>,
}

/// What drawing the issue's blocks did.
struct DrawnBlocks {
    edits: Vec<(Range<usize>, String)>,
    actions: Vec<NoteAction>,
    missing: Vec<String>,
}

/// The reader's two blocks: the sheet, and under it the "Not written yet"
/// block, docked at the bottom of the window. The sheet takes the spare
/// height; the block stays put while a long issue scrolls.
struct ReaderLayout {
    full: Rect,
    page_left: f32,
    page_width: f32,
    tiles_across: usize,
    none_written: bool,
    block_height: f32,
    scroll_height: f32,
    /// The sheet fills its share between the header and the block.
    sheet_height: f32,
}

impl ReaderLayout {
    fn new(full: Rect, missing: usize, none_written: bool) -> Self {
        let gutter = theme::SPACE_5;
        let page_width = (full.width() - 2.0 * gutter).min(theme::READER_MAX);
        let page_left = full.min.x + ((full.width() - page_width) / 2.0).max(gutter);
        let tiles_across = if page_width >= 560.0 { 4 } else { 2 }.min(missing.max(1));
        let block_height = if none_written {
            let rows = missing.div_ceil(tiles_across) as f32;
            2.0 * theme::SPACE_4
                + theme::CONTROL_H
                + theme::SPACE_3
                + rows * GHOST_TILE_H
                + (rows - 1.0) * theme::SPACE_3
        } else if missing > 0 {
            40.0
        } else {
            0.0
        };
        let block_gap = if block_height > 0.0 {
            theme::SPACE_4
        } else {
            0.0
        };
        let scroll_height = (full.height() - block_height - block_gap).max(120.0);
        Self {
            full,
            page_left,
            page_width,
            tiles_across,
            none_written,
            block_height,
            scroll_height,
            sheet_height: scroll_height - 2.0 * gutter,
        }
    }

    /// The "Not written yet" block, when sections are missing; what it picked.
    fn not_written(
        &self,
        ui: &mut egui::Ui,
        missing: &[String],
        review_ready: bool,
    ) -> Option<NotWritten> {
        if self.block_height <= 0.0 {
            return None;
        }
        let rect = Rect::from_min_size(
            Pos2::new(
                self.page_left,
                self.full.max.y - theme::SPACE_5 - self.block_height,
            ),
            Vec2::new(self.page_width, self.block_height),
        );
        let mut block = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
        );
        not_written_block(
            &mut block,
            rect,
            missing,
            self.none_written,
            self.tiles_across,
            review_ready,
        )
    }
}

/// The labels, on their own row between the head and the title. Returns the
/// one clicked, to filter the board by.
fn label_row(ui: &mut egui::Ui, sheet: &SheetView, labels: &Labels) -> Option<String> {
    let names = &sheet.labels;
    if names.is_empty() {
        return None;
    }
    let mut clicked = None;
    // The row sits closer to the rule than the title does without labels.
    ui.add_space(-theme::SPACE_3);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_2);
        for name in names.iter().filter(|name| !name.starts_with("group:")) {
            let shown = name.strip_prefix("repo:").unwrap_or(name);
            let active = sheet.active_labels.contains(name);
            if widgets::label_pill_lit(ui, shown, labels.color(name), active)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!("Filter by {name}"))
                .clicked()
            {
                clicked = Some(name.clone());
            }
        }
    });
    ui.add_space(theme::SPACE_3 - ui.spacing().item_spacing.y);
    clicked
}

/// Hovering the text shows that it is one edit target: a `line-strong`
/// outline with the hint on its top edge.
fn paint_edit_outline(ui: &egui::Ui, outline: Rect) {
    let p = theme::palette(ui);
    let painter = ui.painter();
    painter.rect_stroke(
        outline,
        CornerRadius::same(theme::RADIUS_SM),
        Stroke::new(1.0, p.line_strong),
        StrokeKind::Outside,
    );
    let hint = egui::RichText::new("Double-click to edit")
        .font(egui::FontId::new(11.0, theme::ui_button().family))
        .color(p.ink_muted);
    let galley = egui::WidgetText::from(hint).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    );
    let kbd_width = 18.0;
    let key = widgets::key_hints_shown(ui.ctx());
    let key_width = if key { kbd_width + theme::SPACE_2 } else { 0.0 };
    let right = outline.max.x - theme::SPACE_3;
    let y = outline.min.y;
    let text_left = right - key_width - galley.size().x;
    let legend = Rect::from_min_max(
        Pos2::new(text_left - theme::SPACE_2, y - 8.0),
        Pos2::new(right + theme::SPACE_2, y + 8.0),
    );
    painter.rect_filled(legend, CornerRadius::ZERO, p.surface);
    if key {
        widgets::paint_kbd(ui, "E", Pos2::new(right - kbd_width, y));
    }
    let text_pos = Pos2::new(text_left, y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, p.ink_muted);
}

/// The issue's text, one top-level block at a time so each has a margin to
/// hang comments on (`notes`, for an issue that takes them). Returns the
/// blocks a ticked checkbox changed, what was asked of the comments, and the
/// images that could not be shown, drawn as placeholders.
fn issue_blocks(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    path: &Path,
    text: &str,
    mut notes: Option<&mut Notes>,
) -> DrawnBlocks {
    let blocks = markdown::blocks(text);
    let mut edits = Vec::new();
    let mut actions = Vec::new();
    let mut missing = Vec::new();
    let mut below: f32 = 0.0;
    for (index, range) in blocks.iter().enumerate() {
        let block = &text[range.clone()];
        let parsed = prose::parse(block);
        let (top, bottom) = parsed
            .as_ref()
            .and_then(|parsed| parsed.first())
            .map_or((theme::SPACE_3, theme::SPACE_3), prose_ui::margins);
        if index > 0 {
            ui.add_space(below.max(top));
        }
        below = bottom;
        let shown = ui
            .push_id(("block", index), |ui| match &parsed {
                Some(parsed) => {
                    let mut ticked = None;
                    let drawn = ui.vertical(|ui| prose_ui::show(ui, parsed, &mut ticked));
                    BlockDrawn {
                        rect: drawn.response.rect,
                        edited: prose_ui::apply(block, ticked),
                    }
                }
                None => viewer_block(ui, cache, path, block, &mut missing),
            })
            .inner;
        if let Some(edited) = shown.edited {
            edits.push((range.clone(), edited));
        }
        if let Some(notes) = notes.as_deref_mut() {
            let anchor = text[range.clone()].trim();
            let row = shown.rect.with_max_x(ui.max_rect().right());
            actions.extend(block_notes(ui, notes, anchor, row, index));
        }
    }
    if let Some(notes) = notes {
        actions.extend(outdated_notes(ui, notes, text, &blocks));
    }
    DrawnBlocks {
        edits,
        actions,
        missing,
    }
}

/// Where a block was drawn, and its text when a ticked box changed it.
struct BlockDrawn {
    rect: Rect,
    edited: Option<String>,
}

/// A block that is more than prose, drawn by the markdown viewer: its lines
/// keep the viewer's height. `missing` receives the images it could not show.
fn viewer_block(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    path: &Path,
    block: &str,
    missing: &mut Vec<String>,
) -> BlockDrawn {
    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
    let mut edited = block.to_string();
    let broken = images::missing_images(path, block);
    let table = markdown::is_table(block);
    let mut draw = |ui: &mut egui::Ui| {
        let viewer = CommonMarkViewer::new().default_implicit_uri_scheme(image_base_uri(path));
        if broken.is_empty() {
            viewer.show_mut(ui, cache, &mut edited).response
        } else {
            let placeheld = images::with_placeholders(block, &broken);
            viewer.show(ui, cache, &placeheld).response
        }
    };
    // Table cells do not wrap: a wide table scrolls within the page.
    let shown = match table {
        true => egui::ScrollArea::horizontal().show(ui, draw).inner,
        false => draw(ui),
    };
    missing.extend(broken.into_iter().map(|link| link.url));
    BlockDrawn {
        rect: shown.rect,
        edited: shown.changed().then_some(edited),
    }
}

/// The `file://` base a relative image link in `issue` resolves against: the
/// issue's own folder, so `assets/<stem>/ui.png` finds the mockup written beside
/// it. Passed to the markdown viewer as its implicit URI scheme.
pub(super) fn image_base_uri(issue: &Path) -> String {
    let dir = issue.parent().unwrap_or(Path::new("."));
    format!("file://{}/", dir.display())
}

/// Lays `add_contents` out in a column `reader-max` wide, centred, with the
/// window gutter around it.
pub(super) fn page_column<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let gutter = theme::SPACE_5;
    let width = (ui.available_width() - 2.0 * gutter).min(theme::READER_MAX);
    let left = ((ui.available_width() - width) / 2.0).max(gutter);
    let rect = Rect::from_min_size(
        ui.cursor().min + Vec2::new(left, gutter),
        Vec2::new(width, (ui.available_height() - 2.0 * gutter).max(0.0)),
    );
    let result = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
            add_contents,
        )
        .inner;
    ui.add_space(gutter);
    result
}

/// Height of a ghost tile in the "Not written yet" block.
const GHOST_TILE_H: f32 = 72.0;

/// What the "Not written yet" block asked for.
enum NotWritten {
    Section(String),
    Review,
}

/// The "Not written yet" block under the reader sheet: its own `surface`
/// card. With nothing written, a head row (label, Draft with Review) over a
/// grid of ghost tiles; once some sections exist, one line of chips.
fn not_written_block(
    ui: &mut egui::Ui,
    rect: Rect,
    missing: &[String],
    none_written: bool,
    across: usize,
    review_ready: bool,
) -> Option<NotWritten> {
    let p = theme::palette(ui);
    ui.painter().rect(
        rect,
        CornerRadius::same(theme::RADIUS_MD),
        p.surface,
        Stroke::new(1.0, p.line_soft),
        StrokeKind::Inside,
    );
    let inner = rect.shrink2(Vec2::new(theme::SPACE_5, theme::SPACE_4));
    let mut ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    let mut picked = None;
    let label = |ui: &mut egui::Ui| {
        ui.label(
            RichText::new("NOT WRITTEN YET")
                .font(theme::ui_label())
                .extra_letter_spacing(theme::LABEL_TRACKING)
                .color(p.ink_muted),
        );
    };
    if !none_written {
        // One 40pt line: the label, then a chip per missing section.
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            label(ui);
            for section in missing {
                if widgets::chip(ui, &format!("+ {section}"), false)
                    .on_hover_text(format!("Add ## {section} in the editor"))
                    .clicked()
                {
                    picked = Some(NotWritten::Section(section.clone()));
                }
            }
        });
        return picked;
    }

    ui.allocate_ui_with_layout(
        Vec2::new(inner.width(), theme::CONTROL_H),
        Layout::left_to_right(Align::Center),
        |ui| {
            label(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if draft_with_review(ui, review_ready) {
                    picked = Some(NotWritten::Review);
                }
            });
        },
    );
    ui.add_space(theme::SPACE_3);
    let gap = theme::SPACE_3;
    let tile_w = (inner.width() - gap * (across as f32 - 1.0)) / across as f32;
    let top = ui.cursor().min.y;
    for (index, section) in missing.iter().enumerate() {
        let (row, col) = (index / across, index % across);
        let tile = Rect::from_min_size(
            Pos2::new(
                inner.min.x + col as f32 * (tile_w + gap),
                top + row as f32 * (GHOST_TILE_H + gap),
            ),
            Vec2::new(tile_w, GHOST_TILE_H),
        );
        let mut cell = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(tile)
                .layout(Layout::top_down(Align::Min)),
        );
        if widgets::ghost_section(&mut cell, section, sheet::section_hint(section))
            .on_hover_text(format!("Add ## {section} in the editor"))
            .clicked()
        {
            picked = Some(NotWritten::Section(section.clone()));
        }
    }
    picked
}

/// "Draft with Review", or the same button disabled while no agent is set
/// up. True when it was clicked.
fn draft_with_review(ui: &mut egui::Ui, review_ready: bool) -> bool {
    let kind = ButtonKind::ReviewHairline.when(review_ready);
    let response = widgets::button(
        ui,
        kind,
        Some(Icon::Review),
        "Draft with Review",
        Some("⌘⇧R"),
    );
    if review_ready {
        response
            .on_hover_text("Ask the review agent to propose these sections")
            .clicked()
    } else {
        response.on_hover_text("No agent is set up yet: choose one in Settings › AI review (⌘,)");
        false
    }
}

/// The reader's `surface` page: `reader-max` wide, centred, 24×32 padding.
pub(super) fn page<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    page_column(ui, |ui| {
        let p = theme::palette(ui);
        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_soft))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(Vec2::new(theme::SPACE_7, theme::SPACE_6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add_contents(ui)
            })
    })
}

/// What the reader sheet's head strip shows.
#[derive(Clone, Copy)]
struct SheetHead<'a> {
    column: &'a str,
    status: Color32,
    path: &'a str,
    modified: Option<std::time::SystemTime>,
    /// `(ticked, total)` checkboxes; with none, the word count shows instead.
    checklist: (usize, usize),
    words: usize,
}

/// The sheet's `meta` head: the column as a status mark and the file path on
/// the left, "modified 1h ago" and checklist progress (or the word count) on
/// the right, over a `line` rule.
fn sheet_head(ui: &mut egui::Ui, head: SheetHead) {
    let p = theme::palette(ui);
    let meta = |text: String, color: Color32| {
        egui::Label::new(RichText::new(text).font(theme::meta()).color(color))
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if !head.column.is_empty() {
            column_mark(ui, head.column, head.status);
            ui.add(meta("·".into(), p.ink_faint));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (ticked, total) = head.checklist;
            if total > 0 {
                ui.add(meta(format!("{ticked}/{total} done"), p.ink_muted));
                ui.spacing_mut().item_spacing.x = 6.0;
                widgets::meter(ui, ticked, total);
                ui.spacing_mut().item_spacing.x = 10.0;
            } else {
                let unit = if head.words == 1 { "word" } else { "words" };
                ui.add(meta(format!("{} {unit}", head.words), p.ink_muted));
            }
            if let Some(modified) = head.modified {
                ui.add(meta("·".into(), p.ink_faint));
                ui.add(meta(
                    format!("modified {}", relative_time(modified)),
                    p.ink_muted,
                ));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(meta(head.path.to_string(), p.ink_muted).truncate());
            });
        });
    });
    ui.add_space(theme::SPACE_4);
    sheet_rule(ui);
    ui.add_space(20.0);
}

/// A sheet head's column: its status dot and its name in capitals.
pub(super) fn column_mark(ui: &mut egui::Ui, column: &str, status: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 4.0, status);
        ui.label(
            RichText::new(column.to_uppercase())
                .font(theme::ui_label())
                .extra_letter_spacing(theme::LABEL_TRACKING)
                .color(theme::palette(ui).ink),
        );
    });
}

/// The `line` rule under a sheet's head.
pub(super) fn sheet_rule(ui: &mut egui::Ui) {
    let line = theme::palette(ui).line;
    let (rule, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(rule.x_range(), rule.center().y, Stroke::new(1.0, line));
}

/// "Click below the text to keep writing" in `ink-faint`, at the foot of the
/// empty rest of the sheet, with its key `E` while ⌘ is held.
pub(super) fn paint_rest_hint(ui: &egui::Ui, rest: Rect) {
    let p = theme::palette(ui);
    let text = egui::WidgetText::from(
        RichText::new("Click below the text to keep writing")
            .font(theme::ui_small())
            .color(p.ink_faint),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    );
    let key = widgets::key_hints_shown(ui.ctx());
    let gap = 6.0;
    let kbd_width = if key { gap + 18.0 } else { 0.0 };
    let width = theme::ICON_SIZE + gap + text.size().x + kbd_width;
    let y = rest.max.y - 8.0;
    let mut x = rest.center().x - width / 2.0;
    let icon = Rect::from_min_size(
        Pos2::new(x, y - theme::ICON_SIZE / 2.0),
        Vec2::splat(theme::ICON_SIZE),
    );
    theme::paint_icon(ui.painter(), Icon::Edit, icon, p.ink_faint);
    x += theme::ICON_SIZE + gap;
    ui.painter().galley(
        Pos2::new(x, y - text.size().y / 2.0),
        text.clone(),
        p.ink_faint,
    );
    if key {
        widgets::paint_kbd(ui, "E", Pos2::new(x + text.size().x + gap, y));
    }
}
