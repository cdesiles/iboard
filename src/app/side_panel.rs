//! The panel beside an issue's page, the review's or the agent log's: one
//! `surface` card under a `review-soft` head.

use super::{CornerRadius, Frame, RichText, Stroke, theme};

/// A `surface` card with a `line-strong` edge and `radius-md` corners: the
/// head, `title` and `detail`, then `body`.
pub(super) fn side_panel<R>(
    ui: &mut egui::Ui,
    title: &str,
    detail: &str,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_soft))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            head(ui, title, detail);
            body(ui)
        })
        .inner
}

/// The `review-soft` fill of a head at the top of a `radius-md` card: the
/// panel's, or the refinement's.
pub(super) fn head_frame(ui: &egui::Ui) -> Frame {
    Frame::new()
        .fill(theme::palette(ui).review_soft)
        // The header is flush with the panel's top edge, so its
        // fill has to round the top corners itself: egui frames
        // don't clip children, and a square fill would paint over
        // the panel's rounded corners and their border. One less
        // than RADIUS_MD matches the inside of the 1pt border.
        .corner_radius(CornerRadius {
            nw: theme::RADIUS_MD - 1,
            ne: theme::RADIUS_MD - 1,
            sw: 0,
            se: 0,
        })
}

/// The panel's head: `title` ("Review of x.md") over `detail` (the agent, mode
/// and source). Both wrap, so neither is cut and the panel never widens under
/// the issue.
fn head(ui: &mut egui::Ui, title: &str, detail: &str) {
    let p = theme::palette(ui);
    head_frame(ui).inner_margin(theme::ROW_PAD).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = theme::SPACE_1;
        let title = RichText::new(title)
            .font(theme::ui_strong())
            .color(p.review);
        ui.add(egui::Label::new(title).wrap());
        let detail = RichText::new(detail).font(theme::meta()).color(p.review);
        ui.add(egui::Label::new(detail).wrap());
    });
}
