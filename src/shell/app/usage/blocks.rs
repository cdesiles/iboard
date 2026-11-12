//! What every block of the Usage screen shares: its card, the skeleton it
//! keeps until the log reads, and the failure with Retry that names the file.

use super::{ButtonKind, CornerRadius, Frame, RichText, Stroke, Vec2, theme, widgets};

/// Where the log read stands, as the blocks draw it.
#[derive(Clone, Debug, Default)]
pub(crate) enum Read {
    #[default]
    Loaded,
    Waiting {
        pulse: bool,
    },
    Failed(String),
}

impl Read {
    pub(super) fn failed(&self) -> Option<&str> {
        match self {
            Read::Failed(error) => Some(error),
            _ => None,
        }
    }

    pub(super) fn waiting(&self) -> Option<bool> {
        match self {
            Read::Waiting { pulse } => Some(*pulse),
            _ => None,
        }
    }
}

/// A `surface` block with a `line` edge, as every block of the pane sits.
pub(super) fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_soft))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(Vec2::new(14.0, theme::SPACE_4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme::SPACE_2 + 2.0;
            add(ui)
        })
        .inner
}

/// A block's failure: what could not be read, in `danger`, and Retry. True
/// when Retry was pressed.
pub(super) fn failure(ui: &mut egui::Ui, error: &str) -> bool {
    let p = theme::palette(ui);
    ui.add(egui::Label::new(RichText::new(error).font(theme::meta()).color(p.danger)).truncate());
    widgets::button(ui, ButtonKind::Ghost, None, "Retry", None).clicked()
}

/// A pulsing bar `share` of the line wide, or nothing before the skeleton may
/// show.
pub(super) fn placeholder(ui: &mut egui::Ui, share: f32, height: f32, pulse: bool) {
    if pulse {
        widgets::skeleton_bar(ui, share, height);
    } else {
        ui.allocate_exact_size(
            Vec2::new(ui.available_width() * share, height),
            egui::Sense::hover(),
        );
    }
}

/// The whole pane's loading is one live region, not one per block.
pub(super) fn announce_loading(ui: &egui::Ui) {
    widgets::announce(ui, "usage-loading", "Loading usage");
}
