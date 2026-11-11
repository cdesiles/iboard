//! A column as a drop target: the line where a dragged card would land, the
//! highlight of the column it is over, and the well of an empty column.

use super::{
    Color32, CornerRadius, Frame, PathBuf, Pos2, Rect, RichText, Stroke, StrokeKind, Vec2,
    dashed_rect, theme,
};

/// Where a card dropped at `pointer_y` lands among `cards`: before the first
/// card whose middle is below the pointer, else last.
fn insert_index(pointer_y: Option<f32>, cards: &[(PathBuf, Rect)]) -> usize {
    pointer_y
        .and_then(|y| cards.iter().position(|(_, rect)| y < rect.center().y))
        .unwrap_or(cards.len())
}

/// Draws the `accent` line where the dragged card would land and returns the
/// card it lands in front of.
pub(super) fn drop_line(ui: &egui::Ui, cards: &[(PathBuf, Rect)]) -> Option<PathBuf> {
    let p = theme::palette(ui);
    let pointer = ui
        .ctx()
        .pointer_interact_pos()
        .or(ui.ctx().pointer_hover_pos());
    let insert = insert_index(pointer.map(|pos| pos.y), cards);
    let line_y = cards
        .get(insert)
        .map(|(_, rect)| rect.min.y - theme::SPACE_3 / 2.0)
        .or_else(|| {
            cards
                .last()
                .map(|(_, rect)| rect.max.y + theme::SPACE_3 / 2.0)
        });
    if let (Some(y), Some((_, first))) = (line_y, cards.first()) {
        ui.painter().line_segment(
            [Pos2::new(first.min.x, y), Pos2::new(first.max.x, y)],
            Stroke::new(2.0, p.accent),
        );
    }
    cards.get(insert).map(|(path, _)| path.clone())
}

/// Highlights the column's cards area, under its header, as a drop target.
pub(super) fn highlight_drop(ui: &egui::Ui, drop_rect: Rect) {
    let p = theme::palette(ui);
    let rect = drop_rect
        .with_min_y(drop_rect.min.y + theme::CONTROL_H + theme::SPACE_3 / 2.0)
        .expand2(Vec2::new(theme::SPACE_2, 0.0))
        .with_max_y(drop_rect.max.y + theme::SPACE_2);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::RADIUS_MD),
        p.accent_soft.gamma_multiply(0.5),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::RADIUS_MD),
        Stroke::new(2.0, p.accent),
        StrokeKind::Inside,
    );
}

/// The dashed well of an empty column, with no fill. It is also where a
/// dragged card lands: while one is `over` it, solid `accent` on `accent-soft`.
pub(super) fn empty_well(
    ui: &mut egui::Ui,
    message: &str,
    hint: Option<&str>,
    width: f32,
    over: bool,
) {
    let p = theme::palette(ui);
    let corner = CornerRadius::same(theme::RADIUS_MD);
    let response = Frame::new()
        .fill(if over {
            p.accent_soft
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(corner)
        .inner_margin(theme::SPACE_4)
        .show(ui, |ui| {
            ui.set_width(width - 2.0 * theme::SPACE_4);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(message)
                        .font(theme::ui_small())
                        .color(p.ink_faint),
                );
                if let Some(hint) = hint {
                    ui.label(
                        RichText::new(hint)
                            .font(theme::ui_small())
                            .color(p.ink_faint),
                    );
                }
            });
        })
        .response;
    if over {
        let edge = Stroke::new(1.0, p.accent);
        ui.painter()
            .rect_stroke(response.rect, corner, edge, StrokeKind::Inside);
    } else {
        dashed_rect(ui.painter(), response.rect, theme::RADIUS_MD, p.line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards() -> Vec<(PathBuf, Rect)> {
        [0.0, 100.0]
            .map(|top| {
                let rect = Rect::from_min_size(Pos2::new(0.0, top), Vec2::new(200.0, 80.0));
                (PathBuf::from(format!("{top}.md")), rect)
            })
            .to_vec()
    }

    #[test]
    fn a_drop_lands_before_the_first_card_whose_middle_is_below_the_pointer() {
        let cards = cards();
        assert_eq!(insert_index(Some(10.0), &cards), 0);
        assert_eq!(insert_index(Some(60.0), &cards), 1);
        assert_eq!(insert_index(Some(139.0), &cards), 1);
    }

    #[test]
    fn a_drop_lands_last_below_the_cards_or_without_a_pointer() {
        let cards = cards();
        assert_eq!(insert_index(Some(170.0), &cards), 2);
        assert_eq!(insert_index(None, &cards), 2);
        assert_eq!(insert_index(Some(10.0), &[]), 0);
    }
}
