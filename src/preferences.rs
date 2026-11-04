//! The user's preferences, read in one go from the user configuration
//! (`~/.config/iboard/iboard.toml`): the sections Settings keeps, the
//! filter's pinned suggestions, and the switches Settings does not list.

use crate::agent::{self, AgentConfig};
use crate::filter;
use crate::general::{self, Appearance};
use crate::labels::{self, Labels};
use crate::settings::{Section, UserConfig};
use crate::titles::{self, Engine};

/// The preferences the user configuration sets, each from its own section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    /// `[ai]`: the agent that reviews issues.
    pub agent: AgentConfig,
    /// `[build]`: the agent that implements Ready issues; `agent` without one.
    pub build_agent: AgentConfig,
    /// `[filter]`: the labels the filter suggests; `None` suggests the
    /// board's most used.
    pub suggestions: Option<Vec<String>>,
    /// `[titles]`: what names the composer's new issues.
    pub title_engine: Engine,
    /// `[delete]`: deleting an issue asks first.
    pub confirm_delete: bool,
    /// `[appearance]`: the theme the windows draw in.
    pub appearance: Appearance,
    /// `[auto]`: reload from disk and save edits as you type.
    pub auto: bool,
    /// `[idesign]`: the beta Design panel and Draft design are reachable.
    pub idesign: bool,
    /// `[direnv]`: agents run through `direnv exec` in projects with an `.envrc`.
    pub direnv: bool,
    /// `[labels]`: the label colours over the presets.
    pub labels: Labels,
}

impl Default for Preferences {
    /// What an empty configuration gives: each section's own default.
    fn default() -> Self {
        Self::load(&mut UserConfig::default())
    }
}

impl Preferences {
    /// Reads each preference from its section of `config`. One that cannot
    /// be used keeps its default and is reported in `config.problems`, in
    /// reading order.
    pub fn load(config: &mut UserConfig) -> Self {
        let agent = agent::load(config);
        let build_agent = agent::load_build(config, &agent);
        Self {
            agent,
            build_agent,
            suggestions: filter::load_suggestions(config),
            title_engine: titles::load_engine(config),
            confirm_delete: general::load_confirm_delete(config),
            appearance: general::load_appearance(config),
            auto: general::load_auto(config),
            idesign: general::load_idesign(config),
            direnv: general::load_direnv(config),
            labels: Labels::load(config),
        }
    }

