//! The reader's chrome: the sheet's page and labels, its edit outline and rest
//! hint, and the "Not written yet" block docked under it.

use super::{
    Align, ButtonKind, Color32, CornerRadius, Frame, Icon, Labels, Layout, Pos2, Rect, RichText,
    Sense, Stroke, StrokeKind, Vec2, sheet, theme, widgets,
};

/// Hovering the text shows that it is one edit target: a `line-strong`
/// outline with the hint on its top edge.
pub(super) fn paint_edit_outline(ui: &egui::Ui, outline: Rect) {
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

/// The labels, on their own row above the title. Returns the one clicked, to
/// filter the board by.
pub(super) fn label_row(
    ui: &mut egui::Ui,
    sheet: &super::SheetView,
    labels: &Labels,
) -> Option<String> {
    let names = &sheet.labels;
    if names.is_empty() {
        return None;
    }
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_2);
        for name in names.iter().filter(|name| !name.starts_with("group:")) {
            let shown = name.strip_prefix("repo:").unwrap_or(name);
            let active = sheet.active_labels.contains(name);
            if widgets::label_pill_lit(ui, shown, labels.style(name), active)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!("Filter by {name}"))
                .clicked()
            {
                clicked = Some(name.clone());
            }
        }
    });
    ui.add_space(LABELS_BELOW - ui.spacing().item_spacing.y);
    clicked
}

/// Between the labels and the title.
const LABELS_BELOW: f32 = 10.0;

/// The reader's two blocks: the sheet, and under it the "Not written yet"
/// block, docked at the bottom of the window. The sheet takes the spare
/// height; the block stays put while a long issue scrolls.
pub(super) struct ReaderLayout {
    full: Rect,
    page_left: f32,
    page_width: f32,
    tiles_across: usize,
    none_written: bool,
    block_height: f32,
    pub(super) scroll_height: f32,
    /// The sheet fills its share between the header and the block.
    pub(super) sheet_height: f32,
}

impl ReaderLayout {
    pub(super) fn new(full: Rect, missing: usize, none_written: bool) -> Self {
        let gutter = theme::SPACE_5;
        let page_width = (full.width() - 2.0 * gutter).min(theme::READER_MAX);
        let page_left = full.min.x + ((full.width() - page_width) / 2.0).max(gutter);
        let tiles_across = GHOST_TILES_ACROSS.min(missing.max(1));
        let block_height = if none_written {
            let rows = missing.div_ceil(tiles_across) as f32;
            2.0 * theme::SPACE_4
                + theme::CONTROL_H
                + theme::SPACE_3
                + rows * ghost_tile_height(page_width - 2.0 * gutter)
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
    pub(super) fn not_written(
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

/// Height of a ghost tile in the "Not written yet" block.
const GHOST_TILE_H: f32 = 72.0;
/// A tile whose hint fits on one line.
const GHOST_TILE_ONE_LINE_H: f32 = 56.0;
/// Four would be narrower than "Acceptance criteria", even on a full sheet.
const GHOST_TILES_ACROSS: usize = 2;

/// How tall the tiles of a block `width` across are.
fn ghost_tile_height(width: f32) -> f32 {
    if width >= 500.0 {
        GHOST_TILE_ONE_LINE_H
    } else {
        GHOST_TILE_H
    }
}

/// What the "Not written yet" block asked for.
pub(super) enum NotWritten {
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
    let tile_h = ghost_tile_height(inner.width());
    let top = ui.cursor().min.y;
    for (index, section) in missing.iter().enumerate() {
        let (row, col) = (index / across, index % across);
        let tile = Rect::from_min_size(
            Pos2::new(
                inner.min.x + col as f32 * (tile_w + gap),
                top + row as f32 * (tile_h + gap),
            ),
            Vec2::new(tile_w, tile_h),
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

/// The sheet's edge, inside its 32pt padding.
const SHEET_EDGE: f32 = 1.0;

/// The reader's `surface` page: `reader-max` wide, centred, 32pt in from its
/// edge to a `read-measure` text column.
pub(super) fn page<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    page_column(ui, |ui| {
        let p = theme::palette(ui);
        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(SHEET_EDGE, p.line_soft))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(theme::SPACE_7 - SHEET_EDGE)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add_contents(ui)
            })
    })
}

/// Lays `add_contents` out in a column `reader-max` wide, centred, with the
/// window gutter around it.
pub(in crate::shell::app) fn page_column<R>(
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

/// A sheet head's column: its status dot and its name in capitals.
pub(in crate::shell::app) fn column_mark(ui: &mut egui::Ui, column: &str, status: Color32) {
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
pub(in crate::shell::app) fn sheet_rule(ui: &mut egui::Ui) {
    let line = theme::palette(ui).line;
    let (rule, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(rule.x_range(), rule.center().y, Stroke::new(1.0, line));
}

/// At the foot of the empty rest of the sheet.
pub(super) const REST_HINT: &str = "Double-click below the text to keep writing";

/// [`REST_HINT`] in `ink-faint`, with its key `E` while ⌘ is held.
pub(super) fn paint_rest_hint(ui: &egui::Ui, rest: Rect) {
    let p = theme::palette(ui);
    let text = egui::WidgetText::from(
        RichText::new(REST_HINT)
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
