//! The dock that rises while a card is dragged, to archive or delete it.

use super::*;

impl IboardApp {
    /// The DropZones dock: while a card is dragged, Delete (or, for Done
    /// cards, Archive) slides up over the bottom of the card's column, above
    /// the status bar. In ColumnFocus the column is the whole window.
    pub(super) fn drop_dock(&mut self, ui: &egui::Ui) {
        let ctx = ui.ctx().clone();
        let drag = egui::DragAndDrop::payload::<IssueDrag>(&ctx);
        let dragged_column = self.dragged_column.take();
        if let Some(drag) = &drag {
            self.dock_archivable = drag.archivable;
            self.dock_span = dragged_column;
        }
        let slide = theme::motion(&ctx, DOCK_SLIDE);
        let shown = ctx.animate_bool_with_time(egui::Id::new("drop_dock"), drag.is_some(), slide);
        if shown <= 0.0 {
            return;
        }
        let p = theme::palette(ui);
        let area = ui.max_rect();
        let top = area.max.y - DOCK_H * shown;
        let (x, margin) = match self.dock_span {
            Some(span) if span.intersection(area.x_range()).span() > 0.0 => {
                (span.intersection(area.x_range()), 0.0)
            }
            _ => (area.x_range(), theme::SPACE_5),
        };
        let dock = Rect::from_x_y_ranges(x, egui::Rangef::new(top, top + DOCK_H));
        let archivable = self.dock_archivable;
        let mut dropped = None;
        egui::Area::new(egui::Id::new("drop_dock_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(dock.min)
            .movable(false)
            .constrain(false)
            .show(&ctx, |ui| {
                ui.set_clip_rect(area);
                let painter = ui.painter();
                painter.rect_filled(dock, CornerRadius::ZERO, p.bg);
                painter.hline(dock.x_range(), dock.min.y + 0.5, Stroke::new(1.0, p.line));
                // Swallow the pointer over the whole band, so a drop beside
                // the zone never lands on the column underneath.
                ui.interact(dock, egui::Id::new("drop_dock_band"), Sense::hover());

                let inner = Rect::from_min_max(
                    dock.min + Vec2::new(margin, 1.0 + theme::SPACE_4),
                    dock.max - Vec2::new(margin, theme::SPACE_4),
                );
                // Done cards are archived, never deleted; the others are
                // deleted, never archived.
                let zone = if archivable {
                    DropZone::Archive
                } else {
                    DropZone::Delete
                };
                if let Some(path) = drop_zone(ui, inner, zone, self.confirm_delete) {
                    dropped = Some(match zone {
                        DropZone::Archive => DockDrop::Archive(path),
                        DropZone::Delete => DockDrop::Delete(path),
                    });
                }
            });
        if shown < 1.0 {
            ctx.request_repaint();
        }
        match dropped {
            Some(DockDrop::Archive(path)) => self.archive_issue(&path),
            Some(DockDrop::Delete(path)) => self.request_delete(path),
            None => {}
        }
    }
}

/// Draws a drop zone and returns the dragged issue if it was dropped here:
/// dashed at rest, a solid 2pt edge on a soft fill while a card is over it.
/// Every zone drawn accepts the drop; the dock simply does not draw the zones
/// that would refuse it.
pub(super) fn drop_zone(
    ui: &mut egui::Ui,
    rect: Rect,
    zone: DropZone,
    asks: bool,
) -> Option<PathBuf> {
    let p = theme::palette(ui);
    let id = match zone {
        DropZone::Archive => "drop_zone_archive",
        DropZone::Delete => "drop_zone_delete",
    };
    let response = ui.interact(rect, egui::Id::new(id), Sense::hover());
    let over = response.dnd_hover_payload::<IssueDrag>().is_some();
    let (icon, word, hint) = match zone {
        DropZone::Archive => (Icon::Archive, "Archive", "moves to .archive/"),
        DropZone::Delete if asks => (Icon::Trash, "Delete", "asks to confirm"),
        DropZone::Delete => (Icon::Trash, "Delete", "moves to the Trash"),
    };
    let radius = CornerRadius::same(theme::RADIUS_MD);
    let painter = ui.painter();
    let ink = match zone {
        DropZone::Archive if over => {
            painter.rect(
                rect,
                radius,
                p.accent_soft,
                Stroke::new(2.0, p.accent),
                StrokeKind::Inside,
            );
            p.accent
        }
        DropZone::Archive => {
            painter.rect_filled(rect, radius, p.surface_sunken);
            dashed_rect(painter, rect, p.line_strong);
            p.ink_muted
        }
        DropZone::Delete if over => {
            painter.rect(
                rect,
                radius,
                p.danger_soft,
                Stroke::new(2.0, p.danger),
                StrokeKind::Inside,
            );
            p.danger
        }
        DropZone::Delete => {
            painter.rect_filled(rect, radius, p.surface_sunken);
            dashed_rect(painter, rect, p.danger);
            p.danger
        }
    };

    // An 18pt icon, the word and the hint, centred together.
    let word = painter.layout_no_wrap(word.to_owned(), theme::ui_strong(), ink);
    let hint = painter.layout_no_wrap(hint.to_owned(), theme::meta(), p.ink_muted);
    let icon_size = 18.0;
    let gap = theme::SPACE_3;
    let total = icon_size + gap + word.size().x + gap + hint.size().x;
    let mut x = rect.center().x - total / 2.0;
    let y = rect.center().y;
    theme::paint_icon(
        painter,
        icon,
        Rect::from_min_size(Pos2::new(x, y - icon_size / 2.0), Vec2::splat(icon_size)),
        ink,
    );
    x += icon_size + gap;
    let word_width = word.size().x;
    painter.galley(Pos2::new(x, y - word.size().y / 2.0), word, ink);
    x += word_width + gap;
    painter.galley(Pos2::new(x, y - hint.size().y / 2.0), hint, p.ink_muted);

    response
        .dnd_release_payload::<IssueDrag>()
        .map(|drag| drag.path.clone())
}
