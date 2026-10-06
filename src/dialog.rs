//! The OS folder picker used by the "Open folder…" row of the project menu.
//!
//! macOS answers with an `NSOpenPanel` restricted to directories. Everywhere
//! else there is no picker yet, so the call reports that the user cancelled and
//! the recently opened projects remain the way to switch.

use std::path::PathBuf;

/// Asks the OS for a folder. `None` when the user cancels, or on a platform
/// with no implementation.
pub fn pick_folder() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        macos::pick_folder()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::PathBuf;

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::NSString;

    pub fn pick_folder() -> Option<PathBuf> {
        let mtm = MainThreadMarker::new()?;
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseFiles(false);
        panel.setCanChooseDirectories(true);
        panel.setAllowsMultipleSelection(false);
        panel.setResolvesAliases(true);
        panel.setMessage(Some(&NSString::from_str(
            "Choose a folder containing an issues directory",
        )));
        panel.setPrompt(Some(&NSString::from_str("Open")));
        if panel.runModal() != NSModalResponseOK {
            return None;
        }
        let path = panel.URLs().firstObject()?.path()?;
        Some(PathBuf::from(path.to_string()))
    }
}
