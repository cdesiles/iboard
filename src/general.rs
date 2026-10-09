//! Settings › General, stored in the user configuration
//! (`~/.config/iboard/iboard.toml`):
//!
//! ```toml
//! [delete]
//! confirm = true   # false: delete without asking
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
}
