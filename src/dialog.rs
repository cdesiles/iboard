//! The OS folder picker: the "Open folder…" row of the project menu, and
//! Choose folders… in the groups dialog.
//!
//! macOS answers with an `NSOpenPanel` restricted to directories. Everywhere
//! else there is no picker yet, so the call reports that the user cancelled and
//! the recently opened projects remain the way to switch.

use std::path::PathBuf;

/// Asks the OS for a folder. `None` when the user cancels, or on a platform
/// with no implementation.
pub fn pick_folder() -> Option<PathBuf> {
    pick(false).into_iter().next()
}

/// Asks the OS for one or more folders (`⌘`-click); empty when the user
/// cancels.
pub fn pick_folders() -> Vec<PathBuf> {
    pick(true)
}

fn pick(multiple: bool) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        macos::pick(multiple)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = multiple;
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::PathBuf;

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::NSString;

    pub fn pick(multiple: bool) -> Vec<PathBuf> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Vec::new();
        };
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseFiles(false);
        panel.setCanChooseDirectories(true);
        panel.setAllowsMultipleSelection(multiple);
        panel.setResolvesAliases(true);
        panel.setMessage(Some(&NSString::from_str(
            "Choose a folder containing an issues directory",
        )));
        panel.setPrompt(Some(&NSString::from_str("Open")));
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
