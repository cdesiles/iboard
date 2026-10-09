//! Issue labels: the bracketed tokens at the very start of an issue's `# H1`
//! (`# [Discovery][UI] Hey look!`), plus a front-matter `labels:` list, shown
//! as small chips on the board and in the reader.
//!
//! The name is display data and is lowercased (`discovery`). Its colour is a
//! user preference: iboard ships a built-in preset, and the `[labels]` table of
//! the user configuration (`~/.config/iboard/iboard.toml`) overrides or adds a
//! `name = "#rrggbb"` mapping. An empty value (`bug = ""`) removes a preset.
//! The display title drops the labels; the file keeps them, and the
//! LabelPicker adds one by rewriting that single line (see [`add_label`]).

use std::collections::BTreeMap;
use std::path::Path;

use egui::Color32;

use crate::settings::{Section, UserConfig};

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

    /// The presets, overridden and extended by the `[labels]` section. A value
    /// that is not a colour is reported and skipped.
    pub fn load(config: &mut UserConfig) -> Self {
        let table: BTreeMap<String, String> = config.section("labels");
        for (name, value) in &table {
            if !value.trim().is_empty() && parse_hex(value).is_none() {
                config.problem(format!("[labels]: {name} = \"{value}\" is not #rrggbb"));
            }
        }
        Self::with(&table)
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
                labels.colors.insert(name, color);
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

/// What `[labels]` holds for `entries`: only what differs from the presets,
/// so a preset later improved by iboard still reaches labels left untouched.
/// A preset missing from `entries` was removed and is written as `name = ""`.
pub fn table(entries: &[(String, String)]) -> BTreeMap<String, String> {
    let mut table = BTreeMap::new();
    for (name, hex) in entries {
        let name = name.trim().to_lowercase();
        let Some(color) = parse_hex(hex) else {
            continue;
        };
        let hex = to_hex(color);
        if name.is_empty() || preset_hex(&name) == Some(hex.as_str()) {
            continue;
        }
        table.insert(name, hex);
    }
    for (preset, _) in PRESETS {
        let kept = entries.iter().any(|(name, hex)| {
            name.trim().eq_ignore_ascii_case(preset) && parse_hex(hex).is_some()
        });
        if !kept {
            table.insert(preset.to_string(), String::new());
        }
    }
    table
}

/// The `[labels]` section that stores `table`; none at all when it is empty.
pub fn section(table: &BTreeMap<String, String>) -> Result<Section, String> {
    if table.is_empty() {
        Ok(Section::removed("labels"))
    } else {
        Section::new("labels", table)
    }
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

/// The longest label kept, in characters.
pub const MAX_LABEL_CHARS: usize = 25;

/// A label as iboard shows it: trimmed, lowercased, spaces as hyphens, at most
/// 25 characters (`[Tech debt]` gives `tech-debt`). `None` when empty.
pub fn normalize(name: &str) -> Option<String> {
    let name = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    let name: String = name.chars().take(MAX_LABEL_CHARS).collect();
    (!name.is_empty()).then_some(name)
}

/// Splits the leading `[label]` groups off a level-1 heading, returning the
/// labels (normalized, repeats merged) and the display title. `[Bug] [UI] T`
/// and `[Bug][UI] T` both give `bug` and `ui`. Brackets later in the title, an
/// empty `[]` (dropped) and a markdown link (`[text](url)`) end the run.
pub fn split_leading_labels(heading: &str) -> (Vec<String>, String) {
    let mut labels: Vec<String> = Vec::new();
    let mut rest = heading.trim_start();
    while let Some(inner) = rest.strip_prefix('[')
        && let Some(end) = inner.find(']')
    {
        let after = &inner[end + 1..];
        if after.starts_with('(') {
            break;
        }
        if let Some(name) = normalize(&inner[..end])
            && !labels.contains(&name)
        {
            labels.push(name);
        }
        rest = after.trim_start();
    }
    (labels, rest.trim().to_string())
}

/// The `labels:` list of a markdown file's front matter, normalized: either
/// inline (`labels: [bug, ui]`) or one `- item` per line.
pub fn front_matter_labels(markdown: &str) -> Vec<String> {
    let mut lines = markdown.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Vec::new();
    }
    let mut labels: Vec<String> = Vec::new();
    let mut push = |raw: &str| {
        let raw = raw.trim().trim_matches(['"', '\'']);
        if let Some(name) = normalize(raw)
            && !labels.contains(&name)
        {
            labels.push(name);
        }
    };
    let mut in_list = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if in_list {
            if let Some(item) = trimmed.strip_prefix("- ") {
                push(item);
                continue;
            }
            in_list = false;
        }
        if let Some(value) = trimmed.strip_prefix("labels:") {
            let value = value.trim();
            if value.is_empty() {
                in_list = true;
            } else {
                for item in value
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                {
                    push(item);
                }
            }
        }
    }
    labels
}

