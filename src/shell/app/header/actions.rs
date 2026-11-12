//! The Reader's own actions in the header: Refresh, Agent log, Draft design
//! and Archive, shown as buttons instead of behind a `…` menu.

use super::{ButtonKind, IboardApp, Icon, Path, widgets};
use crate::domains::board::lifecycle::{self, Exit};

impl IboardApp {
    /// The issue's actions, right to left: Archive, Draft design, Agent log
    /// and Refresh, each only where it applies.
    pub(super) fn reader_actions(&mut self, ui: &mut egui::Ui, path: &Path, exists: bool) {
        let present = self.session.reading().is_some() && exists;
        let done = self.is_done_issue(path);
        if present && lifecycle::exit(done) == Exit::Archive {
            let archive =
                widgets::button(ui, ButtonKind::Header, Some(Icon::Archive), "Archive", None)
                    .on_hover_text("Move this issue to .archive/");
            if archive.clicked() {
                self.archive_issue(path);
            }
        }
        if present && !done && self.design_reachable() {
            let draft = widgets::button(ui, ButtonKind::Header, None, "Draft design", None)
                .on_hover_text("Ask the agent for a mockup of this issue");
            if draft.clicked() {
                self.start_enrichment(path);
            }
        }
        if present && self.has_agent_log(path) {
            let log = widgets::button(ui, ButtonKind::Header, Some(Icon::Log), "Agent log", None)
                .on_hover_text("Show the build agent's log");
            if log.clicked() {
                self.toggle_agent_log(path);
            }
        }
        if !self.prefs.auto {
            let refresh = widgets::button(
                ui,
                ButtonKind::Header,
                Some(Icon::Refresh),
                "Refresh",
                Some(self.refresh_key(path)),
            )
            .on_hover_text("Reload from disk");
            if refresh.clicked() {
                self.refresh();
            }
        }
    }
}
