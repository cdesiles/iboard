//! The app in egui_kittest, run as eframe runs it, its widgets pressed by
//! label as a pointer does.

use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, HarnessBuilder};

use super::*;

/// The app in a kittest harness, beside whether its theme is installed.
pub(super) type Kit = Harness<'static, (bool, IboardApp)>;

/// `app` in a kittest harness of the window's size, two frames in, each run
/// as eframe runs it: its logic, then the drawing.
pub(super) fn kittest(app: IboardApp) -> Kit {
    kittest_with(app, Harness::builder().with_size(window().size()))
}

/// [`kittest`] from `builder`, which sets the window's size and the step.
pub(super) fn kittest_with(app: IboardApp, builder: HarnessBuilder<(bool, IboardApp)>) -> Kit {
    let mut harness = builder.build_ui_state(
        |ui, (themed, app): &mut (bool, IboardApp)| {
            // The fonts are registered on the first frame and usable from the next.
            if !*themed {
                theme::install(ui.ctx());
                *themed = true;
                return;
            }
            app.frame_logic(ui.ctx());
            app.draw(ui);
        },
        (false, app),
    );
    harness.run_steps(2);
    harness
}

/// Clicks the `nth` widget labelled `label` the way a pointer does: over
/// it for a frame first, as egui settles hover on the frame before.
pub(super) fn press_nth(harness: &mut Kit, label: &str, nth: usize) {
    harness.get_all_by_label(label).nth(nth).unwrap().hover();
    harness.run_steps(1);
    harness.get_all_by_label(label).nth(nth).unwrap().click();
    harness.run_steps(2);
}

/// [`press_nth`] on the only widget labelled `label`.
pub(super) fn press(harness: &mut Kit, label: &str) {
    harness.get_by_label(label);
    press_nth(harness, label, 0);
}