/// Adds `label` (already normalized) to an issue's markdown, where its labels
/// already live: the front matter's `labels:` list when it has one, else the
/// `# H1`, after its leading `[label]` groups. Only that one line changes (or,
/// for a block list, one line is added). `Ok(None)` when the issue already
/// has the label; `Err` when there is no `# ` heading to put it on.
pub fn add_label(markdown: &str, label: &str) -> Result<Option<String>, String> {
    let lines: Vec<&str> = markdown.split_inclusive('\n').collect();
    if let Some((title_labels, _)) = h1(&lines).map(|i| split_leading_labels(heading(lines[i])))
        && title_labels.iter().any(|name| name == label)
    {
        return Ok(None);
    }
    if front_matter_labels(markdown)
        .iter()
        .any(|name| name == label)
    {
        return Ok(None);
    }
    let mut out: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    if let Some(at) = front_matter_list(&lines) {
        match at {
            List::Inline(i) => {
                let (body, end) = split_eol(lines[i]);
                let (key, value) = body.split_once("labels:").unwrap_or((body, ""));
                let items = value
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .trim();
                let items = if items.is_empty() {
                    label.to_string()
                } else {
                    format!("{items}, {label}")
                };
                out[i] = format!("{key}labels: [{items}]{end}");
            }
            List::Block { last, indent } => {
                let (_, end) = split_eol(lines[last]);
                let end = if end.is_empty() { "\n" } else { end };
                if !out[last].ends_with('\n') {
                    out[last].push('\n');
                }
                out.insert(last + 1, format!("{indent}- {label}{end}"));
            }
        }
        return Ok(Some(out.concat()));
    }
    let Some(i) = h1(&lines) else {
        return Err("it has no `# ` title line".to_string());
    };
    let (body, end) = split_eol(lines[i]);
    let indent = &body[..body.len() - body.trim_start().len()];
    let rest = heading(lines[i]);
    // Skip the leading bracket groups exactly as `split_leading_labels` reads them.
    let mut tail = rest.trim_start();
    while let Some(inner) = tail.strip_prefix('[')
        && let Some(close) = inner.find(']')
        && !inner[close + 1..].starts_with('(')
    {
        tail = inner[close + 1..].trim_start();
    }
    let groups = rest[..rest.len() - tail.len()].trim_end();
    let line = if groups.is_empty() {
        format!("{indent}# [{label}] {tail}")
    } else {
        format!("{indent}# {groups} [{label}] {tail}")
    };
    out[i] = format!("{}{end}", line.trim_end());
    Ok(Some(out.concat()))
}

/// Adds `label` to the issue at `path`, re-reading it first so an edit made
/// since the last refresh is kept. `Ok(false)` when it was already there.
pub fn add_label_to_file(path: &Path, label: &str) -> Result<bool, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let markdown =
        std::fs::read_to_string(path).map_err(|err| format!("Couldn't read {name}: {err}"))?;
    match add_label(&markdown, label).map_err(|why| format!("Couldn't label {name}: {why}"))? {
        None => Ok(false),
        Some(updated) => {
            std::fs::write(path, updated).map_err(|err| format!("Couldn't save {name}: {err}"))?;
            Ok(true)
        }
    }
}

