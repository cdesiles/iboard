//! Settings › General: the theme, Auto, notifications, and whether deleting
//! an issue asks first.

use super::Dialog;
use crate::general::Appearance;
use crate::theme;
use crate::widgets::{self, field_label, help};

impl Dialog {
    /// Settings › General: the theme, and whether deleting an issue asks
    /// first.
    pub(super) fn general_body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        field_label(ui, "Theme");
        let options = Appearance::ALL.map(|appearance| (appearance, appearance.name()));
        if let Some(appearance) = widgets::segmented(ui, self.appearance, &options) {
            self.appearance = appearance;
        }
        ui.add_space(theme::SPACE_2);
        let theme_help = match self.appearance {
            Appearance::Light => "Light, whatever the system's appearance.",
            Appearance::Dark => "Dark, whatever the system's appearance.",
            Appearance::System => "Follows the system's appearance, light or dark.",
        };
        help(ui, theme_help, p.ink_muted);
        ui.add_space(theme::SPACE_5);
        field_label(ui, "Auto");
        widgets::checkbox(ui, &mut self.auto, "Reload and save automatically");
        ui.add_space(theme::SPACE_2);
        let auto_help = if self.auto {
            "The board reloads when files change on disk, and edits save as you type."
        } else {
            "Refresh (⌘R) reloads the board, and the editor asks before leaving unsaved edits."
        };
        help(ui, auto_help, p.ink_muted);
        ui.add_space(theme::SPACE_5);
        field_label(ui, "Notifications");
        widgets::checkbox(ui, &mut self.notifications.build_done, "A build finishes");
        widgets::checkbox(ui, &mut self.notifications.build_failed, "A build fails");
        widgets::checkbox(
            ui,
            &mut self.notifications.review_ready,
            "A review is ready",
        );
        widgets::checkbox(ui, &mut self.notifications.ask_done, "An ask finishes");
        widgets::checkbox(
            ui,
            &mut self.notifications.auto_log,
            "Open the agent log on a Doing issue",
        );
        ui.add_space(theme::SPACE_2);
        help(
            ui,
            "Questions from a build always reach the bell, and wait in the ticket until answered.",
            p.ink_muted,
        );
        ui.add_space(theme::SPACE_5);
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
