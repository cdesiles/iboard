//! The Biggest files table of Project stats: the files at the page's newest
//! commit, sortable, the first few until "Show all".

use std::path::Path;

use crate::commits::{self, Commit};
use crate::stats::FileStat;

use super::blocks::{Blocks, Part, clock_note, framed, list_rows, panel_head};
use super::{
    Align, ButtonKind, Color32, CornerRadius, Duration, FileSort, Icon, Job, Layout, Pos2, Rect,
    RichText, Sense, Stroke, Vec2, theme, thousands, widgets,
};

/// The files listed before "Show all".
const FILES_SHOWN: usize = 8;
/// The block's height: room for those rows and "Show all", so nothing moves
/// as it fills.
const FILES_H: f32 = 340.0;
/// A row and the head above them.
const ROW_H: f32 = 28.0;
const HEAD_H: f32 = 22.0;
/// The figure columns, right-aligned against the end of each.
const COLUMNS: [f32; 4] = [150.0, 96.0, 84.0, 96.0];
/// The columns' names, and the sort each head asks for.
const HEADS: [(&str, Option<FileSort>); 4] = [
    ("Lines", Some(FileSort::Lines)),
    ("Complexity", Some(FileSort::Complexity)),
    ("Changes", Some(FileSort::Changes)),
    ("Hotspot", None),
];
/// In the Lines column, where its number ends and how wide its bar can be.
const LINES_NUMBER: f32 = 58.0;
const LINES_BAR: f32 = 80.0;
/// The popup's width and its list's tallest.
const HISTORY_W: f32 = 520.0;
const HISTORY_H: f32 = 320.0;

const NAME: &str = "Biggest files";

/// The files at the page's newest commit, biggest first unless sorted
/// otherwise, on their own job. Returns the path of a row clicked.
pub(super) fn files_table(
    ui: &mut egui::Ui,
    blocks: &mut Blocks,
    sort: &mut FileSort,
    all: &mut bool,
) -> Option<String> {
    framed(
        ui,
        blocks,
        (Part::Files, NAME, FILES_H),
        list_rows,
        |ui, stats, clock| table(ui, &stats.files, sort, all, clock),
    )
    .flatten()
}

fn table(
    ui: &mut egui::Ui,
    files: &[FileStat],
    sort: &mut FileSort,
    all: &mut bool,
    clock: Option<Duration>,
) -> Option<String> {
    let mut rows: Vec<&FileStat> = files.iter().collect();
    match sort {
        FileSort::Lines => {}
        FileSort::Complexity => rows.sort_by_key(|f| std::cmp::Reverse(f.complexity)),
        FileSort::Changes => rows.sort_by_key(|f| std::cmp::Reverse(f.changes)),
    }
    let shown = if *all { rows.len() } else { FILES_SHOWN };
    let caption = format!(
        "top {} of {} · code lines, no blanks or comments",
        shown.min(rows.len()),
        rows.len()
    );
    panel_head(ui, NAME, &caption, |ui| {
        let mut options = FileSort::ALL;
        options.reverse();
        if let Some(picked) = widgets::segmented(ui, *sort, &options) {
            *sort = picked;
        }
        clock_note(ui, clock);
    });
    let largest = files.first().map_or(1, |f| f.code.max(1));
    let mut opened = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        file_head(ui, sort);
        for f in rows.iter().take(shown) {
            if file_row(ui, f, f.code as f32 / largest as f32).clicked() {
                opened = Some(f.path.clone());
            }
        }
    });
    if rows.len() > FILES_SHOWN && !*all {
        let label = format!("Show all {}", rows.len());
        if widgets::button(ui, ButtonKind::Ghost, None, &label, None).clicked() {
            *all = true;
        }
    }
    opened
}

/// The left edge of the first figure column in a row spanning `rect`.
fn columns_left(rect: Rect) -> f32 {
    rect.right() - COLUMNS.iter().sum::<f32>()
}

/// The column heads: `File`, and each figure's name right-aligned. A click on a
/// figure sorts by it; the sorted one is in `ink`, with its arrow.
fn file_head(ui: &mut egui::Ui, sort: &mut FileSort) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), HEAD_H), Sense::hover());
    let painter = ui.painter_at(rect);
    let font = theme::ui_label();
    painter.text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        "FILE",
        font.clone(),
        p.ink_muted,
    );
    let mut x = columns_left(rect);
    for ((name, by), width) in HEADS.into_iter().zip(COLUMNS) {
        let cell = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(width, HEAD_H));
        x += width;
        let mut hovered = false;
        if let Some(by) = by {
            let id = ui.scope_id().with(("files-sort", name));
            let response = ui.interact(cell, id, Sense::click());
            let label = format!("Sort by {name}");
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
            hovered = response.hovered();
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *sort = by;
            }
        }
        let sorted = by == Some(*sort);
        let text = if sorted {
            format!("{} ↓", name.to_uppercase())
        } else {
            name.to_uppercase()
        };
        painter.text(
            cell.right_center() - Vec2::new(theme::SPACE_3, 0.0),
            egui::Align2::RIGHT_CENTER,
            text,
            font.clone(),
            if sorted || hovered {
                p.ink
            } else {
                p.ink_muted
            },
        );
    }
}

