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

use serde::{Deserialize, Serialize};

use crate::settings::{Section, UserConfig};

/// The `[delete]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
struct RawDelete {
    confirm: Option<bool>,
}

/// Whether deleting an issue asks first; on unless `[delete]` says otherwise.
pub fn load_confirm_delete(config: &mut UserConfig) -> bool {
    config
        .section::<RawDelete>("delete")
        .confirm
        .unwrap_or(true)
}

/// The `[delete]` section that stores `confirm`.
pub fn section(confirm: bool) -> Result<Section, String> {
    Section::new(
        "delete",
        &RawDelete {
            confirm: Some(confirm),
        },
    )
}

/// The `[auto] enabled` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
struct RawAuto {
    enabled: Option<bool>,
}

/// Whether the board reloads from disk and the editor saves as you type; on
/// unless `[auto]` says otherwise.
pub fn load_auto(config: &mut UserConfig) -> bool {
    config.section::<RawAuto>("auto").enabled.unwrap_or(true)
}

/// The `[auto]` section that stores `enabled`.
pub fn auto_section(enabled: bool) -> Result<Section, String> {
    Section::new(
        "auto",
        &RawAuto {
            enabled: Some(enabled),
        },
    )
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
}

/// The `[appearance]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
struct RawAppearance {
    theme: Option<String>,
}

/// The chosen theme. An unknown value is reported and reads as light.
pub fn load_appearance(config: &mut UserConfig) -> Appearance {
    let Some(theme) = config.section::<RawAppearance>("appearance").theme else {
        return Appearance::default();
    };
    match Appearance::ALL
        .into_iter()
        .find(|appearance| appearance.key() == theme.trim().to_lowercase())
    {
        Some(appearance) => appearance,
        None => {
            config.problem(format!(
                "[appearance]: theme = \"{theme}\" is not light, dark or system"
            ));
            Appearance::default()
        }
    }
}

/// The `[appearance]` section that stores `appearance`.
pub fn appearance_section(appearance: Appearance) -> Result<Section, String> {
    Section::new(
        "appearance",
        &RawAppearance {
            theme: Some(appearance.key().to_string()),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn asking_is_on_by_default_and_can_be_turned_off() {
        let file = Path::new("iboard.toml");
        assert!(load_confirm_delete(&mut UserConfig::default()));
        let mut config = UserConfig::parse("[titles]\nengine = \"manual\"\n", file);
        assert!(load_confirm_delete(&mut config));
        let mut config = UserConfig::parse("[delete]\nconfirm = false\n", file);
        assert!(!load_confirm_delete(&mut config));
        let mut config = UserConfig::parse("[delete]\nconfirm = \"no\"\n", file);
        assert!(load_confirm_delete(&mut config));
        assert_eq!(config.problems.len(), 1, "a wrong type is reported");
    }

    #[test]
    fn auto_is_on_by_default_and_can_be_turned_off() {
        let file = Path::new("iboard.toml");
        assert!(load_auto(&mut UserConfig::default()));
        let mut config = UserConfig::parse("[auto]\nenabled = false\n", file);
        assert!(!load_auto(&mut config));
        let mut config = UserConfig::parse("[auto]\nenabled = true\n", file);
        assert!(load_auto(&mut config));
    }

    #[test]
    fn the_theme_is_light_by_default_and_can_be_dark_or_follow_the_system() {
        let file = Path::new("iboard.toml");
        assert_eq!(
            load_appearance(&mut UserConfig::default()),
            Appearance::Light
        );
        for appearance in Appearance::ALL {
            let text = format!("[appearance]\ntheme = \"{}\"\n", appearance.key());
            let mut config = UserConfig::parse(&text, file);
            assert_eq!(load_appearance(&mut config), appearance);
            assert!(config.problems.is_empty());
        }
        let mut config = UserConfig::parse("[appearance]\ntheme = \"Dark\"\n", file);
        assert_eq!(load_appearance(&mut config), Appearance::Dark);
        let mut config = UserConfig::parse("[appearance]\ntheme = \"sepia\"\n", file);
        assert_eq!(load_appearance(&mut config), Appearance::Light);
        assert_eq!(config.problems.len(), 1, "an unknown theme is reported");
    }
}