    /// The sections that store what changed since `before`, in the order
    /// Settings writes them. `[filter]`, `[idesign]` and `[direnv]`, which
    /// Settings does not edit, are left as the user wrote them.
    pub fn sections_since(&self, before: &Self) -> Result<Vec<Section>, String> {
        let mut sections = Vec::new();
        if self.agent != before.agent {
            sections.push(agent::section(&self.agent)?);
        }
        if self.build_agent != before.build_agent {
            sections.push(agent::build_section(&self.build_agent)?);
        }
        if self.title_engine != before.title_engine {
            sections.push(titles::section(self.title_engine)?);
        }
        if self.confirm_delete != before.confirm_delete {
            sections.push(general::section(self.confirm_delete)?);
        }
        if self.appearance != before.appearance {
            sections.push(general::appearance_section(self.appearance)?);
        }
        if self.auto != before.auto {
            sections.push(general::auto_section(self.auto)?);
        }
        if self.labels != before.labels {
            sections.push(labels::section(&labels::table(&self.labels.entries()))?);
        }
        Ok(sections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{ReviewMode, Tool};
    use crate::settings;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn parse(text: &str) -> UserConfig {
        UserConfig::parse(text, Path::new("iboard.toml"))
    }

    fn configured(tool: Tool, model: &str) -> AgentConfig {
        AgentConfig {
            tool,
            model: model.to_string(),
            configured: true,
            ..AgentConfig::default()
        }
    }

    fn colours(name: &str, hex: &str) -> Labels {
        Labels::with(&BTreeMap::from([(name.to_string(), hex.to_string())]))
    }

    #[test]
    fn the_defaults_are_what_an_empty_configuration_gives() {
        let defaults = Preferences {
            agent: AgentConfig::default(),
            build_agent: AgentConfig::default(),
            suggestions: None,
            title_engine: Engine::Apple,
            confirm_delete: true,
            appearance: Appearance::Light,
            auto: true,
            idesign: false,
            direnv: false,
            labels: Labels::presets(),
        };
        assert_eq!(Preferences::default(), defaults);
        let mut empty = UserConfig::default();
        assert_eq!(Preferences::load(&mut empty), defaults);
        assert!(empty.problems.is_empty());
    }

    #[test]
    fn each_preference_comes_from_its_section() {
        let mut config = parse(
            "[ai]\ntool = \"claude\"\nmodel = \"opus\"\n\
             [build]\ntool = \"opencode\"\n\
             [filter]\nsuggestions = [\"Bug\"]\n\
             [titles]\nengine = \"manual\"\n\
             [delete]\nconfirm = false\n\
             [appearance]\ntheme = \"system\"\n\
             [auto]\nenabled = false\n\
             [idesign]\nenabled = true\n\
             [direnv]\nenabled = true\n\
             [labels]\nspike = \"#123456\"\n",
        );
        let preferences = Preferences::load(&mut config);
        assert!(config.problems.is_empty(), "{:?}", config.problems);
        assert_eq!(
            preferences,
            Preferences {
                agent: configured(Tool::Claude, "opus"),
                build_agent: configured(Tool::OpenCode, ""),
                suggestions: Some(vec!["bug".to_string()]),
                title_engine: Engine::Manual,
                confirm_delete: false,
                appearance: Appearance::System,
                auto: false,
                idesign: true,
                direnv: true,
                labels: colours("spike", "#123456"),
            }
        );
    }

    #[test]
    fn what_cannot_be_used_falls_back_and_is_reported_in_reading_order() {
        let mut config = parse(
            "[labels]\nbug = \"red\"\n\
             [direnv]\nenabled = \"yes\"\n\
             [idesign]\nenabled = \"yes\"\n\
             [auto]\nenabled = \"no\"\n\
             [appearance]\ntheme = \"sepia\"\n\
             [delete]\nconfirm = \"no\"\n\
             [titles]\nengine = \"off\"\n\
             [filter]\nsuggestions = \"bug\"\n\
             [build]\ntool = \"codex\"\n\
             [ai]\ntool = \"codex\"\n",
        );
        assert_eq!(Preferences::load(&mut config), Preferences::default());
        let sections: Vec<&str> = config
            .problems
            .iter()
            .map(|problem| problem.split(':').next().unwrap_or_default())
            .collect();
        assert_eq!(
            sections,
            [
                "[ai]",
                "[build]",
                "[filter]",
                "[titles]",
                "[delete]",
                "[appearance]",
                "[auto]",
                "[idesign]",
                "[direnv]",
                "[labels]",
            ]
        );
    }

    #[test]
    fn saving_writes_what_settings_changed_in_its_order() {
        let scratch = tempfile::tempdir().unwrap();
        let file = scratch.path().join("iboard.toml");
        let mine = "[filter]\nsuggestions = [\"bug\"]\n";
        std::fs::write(&file, mine).unwrap();
        let before = Preferences::load(&mut UserConfig::load(Some(&file)));
        let unedited = Preferences {
            suggestions: None,
            idesign: true,
            direnv: true,
            ..before.clone()
        };
        assert!(
            unedited.sections_since(&before).unwrap().is_empty(),
            "Settings leaves [filter], [idesign] and [direnv] to the user"
        );
        let auto_off = Preferences {
            auto: false,
            ..before.clone()
        };
        assert_eq!(auto_off.sections_since(&before).unwrap().len(), 1);

        let after = Preferences {
            agent: AgentConfig {
                scope: ReviewMode::Deep,
                ..configured(Tool::Claude, "opus")
            },
            build_agent: configured(Tool::OpenCode, ""),
            title_engine: Engine::Agent,
            confirm_delete: false,
            appearance: Appearance::Dark,
            auto: false,
            labels: colours("spike", "#123456"),
            ..before.clone()
        };
        settings::save(&file, &after.sections_since(&before).unwrap()).unwrap();

        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!(
                "{mine}\n\
                 [ai]\ntool = \"claude\"\nmodel = \"opus\"\nscope = \"project\"\n\n\
                 [build]\ntool = \"opencode\"\nmodel = \"default\"\n\n\
                 [titles]\nengine = \"agent\"\n\n\
                 [delete]\nconfirm = false\n\n\
                 [appearance]\ntheme = \"dark\"\n\n\
                 [auto]\nenabled = false\n\n\
                 [labels]\nspike = \"#123456\"\n"
            )
        );
        let mut config = UserConfig::load(Some(&file));
        assert_eq!(Preferences::load(&mut config), after);
        assert!(config.problems.is_empty());
    }
}
