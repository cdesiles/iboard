//! Settings › General's "Notifications" switch, stored in the user
//! configuration (`~/.config/iboard/iboard.toml`):
//!
//! ```toml
//! [notifications]
//! auto_log = true  # open a Doing issue's agent log as the ticket opens
//! ```

use serde::{Deserialize, Serialize};

use crate::settings::{Section, UserConfig};

/// The `[notifications]` switches. Questions always reach the bell; only
/// opening their log is optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prefs {
    /// Open a Doing issue's agent log as the ticket opens.
    pub auto_log: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { auto_log: true }
    }
}

#[derive(Default, Deserialize, Serialize)]
struct RawPrefs {
    auto_log: Option<bool>,
}

pub fn load_prefs(config: &mut UserConfig) -> Prefs {
    let raw: RawPrefs = config.section("notifications");
    Prefs {
        auto_log: raw.auto_log.unwrap_or(Prefs::default().auto_log),
    }
}

pub fn prefs_section(prefs: &Prefs) -> Result<Section, String> {
    Section::new(
        "notifications",
        &RawPrefs {
            auto_log: Some(prefs.auto_log),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_agent_log_switch_is_on_by_default_and_can_be_turned_off() {
        assert!(Prefs::default().auto_log);
        let file = Path::new("iboard.toml");
        let mut config = UserConfig::parse("[notifications]\nauto_log = false\n", file);
        assert!(!load_prefs(&mut config).auto_log);
        assert!(config.problems.is_empty());
    }
}
