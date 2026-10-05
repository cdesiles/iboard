//! Issue labels: one bracketed token at the very start of an issue's `# H1`
//! (`# [Discovery] Hey look!`), shown as a small chip on the board and in the
//! reader.
//!
//! The name is display data and is lowercased (`discovery`). Its colour is a
//! user preference: iboard ships a built-in preset, and the `[labels]` table of
//! the user configuration (`~/.config/iboard/iboard.toml`) overrides or adds a
//! `name = "#rrggbb"` mapping. The raw issue file is never rewritten for a
//! label; only the derived display title drops it. An empty value
//! (`bug = ""`) removes a preset.

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
            // An empty colour removes the label, presets included.
            if value.trim().is_empty() {
                labels.colors.remove(&name);
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

/// The built-in colour of `name`, as `#rrggbb`, when it has one.
pub fn preset_hex(name: &str) -> Option<&'static str> {
    PRESETS
        .iter()
        .find(|(preset, _)| *preset == name)
        .map(|(_, hex)| *hex)
}

/// `#rrggbb` for a colour.
pub fn to_hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

impl Labels {
    /// Every label with a colour, by name, as `#rrggbb`.
    pub fn entries(&self) -> Vec<(String, String)> {
        self.colors
            .iter()
            .map(|(name, color)| (name.clone(), to_hex(*color)))
            .collect()
    }
}

/// Writes the `[labels]` table: only what differs from the presets, so a
/// preset later improved by iboard still reaches labels left untouched. A
/// preset missing from `entries` was removed and is written as `name = ""`.
/// Keeps the file's other sections.
pub fn save(file: &Path, entries: &[(String, String)]) -> Result<(), String> {
    let mut table = toml::map::Map::new();
    for (name, hex) in entries {
        let name = name.trim().to_lowercase();
        let Some(color) = parse_hex(hex) else {
            continue;
        };
        let hex = to_hex(color);
        if name.is_empty() || preset_hex(&name) == Some(hex.as_str()) {
            continue;
        }
        table.insert(name, toml::Value::String(hex));
    }
    for (preset, _) in PRESETS {
        let kept = entries.iter().any(|(name, hex)| {
            name.trim().eq_ignore_ascii_case(preset) && parse_hex(hex).is_some()
        });
        if !kept {
            table.insert(preset.to_string(), toml::Value::String(String::new()));
        }
    }
    let parent = file
        .parent()
        .ok_or_else(|| "the configuration has no folder".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("Couldn't create {}: {err}", parent.display()))?;
    let mut document = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
        .and_then(|value| value.as_table().cloned())
        .unwrap_or_default();
    if table.is_empty() {
        document.remove("labels");
    } else {
        document.insert("labels".into(), toml::Value::Table(table));
    }
    let body =
        toml::to_string(&document).map_err(|err| format!("Couldn't write the config: {err}"))?;
    let temp = file.with_extension("tmp");
    std::fs::write(&temp, body)
        .map_err(|err| format!("Couldn't write {}: {err}", file.display()))?;
    std::fs::rename(&temp, file).map_err(|err| format!("Couldn't save {}: {err}", file.display()))
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

    #[test]
    fn saving_keeps_only_what_differs_from_the_presets() {
        let dir = std::env::temp_dir().join(format!("iboard-labels-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("iboard.toml");
        std::fs::write(&file, "[delete]\nconfirm = false\n").unwrap();
        let entries = vec![
            ("bug".to_string(), "#B12F22".to_string()),
            ("feature".to_string(), "#123456".to_string()),
            ("Ops".to_string(), "abcdef".to_string()),
            ("broken".to_string(), "nope".to_string()),
        ];
        save(&file, &entries).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains("bug"),
            "a preset left as is is not written: {text}"
        );
        assert!(text.contains("feature = \"#123456\""), "{text}");
        assert!(text.contains("ops = \"#abcdef\""), "{text}");
        assert!(!text.contains("broken"));
        assert!(text.contains("confirm = false"), "other sections stay");
        let labels = Labels::load(Some(&file));
        assert_eq!(labels.color("feature"), parse_hex("#123456"));
        assert_eq!(labels.color("bug"), parse_hex("#b12f22"));
        // The presets left out of `entries` were removed and stay removed.
        assert!(text.contains("chore = \"\""), "{text}");
        assert_eq!(labels.color("chore"), None);
        assert_eq!(labels.color("discovery"), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
