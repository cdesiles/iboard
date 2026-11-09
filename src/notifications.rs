//! Settings › General's "Notifications" switch, stored in the user
//! configuration (`~/.config/iboard/iboard.toml`):
//!
//! ```toml
//! [notifications]
//! auto_log = true  # open a Doing issue's agent log as the ticket opens
//! ```

use serde::{Deserialize, Serialize};

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
pub struct PrefsSection {
    auto_log: Option<bool>,
}

impl PrefsSection {
    pub fn prefs(self) -> Prefs {
        Prefs {
            auto_log: self.auto_log.unwrap_or(Prefs::default().auto_log),
        }
    }
}

pub fn prefs_section(prefs: &Prefs) -> PrefsSection {
    PrefsSection {
        auto_log: Some(prefs.auto_log),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_log_switch_is_on_by_default_and_can_be_turned_off() {
        assert!(Prefs::default().auto_log);
        let text = "[notifications]\nauto_log = false\n";
        let document: toml::Table = toml::from_str(text).unwrap();
        let section: PrefsSection = document
            .get("notifications")
            .and_then(|value| value.clone().try_into().ok())
            .unwrap();
        assert!(!section.prefs().auto_log);
    }
}