/// One row of the files table under a `line` divider: the path, then its
/// figures right-aligned, the lines' bar scaled to the largest file by `share`.
/// A click asks for the file's history.
fn file_row(ui: &mut egui::Ui, file: &FileStat, share: f32) -> egui::Response {
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &file.path));
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::ZERO, p.surface_hover);
    }
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.line));
    let font = theme::meta();
    let mid = rect.center().y;
    let left = columns_left(rect);
    let (dir, name) = file
        .path
        .rsplit_once('/')
        .map_or(("", file.path.as_str()), |(d, n)| (d, n));
    let mut text = egui::text::LayoutJob::default();
    if !dir.is_empty() {
        text.append(
            &format!("{dir}/"),
            0.0,
            egui::TextFormat::simple(font.clone(), p.ink_faint),
        );
    }
    text.append(name, 0.0, egui::TextFormat::simple(font.clone(), p.ink));
    text.wrap = egui::text::TextWrapping::truncate_at_width(left - rect.left() - theme::SPACE_3);
    let galley = ui.fonts_mut(|f| f.layout_job(text));
    painter.galley(
        Pos2::new(rect.left(), mid - galley.size().y / 2.0),
        galley,
        p.ink,
    );
    painter.text(
        Pos2::new(left + LINES_NUMBER, mid),
        egui::Align2::RIGHT_CENTER,
        thousands(file.code),
        font.clone(),
        p.ink,
    );
    let bar = Rect::from_min_size(
        Pos2::new(left + LINES_NUMBER + theme::SPACE_3, mid - 4.0),
        Vec2::new((LINES_BAR * share).max(1.0), 8.0),
    );
    painter.rect_filled(bar, CornerRadius::same(2), p.accent.gamma_multiply(0.8));
    let mut x = left + COLUMNS[0];
    for (cell, width) in [file.complexity, file.changes]
        .into_iter()
        .zip(&COLUMNS[1..])
    {
        x += width;
        painter.text(
            Pos2::new(x - theme::SPACE_3, mid),
            egui::Align2::RIGHT_CENTER,
            cell.to_string(),
            font.clone(),
            p.ink,
        );
    }
    x += COLUMNS[3];
    hotspot_cell(
        &painter,
        Pos2::new(x - theme::SPACE_3, mid),
        file.is_hotspot(),
        p,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Show this file's history")
}

/// The flame and "hotspot" in `status-doing`, or a dash, ending at `end`.
fn hotspot_cell(painter: &egui::Painter, end: Pos2, hot: bool, p: &theme::Palette) {
    let font = theme::meta();
    if !hot {
        painter.text(end, egui::Align2::RIGHT_CENTER, "–", font, p.ink_faint);
        return;
    }
    let shown = painter.text(
        end,
        egui::Align2::RIGHT_CENTER,
        "hotspot",
        font,
        p.status_doing,
    );
    let icon = Rect::from_center_size(
        Pos2::new(shown.left() - theme::SPACE_3, end.y),
        Vec2::splat(12.0),
    );
    theme::paint_icon(painter, Icon::Flame, icon, p.status_doing);
}

/// A file's history popup, read from git on its own job.
pub(in crate::shell::app) struct FileHistory {
    path: String,
    read: Option<Job<Result<Vec<Commit>, String>>>,
    list: Result<Vec<Commit>, String>,
}

impl FileHistory {
    pub(super) fn open(dir: &Path, path: String) -> Self {
        let (dir, file) = (dir.to_path_buf(), path.clone());
        Self {
            path,
            read: Some(Job::spawn(move |_| commits::for_path(&dir, &file))),
            list: Ok(Vec::new()),
        }
    }

    /// Takes the answer once it is in; true while it is still being read.
    pub(super) fn poll(&mut self) -> bool {
        let Some(read) = &self.read else {
            return false;
        };
        let Some(answer) = read.answer() else {
            return true;
        };
        self.list = answer;
        self.read = None;
        false
    }
}

/// The popup over the stats: every commit that changed the file, newest first.
/// True when it is closed, by Close, `esc` or the scrim.
pub(super) fn history_window(ui: &egui::Ui, history: &FileHistory) -> bool {
    let p = theme::palette(ui);
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::unique("file-history"))
        .frame(widgets::modal_frame(ui).inner_margin(theme::SPACE_6))
        .backdrop_color(p.scrim)
        .show(ui.ctx(), |ui| {
            ui.set_width(HISTORY_W);
            ui.style_mut().interaction.selectable_labels = false;
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            ui.label(
                RichText::new("History")
                    .font(theme::ui_title())
                    .color(p.ink),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(&history.path)
                        .font(theme::meta())
                        .color(p.ink_muted),
                )
                .truncate(),
            );
            ui.add_space(theme::SPACE_2);
            match &history.list {
                _ if history.read.is_some() => {
                    history_note(ui, "Reading the file's history…", p.ink_muted);
                }
                Err(error) => history_note(ui, error, p.danger),
                Ok(list) if list.is_empty() => {
                    history_note(ui, "No commit changed it.", p.ink_muted)
                }
                Ok(list) => history_list(ui, list),
            }
            ui.add_space(theme::SPACE_3);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                close = widgets::button(ui, ButtonKind::Secondary, None, "Close", Some("esc"))
                    .clicked();
            });
        });
    close || modal.should_close()
}

fn history_note(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).font(theme::ui_small()).color(color));
}

fn history_list(ui: &mut egui::Ui, list: &[Commit]) {
    let p = theme::palette(ui);
    theme::scroll_area(
        ui,
        egui::ScrollArea::vertical()
            .id_salt("file-history-list")
            .content_margin(theme::scroll_margin(0.0, 0.0))
            .max_height(HISTORY_H)
            .auto_shrink([false, true]),
        |ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
            for commit in list {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    ui.label(
                        RichText::new(&commit.hash)
                            .font(theme::meta())
                            .color(p.accent),
                    );
                    ui.label(
                        RichText::new(commit.day())
                            .font(theme::meta())
                            .color(p.ink_muted),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(&commit.subject)
                                .font(theme::ui_small())
                                .color(p.ink),
                        )
                        .truncate(),
                    );
                });
            }
        },
    );
}
