//! Label colours: iboard's presets, the labels that stand out, and the
//! `[labels]` / `[label_style]` sections of the user configuration
//! (`~/.config/iboard/iboard.toml`).
//!
//! A preset follows the theme's `label-*` palette. The `[labels]` table
//! overrides or adds a `name = "#rrggbb"` mapping, one value in both themes;
//! an empty value (`bug = ""`) removes a preset. `[label_style] stand_out`
//! lists the labels drawn as tinted pills.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::kernel::labels::{Hue, normalize};

const PRESETS: [(&str, Hue); 5] = [
    ("discovery", Hue::Teal),
    ("bug", Hue::Red),
    ("feature", Hue::Green),
    ("chore", Hue::Grey),
    ("question", Hue::Ochre),
];

/// What stands out with no `[label_style]`.
const STAND_OUT: [&str; 1] = ["bug"];

/// A label colour, independent of any drawing toolkit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// A label's colour: a preset's, which follows the theme, or one of the
/// user's, the same in both themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelColor {
    Preset(Hue),
    Own(Rgb),
}

/// How a label's pill is drawn. With no colour it has no dot, and nothing to
/// stand out with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LabelStyle {
    pub color: Option<LabelColor>,
    pub stand_out: bool,
}

/// The `[labels]` section: a label name to its `#rrggbb`, or an empty value
/// to remove it.
pub type LabelsSection = BTreeMap<String, String>;

/// The `[label_style]` section.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct StyleSection {
    stand_out: Option<Vec<String>>,
}

/// The resolved label colours, by lowercased name, and the names that stand
/// out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Labels {
    colors: BTreeMap<String, LabelColor>,
    stand_out: BTreeSet<String>,
}

impl Labels {
    /// The built-in preset, with no user configuration.
    pub fn presets() -> Self {
        Self {
            colors: PRESETS
                .into_iter()
                .map(|(name, hue)| (name.to_string(), LabelColor::Preset(hue)))
                .collect(),
            stand_out: STAND_OUT.into_iter().map(str::to_string).collect(),
        }
    }

    /// The presets, overridden and extended by the `[labels]` section. A value
    /// that is not a colour is reported and skipped.
    pub fn read(table: LabelsSection) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        for (name, value) in &table {
            if !value.trim().is_empty() && parse_hex(value).is_none() {
                problems.push(format!("[labels]: {name} = \"{value}\" is not #rrggbb"));
            }
        }
        (Self::with(&table), problems)
    }

    /// These labels, with `names` standing out instead.
    pub fn with_stand_out(self, names: BTreeSet<String>) -> Self {
        Self {
            stand_out: names,
            ..self
        }
    }

    /// The presets with `table`'s changes: a colour sets a label, an empty
    /// value removes one, presets included.
    pub fn with(table: &BTreeMap<String, String>) -> Self {
        let mut labels = Self::presets();
        for (name, value) in table {
            let name = name.trim().to_lowercase();
            if name.is_empty() {
                continue;
            }
            if value.trim().is_empty() {
                labels.colors.remove(&name);
            } else if let Some(color) = parse_hex(value) {
                labels.colors.insert(name, LabelColor::Own(color));
            }
        }
        labels
    }

    /// How `name`'s pill is drawn. An unknown label still shows, with no dot.
    pub fn style(&self, name: &str) -> LabelStyle {
        let name = name.to_lowercase();
        LabelStyle {
            color: self.colors.get(&name).copied(),
            stand_out: self.stand_out.contains(&name),
        }
    }

    /// The names that stand out.
    pub fn stand_outs(&self) -> &BTreeSet<String> {
        &self.stand_out
    }
}

/// The hue iboard ships for `name`, when it is a preset.
pub fn preset_hue(name: &str) -> Option<Hue> {
    PRESETS
        .iter()
        .find(|(preset, _)| *preset == name)
        .map(|(_, hue)| *hue)
}

