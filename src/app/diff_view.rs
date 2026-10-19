//! The Diff screen: the commits that closed an issue, as a unified diff with
//! its files listed on the left, syntax colours, and a find bar.

use std::collections::{HashMap, HashSet};

use egui::text::{LayoutJob, TextFormat};
use iboard::diff::{self, CommitDiff, FileDiff, FileStatus, Line, LineKind};

use super::diff_syntax::{self, FileColours, Span};
use super::*;

/// Every row of the diff is this tall, so only the visible ones are drawn.
const ROW_H: f32 = 18.0;
/// Files with more diff lines than this start collapsed to one line.
const COLLAPSE_LINES: usize = 1500;
/// The file list on the left.
const FILES_W: f32 = 260.0;
/// The line-number gutters, old and new.
const NUMBER_W: f32 = 44.0;

/// One commit's diff, or every commit that closed an issue, being read or
/// shown.
pub(super) struct DiffView {
    /// The issue whose commits these are; `esc` goes back to it.
    pub(super) issue: PathBuf,
    load: Option<Job<Result<Vec<CommitDiff>, String>>>,
    diffs: Vec<CommitDiff>,
    error: Option<String>,
    rows: Vec<Row>,
    /// Large files the user expanded, by commit and file.
    expanded: HashSet<(usize, usize)>,
    /// Syntax colours of the files drawn so far, by commit and file.
    colours: HashMap<(usize, usize), FileColours>,
    pub(super) find: Option<Find>,
    /// Scroll to this offset on the next frame.
    scroll: Option<f32>,
    /// The scroll offset last frame.
    offset: f32,
}

/// One drawn row of the diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    /// The commit's hash and subject, when several are shown.
    Commit(usize),
    File(usize, usize),
    /// A binary file, or a large one not expanded.
    Collapsed(usize, usize),
    Hunk(usize, usize, usize),
    Line(usize, usize, usize, usize),
}

/// The find bar: its text, and where it matches.
#[derive(Default)]
pub(super) struct Find {
    pub(super) text: String,
    focus: bool,
    /// What `matches` were computed for.
    searched: String,
    /// Each match: its row, and its start and end in characters.
    matches: Vec<(usize, usize, usize)>,
    current: usize,
}

impl DiffView {
    /// Starts reading `hashes`, newest first, in the repository around `dir`.
    pub(super) fn open(issue: PathBuf, dir: PathBuf, hashes: Vec<String>) -> Self {
        Self {
            issue,
            load: Some(Job::spawn(move |cancel| diff::show(&dir, &hashes, cancel))),
            diffs: Vec::new(),
            error: None,
            rows: Vec::new(),
            expanded: HashSet::new(),
            colours: HashMap::new(),
            find: None,
            scroll: None,
            offset: 0.0,
        }
    }

    /// Adopts the diff once `git` answered; true while it is still running.
    pub(super) fn poll(&mut self) -> bool {
        let Some(answer) = self.load.as_ref().and_then(Job::answer) else {
            return self.load.is_some();
        };
        self.load = None;
        match answer {
            Ok(diffs) => {
                self.diffs = diffs;
                self.build_rows();
            }
            Err(err) => self.error = Some(err),
        }
        false
    }

    fn file(&self, commit: usize, file: usize) -> &FileDiff {
        &self.diffs[commit].files[file]
    }

    fn line(&self, row: Row) -> Option<&Line> {
        match row {
            Row::Line(c, f, h, l) => Some(&self.diffs[c].files[f].hunks[h].lines[l]),
            _ => None,
        }
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        let several = self.diffs.len() > 1;
        for (c, commit) in self.diffs.iter().enumerate() {
            if several {
                rows.push(Row::Commit(c));
            }
            for (f, file) in commit.files.iter().enumerate() {
                rows.push(Row::File(c, f));
                let large = file.line_count() > COLLAPSE_LINES && !self.expanded.contains(&(c, f));
                if file.binary || large {
                    rows.push(Row::Collapsed(c, f));
                    continue;
                }
                for (h, hunk) in file.hunks.iter().enumerate() {
                    rows.push(Row::Hunk(c, f, h));
                    rows.extend((0..hunk.lines.len()).map(|l| Row::Line(c, f, h, l)));
                }
            }
        }
        self.rows = rows;
        if let Some(find) = self.find.as_mut() {
            find.searched = String::from('\u{0}');
        }
    }

    /// The first row on screen.
    fn top(&self) -> usize {
        (self.offset / ROW_H).floor() as usize
    }

