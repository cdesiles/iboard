//! The OS folder picker: Add project… in the project menu, Choose folders…
//! in the groups dialog, and the WelcomeScreen's actions.
//!
//! macOS answers with an `NSOpenPanel` restricted to directories. Everywhere
//! else there is no picker yet, so the call reports that the user cancelled and
//! the recently opened projects remain the way to switch.

use std::path::PathBuf;

/// Asks the OS for one or more folders (`⌘`-click); empty when the user
/// cancels.
pub fn pick_folders() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        macos::pick()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::PathBuf;

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::NSString;

    pub fn pick() -> Vec<PathBuf> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Vec::new();
        };
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseFiles(false);
        panel.setCanChooseDirectories(true);
        panel.setAllowsMultipleSelection(true);
        panel.setResolvesAliases(true);
        panel.setMessage(Some(&NSString::from_str("Choose a project folder")));
        panel.setPrompt(Some(&NSString::from_str("Choose")));
        if panel.runModal() != NSModalResponseOK {
            return Vec::new();
        }
        panel
            .URLs()
            .iter()
            .filter_map(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
            .collect()
    }
}
