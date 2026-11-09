//! Settings › General, stored in the user configuration
//! (`~/.config/iboard/iboard.toml`):
//!
//! ```toml
//! [delete]
//! confirm = true   # false: delete without asking
//!
//! [appearance]
//! theme = "light"  # "dark", or "system" to follow macOS
//!
//! [auto]
//! enabled = true   # false: no reload from disk, no saving as you type
//! ```
//!
//! NewIssueModal writes its Keep my notes as it is ticked:
//!
//! ```toml
//! [new_issue]
//! keep_notes = true  # false: a draft drops your notes
//! ```
//!
//! Beta features stay off unless their section turns them on, and Settings
//! does not list them:
//!
//! ```toml
//! [idesign]
//! enabled = false  # true: the Design panel and Draft design
//! ```

use serde::{Deserialize, Serialize};

/// The `[delete]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
pub struct DeleteSection {
    confirm: Option<bool>,
}

impl DeleteSection {
    /// Whether deleting an issue asks first; on unless the section says
    /// otherwise.
    pub fn confirm(self) -> bool {
        self.confirm.unwrap_or(true)
    }
}

/// The `[delete]` section that stores `confirm`.
pub fn section(confirm: bool) -> DeleteSection {
    DeleteSection {
        confirm: Some(confirm),
    }
}

/// A section that holds one `enabled` switch: `[auto]`, `[idesign]`,
/// `[direnv]`.
#[derive(Default, Deserialize, Serialize)]
pub struct Switch {
    enabled: Option<bool>,
}

impl Switch {
    /// The switch's value, or `default` when the section does not set it.
    pub fn is_on(self, default: bool) -> bool {
        self.enabled.unwrap_or(default)
    }
}

/// The `[auto]` section that stores `enabled`.
pub fn auto_section(enabled: bool) -> Switch {
    Switch {
        enabled: Some(enabled),
    }
}

/// The `[new_issue]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
pub struct NewIssueSection {
    keep_notes: Option<bool>,
}

impl NewIssueSection {
    /// Whether NewIssueModal keeps your notes under a draft; on unless the
    /// section says otherwise.
    pub fn keep_notes(self) -> bool {
        self.keep_notes.unwrap_or(true)
    }
}

/// The `[new_issue]` section that stores `keep_notes`.
pub fn keep_notes_section(keep_notes: bool) -> NewIssueSection {
    NewIssueSection {
        keep_notes: Some(keep_notes),
    }
}

/// The theme the windows draw in. Light unless `[appearance]` says otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
    /// Follows the system's appearance.
    System,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::Light, Appearance::Dark, Appearance::System];

    /// The value stored in `[appearance] theme`.
    pub fn key(self) -> &'static str {
        match self {
            Appearance::Light => "light",
            Appearance::Dark => "dark",
            Appearance::System => "system",
        }
    }

    /// The name Settings shows.
    pub fn name(self) -> &'static str {
        match self {
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
            Appearance::System => "System",
        }
    }

    /// The theme the live catalog opens in when no window can say.
    pub fn catalog_theme(self) -> &'static str {
        let dark = match self {
            Appearance::Light => false,
            Appearance::Dark => true,
            Appearance::System => system_is_dark(),
        };
        if dark { "dark" } else { "light" }
    }
}

/// Whether macOS draws in dark mode: the key exists only then.
fn system_is_dark() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("Dark"))
}

/// The `[appearance]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
pub struct AppearanceSection {
    theme: Option<String>,
}

impl AppearanceSection {
    /// The chosen theme. An unknown value is reported and reads as light.
    pub fn appearance(self) -> (Appearance, Vec<String>) {
        let Some(theme) = self.theme else {
            return (Appearance::default(), Vec::new());
        };
        match Appearance::ALL
            .into_iter()
            .find(|appearance| appearance.key() == theme.trim().to_lowercase())
        {
            Some(appearance) => (appearance, Vec::new()),
            None => (
                Appearance::default(),
                vec![format!(
                    "[appearance]: theme = \"{theme}\" is not light, dark or system"
                )],
            ),
        }
    }
}

/// The `[appearance]` section that stores `appearance`.
pub fn appearance_section(appearance: Appearance) -> AppearanceSection {
    AppearanceSection {
        theme: Some(appearance.key().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section<T: serde::de::DeserializeOwned + Default>(text: &str, name: &str) -> T {
        let document: toml::Table = toml::from_str(text).unwrap();
        document
            .get(name)
            .and_then(|value| value.clone().try_into().ok())
            .unwrap_or_default()
    }

    #[test]
    fn asking_is_on_by_default_and_can_be_turned_off() {
        assert!(DeleteSection::default().confirm());
        let other: DeleteSection = section("[titles]\nengine = \"manual\"\n", "delete");
        assert!(other.confirm());
        let off: DeleteSection = section("[delete]\nconfirm = false\n", "delete");
        assert!(!off.confirm());
    }

    #[test]
    fn auto_is_on_by_default_and_can_be_turned_off() {
        assert!(Switch::default().is_on(true));
        let off: Switch = section("[auto]\nenabled = false\n", "auto");
        assert!(!off.is_on(true));
        let on: Switch = section("[auto]\nenabled = true\n", "auto");
        assert!(on.is_on(true));
    }

    #[test]
    fn idesign_is_off_by_default_and_can_be_turned_on() {
        assert!(!Switch::default().is_on(false));
        let other: Switch = section("[auto]\nenabled = true\n", "idesign");
        assert!(!other.is_on(false));
        let on: Switch = section("[idesign]\nenabled = true\n", "idesign");
        assert!(on.is_on(false));
    }

    #[test]
    fn the_theme_is_light_by_default_and_can_be_dark_or_follow_the_system() {
        assert_eq!(
            AppearanceSection::default().appearance().0,
            Appearance::Light
        );
        for appearance in Appearance::ALL {
            let text = format!("[appearance]\ntheme = \"{}\"\n", appearance.key());
            let parsed: AppearanceSection = section(&text, "appearance");
            let (chosen, problems) = parsed.appearance();
            assert_eq!(chosen, appearance);
            assert!(problems.is_empty());
        }
        let dark: AppearanceSection = section("[appearance]\ntheme = \"Dark\"\n", "appearance");
        assert_eq!(dark.appearance().0, Appearance::Dark);
        let sepia: AppearanceSection = section("[appearance]\ntheme = \"sepia\"\n", "appearance");
        let (chosen, problems) = sepia.appearance();
        assert_eq!(chosen, Appearance::Light);
        assert_eq!(Appearance::Light.catalog_theme(), "light");
        assert_eq!(Appearance::Dark.catalog_theme(), "dark");
        assert_eq!(problems.len(), 1, "an unknown theme is reported");
    }
}
