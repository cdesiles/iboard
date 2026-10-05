//! Issue labels: one bracketed token at the very start of an issue's `# H1`
//! (`# [Discovery] Hey look!`), shown as a small chip on the board and in the
//! reader.
//!
//! The name is display data and is lowercased (`discovery`). Its colour is a
//! user preference: iboard ships a built-in preset, and the `[labels]` table of
//! the user configuration (`~/.config/iboard/iboard.toml`) overrides or adds a
//! `name = "#rrggbb"` mapping. The raw issue file is never rewritten for a
//! label; only the derived display title drops it.

use std::collections::BTreeMap;
use std::path::Path;

use egui::Color32;
use serde::Deserialize;

/// The labels iboard ships with, applied with zero configuration. Mid tones,
/// so the colour stays readable as chip text in both themes; the chip falls
/// back to `ink` when it does not.
pub const PRESETS: [(&str, &str); 5] = [
    ("discovery", "#0b6076"),
    ("bug", "#b12f22"),
    ("feature", "#1a6e42"),
    ("chore", "#5b6472"),
    ("question", "#8a5006"),
];

/// The resolved label colours: lowercased name → colour.
#[derive(Debug, Clone, Default)]
pub struct Labels {
    colors: BTreeMap<String, Color32>,
}

impl Labels {
    /// The built-in preset, with no user configuration.
    pub fn presets() -> Self {
        let mut labels = Self::default();
        for (name, hex) in PRESETS {
            if let Some(color) = parse_hex(hex) {
                labels.colors.insert(name.to_string(), color);
            }
        }
        labels
    }

    /// The presets, overridden and extended by the `[labels]` table of `file`.
    /// A missing or unreadable file leaves the presets untouched.
    pub fn load(file: Option<&Path>) -> Self {
        let mut labels = Self::presets();
        let Some(file) = file else {
            return labels;
        };
        let Ok(text) = std::fs::read_to_string(file) else {
            return labels;
        };
        let Ok(raw) = toml::from_str::<RawFile>(&text) else {
            return labels;
        };
        for (name, value) in raw.labels {
            let name = name.trim().to_lowercase();
            if name.is_empty() {
                continue;
            }
            match parse_hex(&value) {
                Some(color) => {
                    labels.colors.insert(name, color);
                }
                None => eprintln!("warning: ignoring label {name:?}: `{value}` is not #rrggbb"),
            }
        }
        labels
    }

    /// The colour mapped to `name`, or `None` when the label is unknown (it
    /// still shows, in the neutral style).
    pub fn color(&self, name: &str) -> Option<Color32> {
        self.colors.get(&name.to_lowercase()).copied()
    }
}

/// The `[labels]` section of the user configuration, as written on disk.
#[derive(Debug, Default, Deserialize)]
struct RawFile {
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

/// Parses `#rrggbb` (or a bare `rrggbb`). `None` for anything else.
pub fn parse_hex(value: &str) -> Option<Color32> {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&value[i..i + 2], 16).ok();
    Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// Splits a leading `[label]` off a level-1 heading, returning the lowercased
/// label (when present) and the display title. A token that is not at the very
/// start, an empty `[]`, and a markdown link (`[text](url)`) stay in the title.
pub fn split_leading_label(heading: &str) -> (Option<String>, String) {
    let trimmed = heading.trim_start();
    if let Some(rest) = trimmed.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        let name = rest[..end].trim();
        let after = &rest[end + 1..];
        if !name.is_empty() && !after.starts_with('(') {
            return (Some(name.to_lowercase()), after.trim_start().to_string());
        }
    }
    (None, heading.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_bracket_is_the_label() {
        let (label, title) = split_leading_label("[Discovery] Hey look!");
        assert_eq!(label.as_deref(), Some("discovery"));
        assert_eq!(title, "Hey look!");
    }

    #[test]
    fn the_label_is_case_insensitive_and_lowercased() {
        let (label, title) = split_leading_label("[BUG] Fix the crash");
        assert_eq!(label.as_deref(), Some("bug"));
        assert_eq!(title, "Fix the crash");
    }

    #[test]
    fn only_the_first_bracket_counts() {
        let (label, title) = split_leading_label("Hey [there]");
        assert_eq!(label, None);
        assert_eq!(title, "Hey [there]");
        let (label, title) = split_leading_label("[x][y] nested");
        assert_eq!(label.as_deref(), Some("x"));
        assert_eq!(title, "[y] nested");
    }

    #[test]
    fn empty_brackets_and_links_are_not_labels() {
        assert_eq!(split_leading_label("[] nothing").0, None);
        assert_eq!(split_leading_label("[home](https://x)").0, None);
    }

    #[test]
    fn hex_parses_with_or_without_the_hash() {
        assert_eq!(parse_hex("#0b6076"), Some(Color32::from_rgb(11, 96, 118)));
        assert_eq!(parse_hex("0b6076"), Some(Color32::from_rgb(11, 96, 118)));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("nope"), None);
    }

    #[test]
    fn the_preset_covers_the_five_shipped_names() {
        let labels = Labels::presets();
        for (name, hex) in PRESETS {
            assert_eq!(labels.color(name), parse_hex(hex));
        }
        assert_eq!(labels.color("unknown"), None);
        // Matching is case-insensitive.
        assert_eq!(labels.color("BUG"), labels.color("bug"));
    }

    #[test]
    fn the_config_overrides_and_extends_the_preset() {
        let dir = std::env::temp_dir().join(format!("iboard-labels-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("iboard.toml");

        // Without `[labels]`, the preset stands.
        std::fs::write(&file, "[ai]\ntool = \"opencode\"\n").unwrap();
        assert_eq!(Labels::load(Some(&file)).color("bug"), parse_hex("#b12f22"));

        // With it, a preset is overridden and a name is added.
        std::fs::write(&file, "[labels]\nbug = \"#ff0000\"\nspike = \"#123456\"\n").unwrap();
        let labels = Labels::load(Some(&file));
        assert_eq!(labels.color("bug"), Some(Color32::from_rgb(255, 0, 0)));
        assert_eq!(labels.color("spike"), Some(Color32::from_rgb(18, 52, 86)));
        assert_eq!(
            labels.color("feature"),
            parse_hex("#1a6e42"),
            "other presets stay"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
