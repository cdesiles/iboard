//! The app in egui_kittest, run as eframe runs it, its widgets pressed by
//! label as a pointer does, and the fake agent its AI runs go to.

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

/// A project of its own for the test `tag`, its `issues/` holding `files`
/// as `(path, text)`, and the app opened on it. A path ending in `/` is an
/// empty folder.
pub(super) fn project(tag: &str, files: &[(&str, &str)]) -> (Scratch, IboardApp) {
    let dir = scratch_dir(tag);
    let issues = dir.join("issues");
    for (path, text) in files {
        let path = issues.join(path);
        if path.to_string_lossy().ends_with('/') {
            std::fs::create_dir_all(&path).unwrap();
        } else {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
        }
    }
    std::fs::create_dir_all(&issues).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    (dir, app)
}

/// Polls `app` until the review of `file` has landed.
pub(super) fn landed(app: &mut IboardApp, file: &Path) {
    assert!(
        until(|| {
            app.poll_review();
            app.past_reviews.contains_key(file)
        }),
        "the review of {} never landed",
        file.display()
    );
}

/// A review that passes on every count.
pub(super) const PASSING: &str = "printf 'RELEVANCE: Pass\\nok\\nSIZE: Pass\\nok\\nCLARITY: Pass\\nok\\nPROPOSAL: NONE' # {prompt}";

/// A configured agent that runs the custom `command`, as no test may start a
/// real one.
pub(super) fn fake_agent(command: &str) -> AgentConfig {
    AgentConfig {
        tool: agent::Tool::Custom,
        command: command.to_string(),
        configured: true,
        ..AgentConfig::default()
    }
}