    fn scroll_to_row(&mut self, row: usize) {
        self.scroll = Some(row as f32 * ROW_H);
    }

    /// Scrolls to the next (or previous) row that `is` picks.
    fn jump(&mut self, forward: bool, is: impl Fn(&Row) -> bool) {
        let top = self.top();
        let found = if forward {
            self.rows
                .iter()
                .enumerate()
                .find(|(at, row)| *at > top && is(row))
        } else {
            self.rows
                .iter()
                .enumerate()
                .rev()
                .find(|(at, row)| *at < top && is(row))
        };
        if let Some((at, _)) = found {
            self.scroll_to_row(at);
        }
    }

    /// The keys of the Diff screen while no text field has focus: files with
    /// `n p → ←`, hunks with `] [ ⌥↓ ⌥↑`, lines with `↑ ↓`.
    pub(super) fn keys(&mut self, ctx: &egui::Context) {
        let pressed =
            |key: Key, modifiers: egui::Modifiers| ctx.input_mut(|i| i.consume_key(modifiers, key));
        let plain = egui::Modifiers::NONE;
        let is_file = |row: &Row| matches!(row, Row::File(..));
        let is_hunk = |row: &Row| matches!(row, Row::Hunk(..));
        if pressed(Key::N, plain) || pressed(Key::ArrowRight, plain) {
            self.jump(true, is_file);
        }
        if pressed(Key::P, plain) || pressed(Key::ArrowLeft, plain) {
            self.jump(false, is_file);
        }
        if pressed(Key::CloseBracket, plain) || pressed(Key::ArrowDown, egui::Modifiers::ALT) {
            self.jump(true, is_hunk);
        }
        if pressed(Key::OpenBracket, plain) || pressed(Key::ArrowUp, egui::Modifiers::ALT) {
            self.jump(false, is_hunk);
        }
        if pressed(Key::ArrowDown, plain) {
            self.scroll = Some(self.offset + ROW_H);
        }
        if pressed(Key::ArrowUp, plain) {
            self.scroll = Some((self.offset - ROW_H).max(0.0));
        }
    }

    /// `⌘F`: the find bar, focused.
    pub(super) fn open_find(&mut self) {
        let find = self.find.get_or_insert_with(Find::default);
        find.focus = true;
    }

    /// Finds the bar's text again when it changed: every line holding it,
    /// ignoring case.
    fn search(&mut self) {
        let Some(find) = self.find.as_ref() else {
            return;
        };
        if find.searched == find.text {
            return;
        }
        let needle = find.text.to_lowercase();
        let mut matches = Vec::new();
        if !needle.is_empty() {
            for (at, row) in self.rows.iter().enumerate() {
                let Some(line) = self.line(*row) else {
                    continue;
                };
                let hay = line.text.to_lowercase();
                for (start, _) in hay.match_indices(&needle) {
                    let from = hay[..start].chars().count();
                    matches.push((at, from, from + needle.chars().count()));
                }
            }
        }
        if let Some(find) = self.find.as_mut() {
            find.searched = find.text.clone();
            find.matches = matches;
            find.current = 0;
        }
        self.show_match();
    }

    /// Steps to the next (or previous) match and scrolls it into view.
    fn step_match(&mut self, forward: bool) {
        let Some(find) = self.find.as_mut().filter(|find| !find.matches.is_empty()) else {
            return;
        };
        let count = find.matches.len();
        find.current = if forward {
            (find.current + 1) % count
        } else {
            (find.current + count - 1) % count
        };
        self.show_match();
    }

    fn show_match(&mut self) {
        let row = self
            .find
            .as_ref()
            .and_then(|find| find.matches.get(find.current))
            .map(|(row, ..)| *row);
        if let Some(row) = row {
            // A few rows of context above the match.
            self.scroll_to_row(row.saturating_sub(3));
        }
    }

    /// `3/12`, or `0/0` with nothing found.
    pub(super) fn match_count(&self) -> Option<String> {
        let find = self.find.as_ref()?;
        let at = if find.matches.is_empty() {
            0
        } else {
            find.current + 1
        };
        Some(format!("{at}/{}", find.matches.len()))
    }

