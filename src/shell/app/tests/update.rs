//! The status bar's newer-release offer and the dialog behind it.

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use super::*;
use crate::release::update::Update;

fn offered(tag: &str, version: &str) -> (Scratch, IboardApp) {
    let (dir, mut app) = project(tag, &[("todo/a.md", "# A\n")]);
    app.update.live = true;
    app.update.bundle = Some(PathBuf::from("/tmp/iboard.app"));
    app.update.latest = Some(Update {
        version: version.to_string(),
        asset: "https://example.com/iboard-0.5.0-aarch64-apple-darwin.zip".to_string(),
    });
    (dir, app)
}

#[test]
fn the_status_bar_offers_the_newer_release_and_the_dialog_can_wait() {
    let (_dir, app) = offered("update-offer", "0.5.0");
    let mut harness = kittest(app);
    harness.get_by_label("Update to 0.5.0");

    press(&mut harness, "Update to 0.5.0");
    harness.get_by_role_and_label(Role::AlertDialog, "Update iboard?");
    harness.get_by_label("iboard 0.5.0");
    let current = format!("You're on {}.", crate::release::update::VERSION);
    harness.get_by_label(&current);
    assert!(harness.state().1.update.open);

    harness.key_press(Key::Tab);
    harness.run_steps(2);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert!(
        !harness.state().1.update.open,
        "↩ on the focused Later closes, it does not download"
    );

    press(&mut harness, "Update to 0.5.0");
    press(&mut harness, "Later");
    assert!(!harness.state().1.update.open);
    assert!(harness.query_by_label("Later").is_none());
    harness.get_by_label("Update to 0.5.0");
}

#[test]
fn a_copy_outside_the_bundle_offers_no_update() {
    let (_dir, mut app) = project("update-dev", &[("todo/a.md", "# A\n")]);
    app.update.live = false;
    app.update.latest = Some(Update {
        version: "0.5.0".to_string(),
        asset: "https://example.com/iboard-0.5.0-aarch64-apple-darwin.zip".to_string(),
    });
    let harness = kittest(app);
    assert!(
        harness.query_by_label("Update to 0.5.0").is_none(),
        "a checkout never replaces itself"
    );
}
