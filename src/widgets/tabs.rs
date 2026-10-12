//! The column tabs: a column's name and count, with its underline when current.

use std::sync::Arc;

use egui::{
    Color32, CornerRadius, FontId, Galley, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use super::galley;
use crate::theme;

/// One tab of the [`column_tabs`] segmented track.
pub struct ColumnTab<'a> {
    pub name: &'a str,
    pub count: usize,
    /// The column's status colour: its dot and the active underline.
    pub color: Color32,
    pub active: bool,
}

/// A galley with letter spacing, for the uppercase `ui-label` of a column tab.
fn tracked_galley(ui: &Ui, text: &str, font: FontId, color: Color32, tracking: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font,
            color,
            extra_letter_spacing: tracking,
            ..Default::default()
        },
    );
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// What [`column_tabs`] reported: the tab that was clicked, and where each tab
/// was drawn (so the caller can make them drop targets).
pub struct ColumnTabs {
    pub clicked: Option<usize>,
    pub tab_rects: Vec<Rect>,
}

/// The `surface-sunken` segmented track of ColumnFocus column tabs: each tab
/// has a status dot, its uppercase `ui-label` name and its count. The active
/// tab sits on `surface` with a 1pt `line-strong` ring and a 2pt status
/// underline.
pub fn column_tabs(ui: &mut Ui, tabs: &[ColumnTab]) -> ColumnTabs {
    let p = theme::palette(ui);
    let pad = 2.0;
    let gap = 2.0;
    let text_gap = 6.0;
    let side = 8.0;
    let dot = 8.0;
    let tab_h = 22.0;

    let names: Vec<Arc<Galley>> = tabs
        .iter()
        .map(|tab| {
            tracked_galley(
                ui,
                &tab.name.to_uppercase(),
                theme::ui_label(),
                p.ink,
                theme::LABEL_TRACKING,
            )
        })
        .collect();
    let counts: Vec<Arc<Galley>> = tabs
        .iter()
        .map(|tab| galley(ui, &tab.count.to_string(), theme::meta(), p.ink_muted))
        .collect();
    let widths: Vec<f32> = names
        .iter()
        .zip(&counts)
        .map(|(name, count)| {
            side + dot + text_gap + name.size().x + text_gap + count.size().x + side
        })
        .collect();
    let total =
        pad * 2.0 + widths.iter().sum::<f32>() + gap * (tabs.len().saturating_sub(1)) as f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total, tab_h + pad * 2.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            p.surface_sunken,
            Stroke::new(1.0, p.line_strong),
            StrokeKind::Inside,
        );
    }

    let mut clicked = None;
    let mut tab_rects = Vec::with_capacity(tabs.len());
    let mut x = rect.min.x + pad;
    for (index, tab) in tabs.iter().enumerate() {
        let tab_rect = Rect::from_min_size(
            Pos2::new(x, rect.min.y + pad),
            Vec2::new(widths[index], tab_h),
        );
        tab_rects.push(tab_rect);
        let response = ui
            .interact(
                tab_rect,
                egui::Id::new(("coltab", tab.name)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            clicked = Some(index);
        }
        if ui.is_rect_visible(tab_rect) {
            let painter = ui.painter();
            if tab.active {
                painter.rect(
                    tab_rect,
                    CornerRadius::same(3),
                    p.surface,
                    Stroke::new(1.0, p.line_strong),
                    StrokeKind::Inside,
                );
                painter.line_segment(
                    [
                        Pos2::new(tab_rect.min.x + 1.0, tab_rect.max.y - 1.0),
                        Pos2::new(tab_rect.max.x - 1.0, tab_rect.max.y - 1.0),
                    ],
                    Stroke::new(2.0, tab.color),
                );
            }
            let ink = if tab.active || response.hovered() {
                p.ink
            } else {
                p.ink_muted
            };
            let cy = tab_rect.center().y;
            painter.circle_filled(
                Pos2::new(tab_rect.min.x + side + dot / 2.0, cy),
                dot / 2.0,
                tab.color,
            );
            let name = &names[index];
            let name_x = tab_rect.min.x + side + dot + text_gap;
            painter.galley(
                Pos2::new(name_x, cy - name.size().y / 2.0),
                name.clone(),
                ink,
            );
            let count = &counts[index];
            painter.galley(
                Pos2::new(
                    tab_rect.max.x - side - count.size().x,
                    cy - count.size().y / 2.0,
                ),
                count.clone(),
                if tab.active { p.ink_muted } else { p.ink_faint },
            );
        }
        x += widths[index] + gap;
    }
    ColumnTabs { clicked, tab_rects }
}