/// `#rrggbb` for a colour.
pub fn to_hex(Rgb(r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

impl Labels {
    /// Every label with a colour, by name: `#rrggbb` for one of the user's,
    /// empty for a preset that keeps its palette colour.
    pub fn entries(&self) -> Vec<(String, String)> {
        self.colors
            .iter()
            .map(|(name, color)| {
                let hex = match color {
                    LabelColor::Preset(_) => String::new(),
                    LabelColor::Own(rgb) => to_hex(*rgb),
                };
                (name.clone(), hex)
            })
            .collect()
    }
}

/// What `[labels]` holds for `entries`: only the colours set by hand, so a
/// preset later improved by iboard still reaches labels left untouched. A
/// preset without one keeps its palette colour; a preset missing from
/// `entries` was removed and is written as `name = ""`.
pub fn table(entries: &[(String, String)]) -> BTreeMap<String, String> {
    let mut table = BTreeMap::new();
    for (name, hex) in entries {
        let name = name.trim().to_lowercase();
        let Some(color) = parse_hex(hex) else {
            continue;
        };
        if !name.is_empty() {
            table.insert(name, to_hex(color));
        }
    }
    for (preset, _) in PRESETS {
        let kept = entries
            .iter()
            .any(|(name, _)| name.trim().eq_ignore_ascii_case(preset));
        if !kept {
            table.insert(preset.to_string(), String::new());
        }
    }
    table
}

/// These labels, with the `[label_style]` section's stand-outs.
pub fn styled(labels: Labels, section: StyleSection) -> Labels {
    match section.stand_out {
        Some(names) => {
            let names = names.iter().filter_map(|name| normalize(name)).collect();
            labels.with_stand_out(names)
        }
        None => labels,
    }
}

/// The `[labels]` section that stores `table`; `None` when it is empty.
pub fn section(table: &BTreeMap<String, String>) -> Option<BTreeMap<String, String>> {
    (!table.is_empty()).then(|| table.clone())
}

/// The `[label_style]` section that stores `names`; `None` for the default.
pub fn style_section(names: &BTreeSet<String>) -> Option<StyleSection> {
    if names.iter().map(String::as_str).eq(STAND_OUT) {
        return None;
    }
    let stand_out = Some(names.iter().cloned().collect());
    Some(StyleSection { stand_out })
}

/// Parses `#rrggbb` (or a bare `rrggbb`). `None` for anything else.
pub fn parse_hex(value: &str) -> Option<Rgb> {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&value[i..i + 2], 16).ok();
    Some(Rgb(byte(0)?, byte(2)?, byte(4)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::preferences::settings;

    #[test]
    fn hex_parses_with_or_without_the_hash() {
        assert_eq!(parse_hex("#0b6076"), Some(Rgb(11, 96, 118)));
        assert_eq!(parse_hex("0b6076"), Some(Rgb(11, 96, 118)));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("nope"), None);
    }

    #[test]
    fn the_preset_covers_the_five_shipped_names() {
        let labels = Labels::presets();
        for (name, hue) in PRESETS {
            assert_eq!(labels.style(name).color, Some(LabelColor::Preset(hue)));
        }
        assert_eq!(labels.style("unknown"), LabelStyle::default());
        // Matching is case-insensitive.
        assert_eq!(labels.style("BUG"), labels.style("bug"));
        assert_eq!(preset_hue("bug"), Some(Hue::Red));
    }

    fn load(text: &str) -> (Labels, Vec<String>) {
        let document: toml::Table = toml::from_str(text).unwrap();
        let table: LabelsSection = document
            .get("labels")
            .and_then(|value| value.clone().try_into().ok())
            .unwrap_or_default();
        let (labels, problems) = Labels::read(table);
        let style: StyleSection = document
            .get("label_style")
            .and_then(|value| value.clone().try_into().ok())
            .unwrap_or_default();
        (styled(labels, style), problems)
    }

    #[test]
    fn the_config_overrides_and_extends_the_preset() {
        let red = Some(LabelColor::Preset(Hue::Red));
        let own = |hex| parse_hex(hex).map(LabelColor::Own);

        // Without `[labels]`, the preset stands.
        let (labels, _) = load("[ai]\ntool = \"opencode\"\n");
        assert_eq!(labels.style("bug").color, red);

        // With it, a preset is overridden and a name is added.
        let (labels, problems) = load("[labels]\nbug = \"#ff0000\"\nspike = \"#123456\"\n");
        assert_eq!(labels.style("bug").color, own("#ff0000"));
        assert_eq!(labels.style("spike").color, own("#123456"));
        assert_eq!(
            labels.style("feature").color,
            Some(LabelColor::Preset(Hue::Green)),
            "other presets stay"
        );
        assert!(problems.is_empty());

        // A value that is not a colour is skipped, and reported.
        let (labels, problems) = load("[labels]\nbug = \"red\"\nops = \"#123456\"\n");
        assert_eq!(labels.style("bug").color, red);
        assert_eq!(labels.style("ops").color, own("#123456"));
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn a_colour_set_by_hand_stays_as_written_even_on_the_palette() {
        let (labels, _) = load("[labels]\nbug = \"#bc3228\"\n");
        assert_eq!(
            labels.style("bug").color,
            parse_hex("#bc3228").map(LabelColor::Own)
        );
        assert_eq!(
            table(&labels.entries()).get("bug").map(String::as_str),
            Some("#bc3228")
        );
    }

    #[test]
    fn stand_out_is_bug_when_absent_and_as_written_otherwise() {
        let stands_out = |text: &str, name: &str| load(text).0.style(name).stand_out;
        assert!(stands_out("", "bug"));
        assert!(!stands_out("", "chore"));
        let chosen = "[label_style]\nstand_out = [\"Tech debt\", \"chore\"]\n";
        assert!(!stands_out(chosen, "bug"));
        assert!(stands_out(chosen, "chore"));
        assert!(stands_out(chosen, "tech-debt"));
        assert!(!stands_out("[label_style]\nstand_out = []\n", "bug"));
    }

    #[test]
    fn stand_out_is_stored_only_when_it_differs_from_the_default() {
        let scratch = tempfile::tempdir().unwrap();
        let file = scratch.path().join("iboard.toml");
        let save = |names: &[&str]| {
            let names: BTreeSet<String> = names.iter().map(|name| name.to_string()).collect();
            let section = match style_section(&names) {
                Some(style) => settings::Section::new("label_style", &style).unwrap(),
                None => settings::Section::removed("label_style"),
            };
            settings::save(&file, &[section]).unwrap();
            let back = load(&std::fs::read_to_string(&file).unwrap()).0;
            assert_eq!(*back.stand_outs(), names);
            std::fs::read_to_string(&file).unwrap()
        };
        assert_eq!(save(&["chore"]), "[label_style]\nstand_out = [\"chore\"]\n");
        assert_eq!(save(&[]), "[label_style]\nstand_out = []\n");
        assert_eq!(save(&["bug"]), "");
    }

    #[test]
    fn saving_keeps_only_the_colours_set_by_hand() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let file = dir.join("iboard.toml");
        std::fs::write(&file, "[delete]\nconfirm = false\n").unwrap();
        let entries = vec![
            ("bug".to_string(), String::new()),
            ("feature".to_string(), "#123456".to_string()),
            ("Ops".to_string(), "abcdef".to_string()),
            ("broken".to_string(), "nope".to_string()),
            ("question".to_string(), "nope".to_string()),
        ];
        let saved = match section(&table(&entries)) {
            Some(table) => settings::Section::new("labels", &table).unwrap(),
            None => settings::Section::removed("labels"),
        };
        settings::save(&file, &[saved]).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains("bug") && !text.contains("question"),
            "a preset left as is is not written: {text}"
        );
        assert!(text.contains("feature = \"#123456\""), "{text}");
        assert!(text.contains("ops = \"#abcdef\""), "{text}");
        assert!(!text.contains("broken"));
        assert!(text.contains("confirm = false"), "other sections stay");
        let labels = load(&text).0;
        let color = |name| labels.style(name).color;
        assert_eq!(color("feature"), parse_hex("#123456").map(LabelColor::Own));
        assert_eq!(color("bug"), Some(LabelColor::Preset(Hue::Red)));
        // The presets left out of `entries` were removed and stay removed.
        assert!(text.contains("chore = \"\""), "{text}");
        assert_eq!(color("chore"), None);
        assert_eq!(color("discovery"), None);
        assert_eq!(table(&labels.entries()), table(&entries), "round trip");
    }
}
