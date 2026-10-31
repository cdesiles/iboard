//! A later `iboard` hands this window the board to show, or only asks it to
//! come forward, rather than opening a window of its own.

use super::*;

impl IboardApp {
    /// Shows what the iboards started since the last frame asked for, once
    /// the editor's edits are saved, then comes forward.
    pub(super) fn answer_handovers(&mut self, ctx: &egui::Context) {
        let requests: Vec<Request> = self.handovers.iter().flat_map(|r| r.try_iter()).collect();
        if requests.is_empty() {
            return;
        }
        for request in requests {
            let target = match request {
                Request::Focus => continue,
                Request::Project(project) => all::Target::Project(recents::canonical(&project)),
                Request::Group(name) => all::Target::Group(name),
            };
            if target != self.current_target() {
                self.leave(Leave::Open(target));
            }
        }
        come_forward(ctx);
    }
}

/// In front of the other apps' windows, out of the Dock or a hidden app.
fn come_forward(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().minimized == Some(true)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    }
    #[cfg(target_os = "macos")]
    activate();
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}

/// Makes iboard the active app, even hidden, which winit's `Focus` skips.
/// The deprecated call: `activate()` waits for the terminal to yield, which it
/// never does.
#[cfg(target_os = "macos")]
fn activate() {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    if app.isHidden() {
        app.unhide(None);
    }
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}
