//! The agent log beside an issue: what the build agent does on it while it
//! runs, and what it did once it is done.

use super::review::review_head;
use super::*;
use crate::build_log;

/// The log open beside the reader, read again whenever its file grows.
pub(super) struct LogView {
    pub(super) path: PathBuf,
    file: PathBuf,
    len: Option<u64>,
    pub(super) text: String,
}

impl LogView {
    fn reload(&mut self) {
        let len = std::fs::metadata(&self.file).ok().map(|meta| meta.len());
        if len != self.len {
            self.len = len;
            self.text = build_log::load(&self.file).unwrap_or_default();
        }
    }
}

impl IboardApp {
    /// Whether the build agent works on `path` now or ever did.
    pub(super) fn has_agent_log(&self, path: &Path) -> bool {
        self.building_now()
            .is_some_and(|(building, _)| building == path)
            || build_log::path(&self.board.root, path).is_file()
    }

    pub(super) fn agent_log_open(&self, path: &Path) -> bool {
        self.session
            .agent_log
            .as_ref()
            .is_some_and(|log| log.path == path)
    }

    /// Agent log, in the reader's header: opens the log beside the issue, or
    /// closes it.
    pub(super) fn agent_log_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let open = self.agent_log_open(path);
        let kind = if open {
            ButtonKind::Open
        } else {
            ButtonKind::Secondary
        };
        if widgets::button(ui, kind, None, "Agent log", None)
            .on_hover_text("What the build agent does, or did, on this issue")
            .clicked()
        {
            self.toggle_agent_log(path);
        }
    }

    pub(super) fn toggle_agent_log(&mut self, path: &Path) {
        self.session.agent_log = (!self.agent_log_open(path)).then(|| LogView {
            path: path.to_path_buf(),
            file: build_log::path(&self.board.root, path),
            len: None,
            text: String::new(),
        });
    }

    /// The log's panel: whether the agent still runs, then everything it
    /// said and did, kept scrolled to its newest line.
    pub(super) fn agent_log_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let running = self.session.agent_log.as_ref().and_then(|log| {
            self.building_now()
                .filter(|(building, _)| *building == log.path)
                .map(|(_, since)| since)
        });
        let Some(log) = self.session.agent_log.as_mut() else {
            return;
        };
        log.reload();
        let detail = match running {
            Some(since) => format!("running · {}", stopwatch(since.elapsed())),
            None => "not running".to_string(),
        };
        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                review_head(
                    ui,
                    &format!("Agent log of {}", file_name(&log.path)),
                    &detail,
                );
                Frame::new()
                    .inner_margin(Margin::same(theme::SPACE_4 as i8))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("agent_log")
                            .stick_to_bottom(true)
                            .auto_shrink(false)
                            .show(ui, |ui| {
                                let (text, color) = match log.text.as_str() {
                                    "" => ("The agent has not said anything yet.", p.ink_muted),
                                    text => (text, p.ink),
                                };
                                ui.label(
                                    RichText::new(text)
                                        .font(egui::FontId::monospace(11.0))
                                        .color(color),
                                );
                            });
                    });
            });
    }
}
