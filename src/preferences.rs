//! The user's preferences, read in one go from the user configuration
//! (`~/.config/iboard/iboard.toml`): the sections Settings keeps, the
//! filter's pinned suggestions, and the switches Settings does not list.

use std::collections::BTreeMap;

use crate::agenda::{self, Range};
use crate::agent::{self, AgentConfig, BuildTimeout};
use crate::filter;
use crate::general::{self, Appearance};
use crate::labels::{self, Labels};
use crate::notifications;
use crate::settings::{Section, UserConfig};
use crate::stats::ReleasePrefs;
use crate::titles::{self, Engine};

const SPLITS: &str = "splits";

/// The `[splits]` section that stores `splits`.
pub fn splits_section(splits: &BTreeMap<String, u32>) -> Result<Section, String> {
    Section::new(SPLITS, splits)
}

/// The preferences the user configuration sets, each from its own section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    /// `[ai]`: the agent that reviews issues.
    pub agent: AgentConfig,
    /// `[build]`: the agent that implements Ready issues; `agent` without one.
    pub build_agent: AgentConfig,
    /// `[build]`: how long a build or an ask may work.
    pub build_timeout: BuildTimeout,
    /// `[build]`: how many build agents may work at once.
    pub agents: usize,
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
    /// `[labels]` and `[label_style]`: the label colours over the presets, and
    /// the labels that stand out.
    pub labels: Labels,
    /// `[splits]`: the right pane's width per pair of panes, in points. The
    /// board writes it as they are resized; Settings does not list it.
    pub splits: BTreeMap<String, u32>,
    /// `[new_issue]`: a draft keeps your notes under `## Notes`. The modal
    /// writes it as it is ticked; Settings does not list it.
    pub keep_notes: bool,
    /// `[agenda]`: the range the Agenda opens on. It writes it as it changes;
    /// Settings does not list it.
    pub agenda_range: Range,
    /// `[stats]`: the release lines on the Trends. Their checkbox writes it as
    /// it is ticked; Settings does not list it.
    pub releases: ReleasePrefs,
    /// `[notifications]`: what reaches the bell, and whether a Doing issue
    /// opens its agent log.
    pub notifications: notifications::Prefs,
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
            build_timeout: agent::load_build_timeout(config),
            agents: agent::load_agents(config),
            suggestions: filter::load_suggestions(config),
            title_engine: titles::load_engine(config),
            confirm_delete: general::load_confirm_delete(config),
            appearance: general::load_appearance(config),
            auto: general::load_auto(config),
            idesign: general::load_idesign(config),
            direnv: general::load_direnv(config),
            labels: Labels::load(config),
            splits: config.section(SPLITS),
            keep_notes: general::load_keep_notes(config),
            agenda_range: agenda::load_range(config),
            releases: ReleasePrefs::load(config),
            notifications: notifications::load_prefs(config),
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
        if self.build_agent != before.build_agent || self.build_timeout != before.build_timeout {
            sections.push(agent::build_section(&self.build_agent, self.build_timeout)?);
            if self.agents != agent::DEFAULT_AGENTS {
                sections.push(agent::agents_section(self.agents)?);
            }
        } else if self.agents != before.agents {
            sections.push(agent::agents_section(self.agents)?);
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
        if self.labels.entries() != before.labels.entries() {
            sections.push(labels::section(&labels::table(&self.labels.entries()))?);
        }
        if self.labels.stand_outs() != before.labels.stand_outs() {
            sections.push(labels::style_section(self.labels.stand_outs())?);
        }
        if self.notifications != before.notifications {
            sections.push(notifications::prefs_section(&self.notifications)?);
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
            build_timeout: BuildTimeout::default(),
            agents: agent::DEFAULT_AGENTS,
            suggestions: None,
            title_engine: Engine::Apple,
            confirm_delete: true,
            appearance: Appearance::Light,
            auto: true,
            idesign: false,
            direnv: false,
            labels: Labels::presets(),
            splits: BTreeMap::new(),
            keep_notes: true,
            agenda_range: Range::W12,
            releases: ReleasePrefs::default(),
            notifications: notifications::Prefs::default(),
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
             [build]\ntool = \"opencode\"\ntimeout = \"none\"\nagents = 4\n\
             [filter]\nsuggestions = [\"Bug\"]\n\
             [titles]\nengine = \"manual\"\n\
             [delete]\nconfirm = false\n\
             [appearance]\ntheme = \"system\"\n\
             [auto]\nenabled = false\n\
             [idesign]\nenabled = true\n\
             [direnv]\nenabled = true\n\
             [labels]\nspike = \"#123456\"\n\
             [label_style]\nstand_out = [\"bug\"]\n\
             [splits]\nreview = 420\n\
             [new_issue]\nkeep_notes = false\n\
             [agenda]\nrange = \"26w\"\n\
             [stats]\nshow_releases = false\nrelease_tags = \"rel-*\"\n",
        );
        let preferences = Preferences::load(&mut config);
        assert!(config.problems.is_empty(), "{:?}", config.problems);
        assert_eq!(
            preferences,
            Preferences {
                agent: configured(Tool::Claude, "opus"),
                build_agent: configured(Tool::OpenCode, ""),
                build_timeout: BuildTimeout::NONE,
                agents: 4,
                suggestions: Some(vec!["bug".to_string()]),
                title_engine: Engine::Off,
                confirm_delete: false,
                appearance: Appearance::System,
                auto: false,
                idesign: true,
                direnv: true,
                labels: colours("spike", "#123456"),
                splits: BTreeMap::from([("review".to_string(), 420)]),
                keep_notes: false,
                agenda_range: Range::W26,
                releases: ReleasePrefs {
                    show: false,
                    tags: "rel-*".to_string(),
                },
                notifications: notifications::Prefs::default(),
            }
        );
    }

    #[test]
    fn what_cannot_be_used_falls_back_and_is_reported_in_reading_order() {
        let mut config = parse(
            "[splits]\nreview = \"wide\"\n\
             [label_style]\nstand_out = \"bug\"\n\
             [labels]\nbug = \"red\"\n\
             [direnv]\nenabled = \"yes\"\n\
             [idesign]\nenabled = \"yes\"\n\
             [auto]\nenabled = \"no\"\n\
             [appearance]\ntheme = \"sepia\"\n\
             [delete]\nconfirm = \"no\"\n\
             [titles]\nengine = \"nope\"\n\
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
                "[label_style]",
                "[splits]",
            ]
        );
    }

    #[test]
    fn splits_are_saved_on_their_own_and_read_back() {
        let scratch = tempfile::tempdir().unwrap();
        let file = scratch.path().join("iboard.toml");
        let mine = "[delete]\nconfirm = false\n";
        std::fs::write(&file, mine).unwrap();
        let before = Preferences::load(&mut UserConfig::load(Some(&file)));
        let splits = BTreeMap::from([("log".to_string(), 512), ("review".to_string(), 400)]);
        let moved = Preferences {
            splits: splits.clone(),
            ..before.clone()
        };
        assert!(
            moved.sections_since(&before).unwrap().is_empty(),
            "Settings never rewrites [splits]"
        );
        settings::save(&file, &[splits_section(&splits).unwrap()]).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!("{mine}\n[splits]\nlog = 512\nreview = 400\n")
        );
        assert_eq!(Preferences::load(&mut UserConfig::load(Some(&file))), moved);
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

        let mut stand_out = before.labels.stand_outs().clone();
        stand_out.insert("spike".to_string());
        let after = Preferences {
            agent: AgentConfig {
                scope: ReviewMode::Deep,
                ..configured(Tool::Claude, "opus")
            },
            build_agent: configured(Tool::OpenCode, ""),
            build_timeout: BuildTimeout::ALL[0],
            title_engine: Engine::Agent,
            confirm_delete: false,
            appearance: Appearance::Dark,
            auto: false,
            labels: colours("spike", "#123456").with_stand_out(stand_out),
            ..before.clone()
        };
        settings::save(&file, &after.sections_since(&before).unwrap()).unwrap();

        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!(
                "{mine}\n\
                 [ai]\ntool = \"claude\"\nmodel = \"opus\"\nscope = \"project\"\n\n\
                 [build]\ntool = \"opencode\"\nmodel = \"default\"\ntimeout = \"5m\"\n\n\
                 [titles]\nengine = \"agent\"\n\n\
                 [delete]\nconfirm = false\n\n\
                 [appearance]\ntheme = \"dark\"\n\n\
                 [auto]\nenabled = false\n\n\
                 [labels]\nspike = \"#123456\"\n\n\
                 [label_style]\nstand_out = [\"bug\", \"spike\"]\n"
            )
        );
        let mut config = UserConfig::load(Some(&file));
        assert_eq!(Preferences::load(&mut config), after);
        assert!(config.problems.is_empty());
    }
}
