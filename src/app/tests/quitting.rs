//! Quitting while AI runs are at work.

use super::*;

/// Quitting while an AI run is at work asks first; `esc` keeps iboard open,
/// `↩` quits. With nothing at work it quits at once.
#[test]
fn quitting_with_a_run_at_work_asks_first() {
    let (dir, mut app) = project("quit-runs", &[("todo/migrate.md", "# Migrate\n")]);
    app.prefs.agent = fake_agent("sleep 30 # {prompt}");
    app.start_review(&dir.join("issues/todo/migrate.md"), false);
    app.request_quit();
    assert!(matches!(app.session.confirm, Some(Confirm::Quit(1))));
    assert!(!app.closing);
    let ctx = themed_ctx();
    frame(
        &ctx,
        &mut app,
        vec![key(Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(
        app.session.confirm.is_none() && !app.closing,
        "esc keeps iboard open"
    );
    app.request_quit();
    app.quit_anyway();
    assert!(app.closing);

    let mut idle = IboardApp::from_located(discovery::locate(&dir).unwrap());
    idle.request_quit();
    assert!(idle.session.confirm.is_none() && idle.closing);
}
