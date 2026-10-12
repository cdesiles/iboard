//! Settings › General: whether deleting an issue asks first.

use super::Dialog;
use crate::theme::{self};
use crate::widgets::{self, field_label, help};

impl Dialog {
    /// Settings › General: whether deleting an issue asks first.
    pub(super) fn general_body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        field_label(ui, "Deleting issues");
        widgets::checkbox(ui, &mut self.confirm_delete, "Ask before deleting an issue");
        ui.add_space(theme::SPACE_2);
        let help_line = if self.confirm_delete {
            "Delete opens a confirmation first: Del then ↩."
        } else {
            "Delete moves the file to the Trash at once. ⌘Z puts it back, or restore it from the Trash."
        };
        help(ui, help_line, p.ink_muted);
        if let Some(error) = &self.save_error {
            ui.add_space(theme::SPACE_5);
            help(ui, error, p.danger);
        }
    }
}
