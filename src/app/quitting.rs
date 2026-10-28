//! Quitting without losing the AI runs: with agents at work, ⌘Q asks
//! first; a termination signal quits the way ⌘Q does.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

/// How long the app has to quit on a signal before the agents are stopped
/// without it.
const GRACE: Duration = Duration::from_secs(3);

static REQUESTED: AtomicBool = AtomicBool::new(false);
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

/// Turns SIGTERM, SIGINT and SIGHUP into a quit: edits saved, agents stopped.
/// Call before the window opens.
pub fn on_signals() {
    #[cfg(unix)]
    process::on_termination(|| {
        REQUESTED.store(true, Ordering::SeqCst);
        if let Some(ctx) = CONTEXT.get() {
            ctx.request_repaint();
        }
        std::thread::sleep(GRACE);
        process::stop_all();
        std::process::exit(0);
    });
}

/// The window's context, which a signal wakes up.
pub(super) fn watch(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

/// Whether a signal asked to quit.
pub(super) fn requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

impl IboardApp {
    /// The AI runs at work: reviews, updates, builds and asks.
    pub(super) fn runs_at_work(&self) -> usize {
        let runs = self.session.review_runs.running_count() + self.building_now().len();
        runs + self.session.asks.len()
    }

    /// ⌘Q or the window closing: quits, asking first while agents work.
    pub(super) fn request_quit(&mut self) {
        match self.runs_at_work() {
            0 => self.leave(Leave::Quit),
            runs => self.session.confirm = Some(Confirm::Quit(runs)),
        }
    }

    /// Quit, agreed with agents at work: they stop.
    pub(super) fn quit_anyway(&mut self) {
        self.session.confirm = None;
        self.leave(Leave::Quit);
    }
}