    /// What the status bar says: the commits and their size.
    pub(super) fn summary(&self) -> String {
        if self.load.is_some() {
            return "Reading the diff…".to_string();
        }
        let files: usize = self.diffs.iter().map(|diff| diff.files.len()).sum();
        let (added, removed) = self
            .diffs
            .iter()
            .flat_map(|diff| &diff.files)
            .fold((0, 0), |(a, r), file| (a + file.added, r + file.removed));
        let noun = if files == 1 { "file" } else { "files" };
        let hashes: Vec<&str> = self.diffs.iter().map(|diff| diff.hash.as_str()).collect();
        format!(
            "{} · {files} {noun} · +{added} −{removed}",
            hashes.join(", ")
        )
    }
}

impl IboardApp {
    /// Opens the Diff screen on `hashes`, the commits of `issue`.
    pub(super) fn open_diff(&mut self, issue: &Path, hashes: Vec<String>) {
        let dir = issue.parent().unwrap_or(&self.board.root).to_path_buf();
        self.session.commits = None;
        self.session.diff = Some(DiffView::open(issue.to_path_buf(), dir, hashes));
        self.session.screen = Screen::Diff;
    }

    /// Leaves the Diff screen for the issue it came from.
    pub(super) fn close_diff(&mut self) {
        let issue = self.session.diff.take().map(|view| view.issue);
        self.session.screen = match issue {
            Some(path) => Screen::Detail(path),
            None => Screen::Board,
        };
    }

    pub(super) fn diff_ui(&mut self, ui: &mut egui::Ui) {
        let Some(view) = self.session.diff.as_mut() else {
            return;
        };
        if view.poll() {
            ui.ctx().request_repaint_after(job::POLL_EVERY);
        }
        let p = theme::palette(ui);
        Frame::new()
            .inner_margin(Margin::same(theme::SPACE_4 as i8))
            .show(ui, |ui| {
                if let Some(error) = &view.error {
                    ui.label(RichText::new(error).color(p.danger));
                    return;
                }
                if view.load.is_some() {
                    ui.label(RichText::new("Reading the diff…").color(p.ink_muted));
                    return;
                }
                if view.find.is_some() {
                    find_bar(ui, view);
                    ui.add_space(theme::SPACE_3);
                }
                ui.horizontal_top(|ui| {
                    file_list(ui, view);
                    ui.separator();
                    // A scroll area takes its parent's direction: the rows
                    // need a vertical one.
                    ui.vertical(|ui| diff_rows(ui, view));
                });
            });
    }
}

/// `⌘F`'s bar: the field, `3/12`, and `↩ ⇧↩` to step through the matches.
fn find_bar(ui: &mut egui::Ui, view: &mut DiffView) {
    let p = theme::palette(ui);
    let id = egui::Id::new("diff-find");
    let count = view.match_count().unwrap_or_default();
    let Some(find) = view.find.as_mut() else {
        return;
    };
    let mut step = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let field = widgets::text_entry(
            ui,
            egui::TextEdit::singleline(&mut find.text)
                .id(id)
                .hint_text(RichText::new("Find in the diff").color(p.ink_faint))
                .font(egui::FontId::monospace(12.0))
                .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
                .min_size(Vec2::new(0.0, theme::CONTROL_H))
                .desired_width(320.0),
        );
        if std::mem::take(&mut find.focus) {
            field.request_focus();
        }
        // `↩` leaves a single-line field: step, and keep the focus.
        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            step = Some(!ui.input(|i| i.modifiers.shift));
            field.request_focus();
        }
        ui.label(RichText::new(&count).font(theme::meta()).color(p.ink_muted));
        widgets::hint(ui, "↩", "next");
        widgets::hint(ui, "⇧↩", "previous");
    });
    view.search();
    if let Some(forward) = step {
        view.step_match(forward);
    }
}

/// The changed files, with their `+n −m`; a click scrolls to one.
fn file_list(ui: &mut egui::Ui, view: &mut DiffView) {
    let p = theme::palette(ui);
    let mut target = None;
    ui.allocate_ui_with_layout(
        Vec2::new(FILES_W, ui.available_height()),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_width(FILES_W);
            theme::scroll_area(
                ui,
                egui::ScrollArea::vertical()
                    .id_salt("diff-files")
                    .auto_shrink([false, false]),
                true,
                |ui| {
                    for (at, row) in view.rows.iter().enumerate() {
                        let Row::File(c, f) = *row else {
                            continue;
                        };
                        let file = view.file(c, f);
                        let response = ui
                            .horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(file.path())
                                            .font(egui::FontId::monospace(11.0))
                                            .color(p.ink),
                                    )
                                    .truncate()
                                    .sense(Sense::click()),
                                )
                                .on_hover_text(file.path())
                                .union(counts(ui, file))
                            })
                            .inner;
                        if response.clicked() {
                            target = Some(at);
                        }
                    }
                },
            );
        },
    );
    if let Some(at) = target {
        view.scroll_to_row(at);
    }
}

