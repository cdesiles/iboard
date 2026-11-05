//! The Biggest files table of Project stats: the files at the page's newest
//! commit, sortable, the first few until "Show all".

use iboard::stats::FileStat;

use super::{
    Align, ButtonKind, Color32, CornerRadius, FileSort, Icon, Layout, Pos2, Rect, Sense, Vec2,
    block, block_label, theme, thousands, widgets,
};

/// The files listed before "Show all".
const FILES_SHOWN: usize = 8;

/// The files at the page's newest commit, biggest first unless sorted
/// otherwise.
pub(super) fn files_table(
    ui: &mut egui::Ui,
    files: &[FileStat],
    sort: &mut FileSort,
    all: &mut bool,
) {
    block(ui, |ui| {
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            block_label(ui, "Biggest files");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut options = FileSort::ALL;
                options.reverse();
                if let Some(picked) = widgets::segmented(ui, *sort, &options) {
                    *sort = picked;
                }
            });
        });
        let mut rows: Vec<&FileStat> = files.iter().collect();
        match sort {
            FileSort::Lines => {}
            FileSort::Complexity => rows.sort_by_key(|f| std::cmp::Reverse(f.complexity)),
            FileSort::Changes => rows.sort_by_key(|f| std::cmp::Reverse(f.changes)),
        }
        let largest = files.first().map_or(1, |f| f.code.max(1));
        let cells = ["Lines", "Complexity", "Changes", "Hotspot"];
        file_row(ui, "File", &cells.map(str::to_string), None, p.ink_muted);
        widgets::rule(ui);
        let shown = if *all { rows.len() } else { FILES_SHOWN };
        for f in rows.iter().take(shown) {
            let hot = if f.is_hotspot() { "hotspot" } else { "–" };
            let cells = [
                thousands(f.code),
                f.complexity.to_string(),
                f.changes.to_string(),
                hot.to_string(),
            ];
            let bar = f.code as f32 / largest as f32;
            file_row(ui, &f.path, &cells, Some(bar), p.ink);
        }
        if rows.len() > FILES_SHOWN && !*all {
            let label = format!("Show all {}", rows.len());
            if widgets::button(ui, ButtonKind::Ghost, None, &label, None).clicked() {
                *all = true;
            }
        }
    });
}

/// One row of the files table, painted on fixed columns: the path, then its
/// four figures; `bar` scales the lines to the largest file.
fn file_row(ui: &mut egui::Ui, path: &str, cells: &[String; 4], bar: Option<f32>, ink: Color32) {
    const WIDTHS: [f32; 4] = [130.0, 80.0, 70.0, 90.0];
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, path));
    let painter = ui.painter_at(rect);
    let font = theme::meta();
    let mut x = rect.right() - WIDTHS.iter().sum::<f32>();
    let (dir, name) = path.rsplit_once('/').map_or(("", path), |(d, n)| (d, n));
    let mut text = egui::text::LayoutJob::default();
    if !dir.is_empty() {
        text.append(
            &format!("{dir}/"),
            0.0,
            egui::TextFormat::simple(font.clone(), p.ink_faint),
        );
    }
    text.append(name, 0.0, egui::TextFormat::simple(font.clone(), ink));
    text.wrap = egui::text::TextWrapping::truncate_at_width(x - rect.left() - theme::SPACE_3);
    let galley = ui.fonts_mut(|f| f.layout_job(text));
    painter.galley(
        Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    for (at, (cell, width)) in cells.iter().zip(WIDTHS).enumerate() {
        let hot = at == 3 && cell == "hotspot";
        let mut left = x;
        if hot {
            let icon =
                Rect::from_center_size(Pos2::new(x + 6.0, rect.center().y), Vec2::splat(12.0));
            theme::paint_icon(&painter, Icon::Flame, icon, p.status_doing);
            left += 16.0;
        }
        let color = if hot { p.status_doing } else { ink };
        let shown = painter.text(
            Pos2::new(left, rect.center().y),
            egui::Align2::LEFT_CENTER,
            cell,
            font.clone(),
            color,
        );
        if let (0, Some(share)) = (at, bar) {
            let track = Rect::from_min_size(
                Pos2::new(shown.right() + theme::SPACE_3, rect.center().y - 3.0),
                Vec2::new(64.0 * share, 6.0),
            );
            painter.rect_filled(track, CornerRadius::same(2), p.accent);
        }
        x += width;
    }
}