/// Removes `label` from an issue's markdown, wherever it carries it: the front
/// matter's `labels:` list and the `# H1`'s leading `[label]` groups. `None`
/// when the issue does not have the label.
pub fn remove_label(markdown: &str, label: &str) -> Option<String> {
    let lines: Vec<&str> = markdown.split_inclusive('\n').collect();
    let mut out: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    let mut remove_lines: Vec<usize> = Vec::new();
    let mut changed = false;

    match front_matter_list(&lines) {
        Some(List::Inline(i)) => {
            let (body, end) = split_eol(lines[i]);
            let (key, value) = body.split_once("labels:").unwrap_or((body, ""));
            let items: Vec<String> = value
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .filter_map(|raw| normalize(raw.trim().trim_matches(['"', '\''])))
                .collect();
            let kept: Vec<&str> = items
                .iter()
                .map(String::as_str)
                .filter(|n| *n != label)
                .collect();
            if kept.len() != items.len() {
                changed = true;
                out[i] = format!("{key}labels: [{}]{end}", kept.join(", "));
            }
        }
        Some(List::Block { last, .. }) => {
            let key = (1..=last)
                .find(|&j| lines[j].trim().starts_with("labels:"))
                .unwrap_or(last);
            for (j, line) in lines.iter().enumerate().take(last + 1).skip(key + 1) {
                let item = line.trim().strip_prefix("- ").unwrap_or("");
                if normalize(item.trim_matches(['"', '\''])).as_deref() == Some(label) {
                    remove_lines.push(j);
                    changed = true;
                }
            }
        }
        None => {}
    }

    if let Some(i) = h1(&lines) {
        let (body, end) = split_eol(lines[i]);
        let indent = &body[..body.len() - body.trim_start().len()];
        let rest = heading(lines[i]);
        // Keep each group's original text; only the removed one goes.
        let mut groups: Vec<String> = Vec::new();
        let mut tail = rest.trim_start();
        while let Some(inner) = tail.strip_prefix('[')
            && let Some(close) = inner.find(']')
            && !inner[close + 1..].starts_with('(')
        {
            groups.push(inner[..close].to_string());
            tail = inner[close + 1..].trim_start();
        }
        let is_label = |group: &String| normalize(group).as_deref() == Some(label);
        if groups.iter().any(is_label) {
            changed = true;
            let kept: Vec<String> = groups
                .into_iter()
                .filter(|group| !is_label(group))
                .map(|group| format!("[{group}]"))
                .collect();
            let prefix = if kept.is_empty() {
                String::new()
            } else {
                format!("{} ", kept.join(" "))
            };
            out[i] = format!("{indent}# {prefix}{}{end}", tail.trim_end());
        }
    }

    if !changed {
        return None;
    }
    for index in remove_lines.into_iter().rev() {
        out.remove(index);
    }
    Some(out.concat())
}

/// Removes `label` from the issue at `path`, re-reading it first so an edit
/// made since the last refresh is kept. `Ok(false)` when it was not there.
pub fn remove_label_to_file(path: &Path, label: &str) -> Result<bool, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let markdown =
        std::fs::read_to_string(path).map_err(|err| format!("Couldn't read {name}: {err}"))?;
    match remove_label(&markdown, label) {
        None => Ok(false),
        Some(updated) => {
            std::fs::write(path, updated).map_err(|err| format!("Couldn't save {name}: {err}"))?;
            Ok(true)
        }
    }
}

/// Renames `from` to `to` on the issue at `path`, keeping where the label
/// lived (front matter list or `# H1` groups). `Ok(false)` when the issue does
/// not carry `from`.
pub fn rename_label_to_file(path: &Path, from: &str, to: &str) -> Result<bool, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let markdown =
        std::fs::read_to_string(path).map_err(|err| format!("Couldn't read {name}: {err}"))?;
    let Some(without) = remove_label(&markdown, from) else {
        return Ok(false);
    };
    let updated = add_label(&without, to).unwrap_or(None).unwrap_or(without);
    std::fs::write(path, updated).map_err(|err| format!("Couldn't save {name}: {err}"))?;
    Ok(true)
}

/// Where a front matter keeps its `labels:` list.
enum List {
    /// `labels: [a, b]` on this line.
    Inline(usize),
    /// `labels:` then `- item` lines; the last item (or the key) and the
    /// items' indent.
    Block { last: usize, indent: String },
}

fn front_matter_list(lines: &[&str]) -> Option<List> {
    if lines.first().map(|line| line.trim()) != Some("---") {
        return None;
    }
    let mut i = 1;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == "---" {
            return None;
        }
        if let Some(value) = trimmed.strip_prefix("labels:") {
            if !value.trim().is_empty() {
                return Some(List::Inline(i));
            }
            let mut last = i;
            let mut indent = "  ".to_string();
            while let Some(line) = lines.get(last + 1)
                && line.trim().starts_with("- ")
            {
                last += 1;
                indent = line[..line.len() - line.trim_start().len()].to_string();
            }
            return Some(List::Block { last, indent });
        }
        i += 1;
    }
    None
}

/// The index of the first `# ` heading line.
fn h1(lines: &[&str]) -> Option<usize> {
    lines.iter().position(|line| line.trim().starts_with("# "))
}

/// The text of a `# ` heading line, after the marker.
fn heading(line: &str) -> &str {
    let (body, _) = split_eol(line);
    body.trim_start()
        .strip_prefix("# ")
        .unwrap_or("")
        .trim_end()
}

/// A line without its line ending, and the ending.
fn split_eol(line: &str) -> (&str, &str) {
    let body = line.trim_end_matches(['\n', '\r']);
    (body, &line[body.len()..])
}