/// A file's `+n −m`, in `status-done` and `danger`.
fn counts(ui: &mut egui::Ui, file: &FileDiff) -> egui::Response {
    let p = theme::palette(ui);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(
            RichText::new(format!("−{}", file.removed))
                .font(theme::meta())
                .color(p.danger),
        );
        ui.label(
            RichText::new(format!("+{}", file.added))
                .font(theme::meta())
                .color(p.status_done),
        )
    })
    .inner
}

/// The diff itself, one `ROW_H` row at a time, only the visible ones drawn.
fn diff_rows(ui: &mut egui::Ui, view: &mut DiffView) {
    let mut area = egui::ScrollArea::vertical()
        .id_salt("diff-rows")
        .auto_shrink([false, false]);
    if let Some(offset) = view.scroll.take() {
        area = area.vertical_scroll_offset(offset);
    }
    let dark = ui.visuals().dark_mode;
    let mut expand = None;
    let output = theme::scroll_rows(ui, area, true, ROW_H, view.rows.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for at in range {
            let row = view.rows[at];
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
            match row {
                Row::Commit(c) => commit_row(ui, rect, &view.diffs[c]),
                Row::File(c, f) => file_row(ui, rect, view.file(c, f)),
                Row::Collapsed(c, f) => {
                    let file = view.file(c, f);
                    collapsed_row(ui, rect, file, &response);
                    if !file.binary && response.clicked() {
                        expand = Some((c, f));
                    }
                }
                Row::Hunk(c, f, h) => hunk_row(ui, rect, &view.file(c, f).hunks[h].header),
                Row::Line(c, f, h, l) => {
                    let marks = row_matches(view.find.as_ref(), at);
                    if view
                        .colours
                        .get(&(c, f))
                        .is_none_or(|known| known.dark != dark)
                    {
                        let colours = diff_syntax::colour_file(view.file(c, f), dark);
                        view.colours.insert((c, f), colours);
                    }
                    let spans = view
                        .colours
                        .get(&(c, f))
                        .and_then(|colours| colours.hunks.get(h)?.get(l));
                    if let Some(line) = view.line(row) {
                        line_row(ui, rect, line, spans.map_or(&[], Vec::as_slice), &marks);
                    }
                }
            }
        }
    });
    view.offset = output.state.offset.y;
    if let Some(key) = expand {
        view.expanded.insert(key);
        view.build_rows();
    }
}

/// The matches of `find` on row `at`: their character ranges, and whether
/// each is the current one.
fn row_matches(find: Option<&Find>, at: usize) -> Vec<(usize, usize, bool)> {
    let Some(find) = find else {
        return Vec::new();
    };
    find.matches
        .iter()
        .enumerate()
        .filter(|(_, (row, ..))| *row == at)
        .map(|(index, (_, from, to))| (*from, *to, index == find.current))
        .collect()
}

fn row_text(ui: &egui::Ui, rect: Rect, x: f32, text: &str, font: egui::FontId, color: Color32) {
    ui.painter().text(
        Pos2::new(rect.min.x + x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        font,
        color,
    );
}

fn commit_row(ui: &egui::Ui, rect: Rect, diff: &CommitDiff) {
    let p = theme::palette(ui);
    ui.painter().rect_filled(rect, 0.0, p.accent_soft);
    let text = format!("{}  {}", diff.hash, diff.subject);
    row_text(
        ui,
        rect,
        theme::SPACE_3,
        &text,
        theme::ui_strong(),
        p.accent,
    );
}

fn file_row(ui: &egui::Ui, rect: Rect, file: &FileDiff) {
    let p = theme::palette(ui);
    ui.painter().rect_filled(rect, 0.0, p.surface);
    ui.painter()
        .hline(rect.x_range(), rect.max.y - 0.5, Stroke::new(1.0, p.line));
    let status = match file.status() {
        FileStatus::Added => "added",
        FileStatus::Deleted => "deleted",
        FileStatus::Renamed => "renamed",
        FileStatus::Modified => "",
    };
    let path = match (file.status(), &file.old_path) {
        (FileStatus::Renamed, Some(old)) => format!("{old} → {}", file.path()),
        _ => file.path().to_string(),
    };
    let counts = format!("+{} −{}  {status}", file.added, file.removed);
    row_text(ui, rect, theme::SPACE_3, &path, theme::ui_strong(), p.ink);
    ui.painter().text(
        Pos2::new(rect.max.x - theme::SPACE_3, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        counts,
        theme::meta(),
        p.ink_muted,
    );
}

fn collapsed_row(ui: &egui::Ui, rect: Rect, file: &FileDiff, response: &egui::Response) {
    let p = theme::palette(ui);
    ui.painter().rect_filled(rect, 0.0, p.surface_sunken);
    let text = if file.binary {
        "Binary file, not shown".to_string()
    } else {
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        format!("{} diff lines, collapsed: click to show", file.line_count())
    };
    row_text(
        ui,
        rect,
        NUMBER_W * 2.0,
        &text,
        theme::ui_small(),
        p.ink_muted,
    );
}

fn hunk_row(ui: &egui::Ui, rect: Rect, header: &str) {
    let p = theme::palette(ui);
    ui.painter().rect_filled(rect, 0.0, p.surface_sunken);
    let font = egui::FontId::monospace(12.0);
    row_text(ui, rect, NUMBER_W * 2.0, header, font, p.ink_muted);
}

/// One line: its old and new numbers, its sign, and its text in its
/// syntax's colours over the added or removed tint; find matches marked.
fn line_row(
    ui: &egui::Ui,
    rect: Rect,
    line: &Line,
    spans: &[Span],
    marks: &[(usize, usize, bool)],
) {
    let p = theme::palette(ui);
    let (tint, sign) = match line.kind {
        LineKind::Added => (Some(p.status_done.gamma_multiply(0.16)), "+"),
        LineKind::Removed => (Some(p.danger_soft), "−"),
        LineKind::Context | LineKind::NoNewline => (None, ""),
    };
    if let Some(tint) = tint {
        ui.painter().rect_filled(rect, 0.0, tint);
    }
    let number = egui::FontId::monospace(11.0);
    let numbers = [(line.old, 0.0), (line.new, NUMBER_W)];
    for (value, x) in numbers {
        if let Some(value) = value {
            ui.painter().text(
                Pos2::new(rect.min.x + x + NUMBER_W - theme::SPACE_2, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                value.to_string(),
                number.clone(),
                p.ink_faint,
            );
        }
    }
    let left = rect.min.x + 2.0 * NUMBER_W;
    row_text(ui, rect, 2.0 * NUMBER_W + 2.0, sign, number, p.ink_muted);
    let text_x = left + theme::SPACE_4;
    if line.kind == LineKind::NoNewline {
        row_text(
            ui,
            rect,
            text_x - rect.min.x,
            &line.text,
            theme::ui_small(),
            p.ink_faint,
        );
        return;
    }
    let job = coloured(&line.text, spans, p.ink);
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let top = rect.center().y - galley.size().y / 2.0;
    for (from, to, current) in marks {
        let start = galley.pos_from_cursor(egui::text::CCursor::new(*from));
        let end = galley.pos_from_cursor(egui::text::CCursor::new(*to));
        let mark = Rect::from_min_max(
            Pos2::new(text_x + start.min.x, rect.min.y + 1.0),
            Pos2::new(text_x + end.min.x, rect.max.y - 1.0),
        );
        let color = if *current {
            p.accent.gamma_multiply(0.45)
        } else {
            p.status_doing.gamma_multiply(0.35)
        };
        ui.painter().rect_filled(mark, 2.0, color);
    }
    ui.painter().galley(Pos2::new(text_x, top), galley, p.ink);
}

/// `text` in its syntax colours, or in `ink` where they are missing.
fn coloured(text: &str, spans: &[Span], ink: Color32) -> LayoutJob {
    let font = egui::FontId::monospace(12.0);
    let mut job = LayoutJob::default();
    let covers = spans.first().is_some_and(|span| span.bytes.start == 0)
        && spans
            .last()
            .is_some_and(|span| span.bytes.end == text.len())
        && spans
            .windows(2)
            .all(|pair| pair[0].bytes.end == pair[1].bytes.start);
    if !covers {
        job.append(text, 0.0, TextFormat::simple(font, ink));
        return job;
    }
    for span in spans {
        let format = TextFormat {
            font_id: font.clone(),
            color: span.color,
            italics: span.italics,
            ..TextFormat::default()
        };
        job.append(text.get(span.bytes.clone()).unwrap_or(""), 0.0, format);
    }
    job
}