/// The display title of a heading: without its leading `[label]` groups.
pub fn display_title(heading: &str) -> String {
    split_leading_labels(heading).1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_bracket_is_the_label() {
        let (labels, title) = split_leading_labels("[Discovery] Hey look!");
        assert_eq!(labels, ["discovery"]);
        assert_eq!(title, "Hey look!");
    }

    #[test]
    fn the_label_is_case_insensitive_and_lowercased() {
        let (labels, title) = split_leading_labels("[BUG] Fix the crash");
        assert_eq!(labels, ["bug"]);
        assert_eq!(title, "Fix the crash");
    }

    #[test]
    fn every_leading_bracket_counts_and_later_ones_stay() {
        let (labels, title) = split_leading_labels("Hey [there]");
        assert!(labels.is_empty());
        assert_eq!(title, "Hey [there]");
        assert_eq!(
            split_leading_labels("[Bug] [UI] Title"),
            (
                vec!["bug".to_string(), "ui".to_string()],
                "Title".to_string()
            )
        );
        assert_eq!(
            split_leading_labels("[Bug][UI][bug] Support [WIP] markers"),
            (
                vec!["bug".to_string(), "ui".to_string()],
                "Support [WIP] markers".to_string()
            )
        );
    }

    #[test]
    fn labels_are_hyphenated_and_capped() {
        assert_eq!(split_leading_labels("[Tech debt] x").0, ["tech-debt"]);
        let long = split_leading_labels("[abcdefghijklmnopqrstuvwxyz] x").0;
        assert_eq!(long, ["abcdefghijklmnopqrstuvwxy"]);
    }

    #[test]
    fn empty_brackets_and_links_are_not_labels() {
        assert_eq!(
            split_leading_labels("[] nothing"),
            (vec![], "nothing".to_string())
        );
        assert!(split_leading_labels("[home](https://x)").0.is_empty());
        assert_eq!(
            display_title("[home](https://x) page"),
            "[home](https://x) page"
        );
    }

    #[test]
    fn front_matter_labels_read_inline_and_block_lists() {
        assert_eq!(
            front_matter_labels("---\nlabels: [Bug, \"tech debt\"]\n---\n# T\n"),
            ["bug", "tech-debt"]
        );
        assert_eq!(
            front_matter_labels("---\ntitle: x\nlabels:\n  - ui\n  - ops\nowner: me\n---\n"),
            ["ui", "ops"]
        );
        assert!(front_matter_labels("# T\nlabels: [bug]\n").is_empty());
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
        let load = |text: &str| {
            let mut config = UserConfig::parse(text, Path::new("iboard.toml"));
            (Labels::load(&mut config), config.problems)
        };

        // Without `[labels]`, the preset stands.
        let (labels, _) = load("[ai]\ntool = \"opencode\"\n");
        assert_eq!(labels.color("bug"), parse_hex("#b12f22"));

        // With it, a preset is overridden and a name is added.
        let (labels, problems) = load("[labels]\nbug = \"#ff0000\"\nspike = \"#123456\"\n");
        assert_eq!(labels.color("bug"), Some(Color32::from_rgb(255, 0, 0)));
        assert_eq!(labels.color("spike"), Some(Color32::from_rgb(18, 52, 86)));
        assert_eq!(
            labels.color("feature"),
            parse_hex("#1a6e42"),
            "other presets stay"
        );
        assert!(problems.is_empty());

        // A value that is not a colour is skipped, and reported.
        let (labels, problems) = load("[labels]\nbug = \"red\"\nops = \"#123456\"\n");
        assert_eq!(labels.color("bug"), parse_hex("#b12f22"));
        assert_eq!(labels.color("ops"), parse_hex("#123456"));
        assert_eq!(problems.len(), 1);
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
        crate::settings::save(&file, &[section(&table(&entries)).unwrap()]).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains("bug"),
            "a preset left as is is not written: {text}"
        );
        assert!(text.contains("feature = \"#123456\""), "{text}");
        assert!(text.contains("ops = \"#abcdef\""), "{text}");
        assert!(!text.contains("broken"));
        assert!(text.contains("confirm = false"), "other sections stay");
        let labels = Labels::load(&mut UserConfig::load(Some(&file)));
        assert_eq!(labels.color("feature"), parse_hex("#123456"));
        assert_eq!(labels.color("bug"), parse_hex("#b12f22"));
        // The presets left out of `entries` were removed and stay removed.
        assert!(text.contains("chore = \"\""), "{text}");
        assert_eq!(labels.color("chore"), None);
        assert_eq!(labels.color("discovery"), None);
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn a_label_goes_after_the_heading_groups() {
        let add = |md: &str, label: &str| add_label(md, label).unwrap().unwrap();
        assert_eq!(
            add("# [Discovery] Offline mode\n\nBody [x]\n", "bug"),
            "# [Discovery] [bug] Offline mode\n\nBody [x]\n"
        );
        assert_eq!(add("# Fix login\n", "bug"), "# [bug] Fix login\n");
        assert_eq!(add("# [A][B] T", "c"), "# [A][B] [c] T");
        assert_eq!(
            add("intro\n# [home](https://x) page\r\nrest", "ux"),
            "intro\n# [ux] [home](https://x) page\r\nrest"
        );
    }

    #[test]
    fn a_front_matter_list_takes_the_label_and_the_title_stays() {
        let add = |md: &str| add_label(md, "bug").unwrap().unwrap();
        assert_eq!(
            add("---\nlabels: [ux]\n---\n# T\n"),
            "---\nlabels: [ux, bug]\n---\n# T\n"
        );
        assert_eq!(
            add("---\nlabels: []\n---\n# T\n"),
            "---\nlabels: [bug]\n---\n# T\n"
        );
        assert_eq!(
            add("---\nlabels:\n    - ux\nowner: me\n---\n# T\n"),
            "---\nlabels:\n    - ux\n    - bug\nowner: me\n---\n# T\n"
        );
        // Front matter without `labels:` leaves it alone; the title takes it.
        assert_eq!(
            add("---\nowner: me\n---\n# T\n"),
            "---\nowner: me\n---\n# [bug] T\n"
        );
    }

    #[test]
    fn adding_a_label_twice_changes_nothing() {
        assert_eq!(add_label("# [BUG] T\n", "bug"), Ok(None));
        assert_eq!(add_label("---\nlabels: [bug]\n---\n# T\n", "bug"), Ok(None));
        assert!(add_label("no heading\n", "bug").is_err());
    }

    #[test]
    fn a_label_comes_off_the_heading_groups() {
        let remove = |md: &str, label: &str| remove_label(md, label).unwrap();
        assert_eq!(
            remove("# [Discovery] [bug] Offline mode\n", "bug"),
            "# [Discovery] Offline mode\n"
        );
        assert_eq!(remove("# [bug] Fix login\n", "bug"), "# Fix login\n");
        assert_eq!(remove("# [A][B] T", "b"), "# [A] T");
        assert!(remove_label("# Fix login\n", "bug").is_none());
        assert!(remove_label("# [home](https://x) page\n", "home").is_none());
    }

    #[test]
    fn a_label_comes_off_the_front_matter_list() {
        assert_eq!(
            remove_label("---\nlabels: [ux, bug]\n---\n# T\n", "bug").unwrap(),
            "---\nlabels: [ux]\n---\n# T\n"
        );
        assert_eq!(
            remove_label("---\nlabels: [bug]\n---\n# T\n", "bug").unwrap(),
            "---\nlabels: []\n---\n# T\n"
        );
        assert_eq!(
            remove_label(
                "---\nlabels:\n  - ux\n  - bug\nowner: me\n---\n# T\n",
                "bug"
            )
            .unwrap(),
            "---\nlabels:\n  - ux\nowner: me\n---\n# T\n"
        );
        assert!(remove_label("---\nlabels: [ux]\n---\n# T\n", "bug").is_none());
    }

    #[test]
    fn a_label_comes_off_both_places() {
        assert_eq!(
            remove_label("---\nlabels: [bug]\n---\n# [bug] T\n", "bug").unwrap(),
            "---\nlabels: []\n---\n# T\n"
        );
    }

    #[test]
    fn renaming_a_label_keeps_where_it_lived() {
        let dir = std::env::temp_dir().join(format!("iboard-rename-label-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.md");
        std::fs::write(&file, "# [Discovery] [bug] T\n").unwrap();
        assert_eq!(rename_label_to_file(&file, "bug", "defect"), Ok(true));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# [Discovery] [defect] T\n"
        );
        assert_eq!(rename_label_to_file(&file, "bug", "defect"), Ok(false));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn labelling_a_file_rewrites_only_the_title_line() {
        let dir = std::env::temp_dir().join(format!("iboard-add-label-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("fix-login.md");
        std::fs::write(&file, "# Fix login\n\n## Context\n\n- [ ] x\n").unwrap();
        assert_eq!(add_label_to_file(&file, "bug"), Ok(true));
        assert_eq!(add_label_to_file(&file, "bug"), Ok(false));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# [bug] Fix login\n\n## Context\n\n- [ ] x\n"
        );
        assert!(file.exists(), "the file name never changes");
        std::fs::remove_dir_all(&dir).ok();
    }
}
