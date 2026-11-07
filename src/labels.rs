//! Issue labels: the bracketed tokens at the very start of an issue's `# H1`
//! (`# [Discovery][UI] Hey look!`), plus a front-matter `labels:` list, shown
//! as small chips on the board and in the reader.
//!
//! The name is display data and is lowercased (`discovery`). Its colour is a
//! user preference: iboard's presets follow the theme's `label-*` palette, and
//! the `[labels]` table of the user configuration
//! (`~/.config/iboard/iboard.toml`) overrides or adds a `name = "#rrggbb"`
//! mapping, one value in both themes. An empty value (`bug = ""`) removes a
//! preset. `[label_style] stand_out` lists the labels drawn as tinted pills.
//! The display title drops the labels; the file keeps them, and the
//! LabelPicker adds one by rewriting that single line (see [`add_label`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::files;
use crate::markdown;
use crate::settings::{Section, UserConfig};

/// The labels iboard ships with, applied with zero configuration.
const PRESETS: [(&str, Hue); 5] = [
    ("discovery", Hue::Teal),
    ("bug", Hue::Red),
    ("feature", Hue::Green),
    ("chore", Hue::Grey),
    ("question", Hue::Ochre),
];

/// What stands out with no `[label_style]`.
const STAND_OUT: [&str; 1] = ["bug"];

const LABEL_STYLE: &str = "label_style";

/// A preset's place in the theme's `label-*` palette, which has a light and a
/// dark value for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hue {
    Red,
    Orange,
    Ochre,
    Green,
    Teal,
    Pink,
    Grey,
}

impl Hue {
    /// Every hue, in token order.
    pub const ALL: [Self; 7] = [
        Self::Red,
        Self::Orange,
        Self::Ochre,
        Self::Green,
        Self::Teal,
        Self::Pink,
        Self::Grey,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Ochre => "ochre",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Pink => "pink",
            Self::Grey => "grey",
        }
    }
}

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

/// The `[label_style]` section.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct RawStyle {
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

    /// The presets, overridden and extended by the `[labels]` section, and
    /// `[label_style]`. A value that is not a colour is reported and skipped.
    pub fn load(config: &mut UserConfig) -> Self {
        let table: BTreeMap<String, String> = config.section("labels");
        for (name, value) in &table {
            if !value.trim().is_empty() && parse_hex(value).is_none() {
                config.problem(format!("[labels]: {name} = \"{value}\" is not #rrggbb"));
            }
        }
        let labels = Self::with(&table);
        let style: RawStyle = config.section(LABEL_STYLE);
        match style.stand_out {
            Some(names) => {
                let names = names.iter().filter_map(|name| normalize(name)).collect();
                labels.with_stand_out(names)
            }
            None => labels,
        }
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

/// The `[labels]` section that stores `table`; none at all when it is empty.
pub fn section(table: &BTreeMap<String, String>) -> Result<Section, String> {
    if table.is_empty() {
        Ok(Section::removed("labels"))
    } else {
        Section::new("labels", table)
    }
}

/// The `[label_style]` section that stores `names`; none at all for the
/// default.
pub fn style_section(names: &BTreeSet<String>) -> Result<Section, String> {
    if names.iter().map(String::as_str).eq(STAND_OUT) {
        return Ok(Section::removed(LABEL_STYLE));
    }
    let stand_out = Some(names.iter().cloned().collect());
    Section::new(LABEL_STYLE, &RawStyle { stand_out })
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

/// The longest label kept, in characters.
pub const MAX_LABEL_CHARS: usize = 25;

/// Whether `label` says which project an issue comes from (`repo:app`,
/// `group:client`): set by the workspace, never written in the issue.
pub fn is_origin(label: &str) -> bool {
    label.starts_with("repo:") || label.starts_with("group:")
}

/// A label as iboard shows it: trimmed, lowercased, spaces as hyphens, at most
/// 25 characters (`[Tech debt]` gives `tech-debt`), a project's whole name
/// for an origin label. `None` when empty.
pub fn normalize(name: &str) -> Option<String> {
    let name = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    let limit = if is_origin(&name) {
        usize::MAX
    } else {
        MAX_LABEL_CHARS
    };
    let name: String = name.chars().take(limit).collect();
    (!name.is_empty()).then_some(name)
}

/// The first `limit` of `labels`, lowercased, most used first, ties in
/// alphabetical order.
pub fn most_used<'a>(labels: impl IntoIterator<Item = &'a String>, limit: usize) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for label in labels {
        *counts.entry(label.to_lowercase()).or_default() += 1;
    }
    let mut counted: Vec<(String, usize)> = counts.into_iter().collect();
    counted.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counted
        .into_iter()
        .map(|(label, _)| label)
        .take(limit)
        .collect()
}

/// Splits the leading `[label]` groups off a level-1 heading, returning the
/// labels (normalized, repeats merged) and the display title. `[Bug] [UI] T`
/// and `[Bug][UI] T` both give `bug` and `ui`. Brackets later in the title, an
/// empty `[]` (dropped) and a markdown link (`[text](url)`) end the run.
pub fn split_leading_labels(heading: &str) -> (Vec<String>, String) {
    let (groups, tail) = bracket_groups(heading);
    let mut labels: Vec<String> = Vec::new();
    for group in groups {
        if let Some(name) = normalize(group)
            && !labels.contains(&name)
        {
            labels.push(name);
        }
    }
    (labels, tail.trim().to_string())
}

/// The leading `[…]` groups of a heading, as written, and the text after
/// them. A markdown link (`[text](url)`) ends the run.
pub fn bracket_groups(heading: &str) -> (Vec<&str>, &str) {
    let mut groups = Vec::new();
    let mut tail = heading.trim_start();
    while let Some(inner) = tail.strip_prefix('[')
        && let Some(close) = inner.find(']')
        && !inner[close + 1..].starts_with('(')
    {
        groups.push(&inner[..close]);
        tail = inner[close + 1..].trim_start();
    }
    (groups, tail)
}

/// The `labels:` list of a markdown file's front matter, normalized: either
/// inline (`labels: [bug, ui]`) or one `- item` per line.
pub fn front_matter_labels(markdown: &str) -> Vec<String> {
    let lines: Vec<&str> = markdown.lines().collect();
    let Some(matter) = markdown::front_matter(&lines) else {
        return Vec::new();
    };
    let mut labels: Vec<String> = Vec::new();
    let mut push = |raw: &str| {
        if let Some(name) = item_name(raw)
            && !labels.contains(&name)
        {
            labels.push(name);
        }
    };
    let mut in_list = false;
    for line in &lines[matter] {
        let trimmed = line.trim();
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
                for item in inline_items(value) {
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
fn add_label(markdown: &str, label: &str) -> Result<Option<String>, String> {
    let lines: Vec<&str> = markdown.split_inclusive('\n').collect();
    if let Some((title_labels, _)) =
        h1(&lines).map(|i| split_leading_labels(markdown::heading_text(lines[i])))
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
    if let Some(at) = front_matter_lists(&lines).into_iter().next() {
        add_to_list(&lines, &mut out, at, label);
    } else if let Some(i) = h1(&lines) {
        out[i] = add_to_heading(lines[i], label);
    } else {
        return Err("it has no `# ` title line".to_string());
    }
    Ok(Some(out.concat()))
}

/// Adds `label` to the front matter's `labels:` list, its items kept as
/// written: last in an inline list, on a new line after a block list's items.
fn add_to_list(lines: &[&str], out: &mut Vec<String>, at: List, label: &str) {
    match at {
        List::Inline(i) => {
            let (body, end) = split_eol(lines[i]);
            let (key, value) = body.split_once("labels:").unwrap_or((body, ""));
            let items = inline_list(value).trim();
            let items = if items.is_empty() {
                label.to_string()
            } else {
                format!("{items}, {label}")
            };
            out[i] = format!("{key}labels: [{items}]{end}");
        }
        List::Block { last, indent, .. } => {
            let (_, end) = split_eol(lines[last]);
            let end = if end.is_empty() { "\n" } else { end };
            if !out[last].ends_with('\n') {
                out[last].push('\n');
            }
            out.insert(last + 1, format!("{indent}- {label}{end}"));
        }
    }
}

/// A `# ` title line with `label` added after its leading `[label]` groups.
fn add_to_heading(line: &str, label: &str) -> String {
    let (body, end) = split_eol(line);
    let indent = &body[..body.len() - body.trim_start().len()];
    let rest = markdown::heading_text(line);
    let (_, tail) = bracket_groups(rest);
    let groups = rest[..rest.len() - tail.len()].trim_end();
    let line = if groups.is_empty() {
        format!("{indent}# [{label}] {tail}")
    } else {
        format!("{indent}# {groups} [{label}] {tail}")
    };
    format!("{}{end}", line.trim_end())
}

/// Adds `label` to the issue at `path`, re-reading it first so an edit made
/// since the last refresh is kept. `Ok(false)` when it was already there.
pub fn add_label_to_file(path: &Path, label: &str) -> Result<bool, String> {
    edit_file(path, |markdown| add_label(markdown, label))
}

/// Removes `label` from an issue's markdown, wherever it carries it: the front
/// matter's `labels:` list and the `# H1`'s leading `[label]` groups. `None`
/// when the issue does not have the label.
fn remove_label(markdown: &str, label: &str) -> Option<String> {
    let lines: Vec<&str> = markdown.split_inclusive('\n').collect();
    let mut out: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    let mut changed = false;
    for at in front_matter_lists(&lines) {
        changed |= remove_from_list(&lines, &mut out, at, label);
    }
    if let Some(i) = h1(&lines)
        && let Some(line) = remove_from_heading(lines[i], label)
    {
        out[i] = line;
        changed = true;
    }
    changed.then(|| out.concat())
}

/// Takes `label` off a front matter `labels:` list: rewrites an inline list
/// with its items normalized, empties a block list's item lines. Whether the
/// list had it.
fn remove_from_list(lines: &[&str], out: &mut [String], at: List, label: &str) -> bool {
    match at {
        List::Inline(i) => {
            let (body, end) = split_eol(lines[i]);
            let (key, value) = body.split_once("labels:").unwrap_or((body, ""));
            let items: Vec<String> = inline_items(value).filter_map(item_name).collect();
            let kept: Vec<&str> = items
                .iter()
                .map(String::as_str)
                .filter(|n| *n != label)
                .collect();
            if kept.len() == items.len() {
                return false;
            }
            out[i] = format!("{key}labels: [{}]{end}", kept.join(", "));
            true
        }
        List::Block { key, last, .. } => {
            let mut changed = false;
            for (j, line) in lines.iter().enumerate().take(last + 1).skip(key + 1) {
                let item = line.trim().strip_prefix("- ").unwrap_or("");
                if item_name(item).as_deref() == Some(label) {
                    out[j].clear();
                    changed = true;
                }
            }
            changed
        }
    }
}

/// A `# ` title line without `label`, its other leading `[label]` groups kept
/// as written. `None` when the heading does not carry it.
fn remove_from_heading(line: &str, label: &str) -> Option<String> {
    let (body, end) = split_eol(line);
    let indent = &body[..body.len() - body.trim_start().len()];
    let (groups, tail) = bracket_groups(markdown::heading_text(line));
    let is_label = |group: &&str| normalize(group).as_deref() == Some(label);
    if !groups.iter().any(is_label) {
        return None;
    }
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
    Some(format!("{indent}# {prefix}{}{end}", tail.trim_end()))
}

/// Removes `label` from the issue at `path`, re-reading it first so an edit
/// made since the last refresh is kept. `Ok(false)` when it was not there.
pub fn remove_label_from_file(path: &Path, label: &str) -> Result<bool, String> {
    edit_file(path, |markdown| Ok(remove_label(markdown, label)))
}

/// Renames `from` to `to` on the issue at `path`, keeping where the label
/// lived (front matter list or `# H1` groups). `Ok(false)` when the issue does
/// not carry `from`.
pub fn rename_label_to_file(path: &Path, from: &str, to: &str) -> Result<bool, String> {
    edit_file(path, |markdown| {
        Ok(remove_label(markdown, from)
            .map(|without| add_label(&without, to).unwrap_or(None).unwrap_or(without)))
    })
}

/// Rewrites the issue at `path` with `change`, atomically. `Ok(false)` when
/// `change` leaves it as it is.
fn edit_file(
    path: &Path,
    change: impl FnOnce(&str) -> Result<Option<String>, String>,
) -> Result<bool, String> {
    let name = files::file_name(path);
    let markdown =
        std::fs::read_to_string(path).map_err(|err| format!("Couldn't read {name}: {err}"))?;
    let Some(updated) = change(&markdown).map_err(|why| format!("Couldn't label {name}: {why}"))?
    else {
        return Ok(false);
    };
    files::atomic_write(path, updated).map_err(|err| format!("Couldn't save {name}: {err}"))?;
    Ok(true)
}

/// Where a front matter keeps its `labels:` list.
enum List {
    /// `labels: [a, b]` on this line.
    Inline(usize),
    /// `labels:` on `key`, then `- item` lines; the last item (or the key)
    /// and the items' indent.
    Block {
        key: usize,
        last: usize,
        indent: String,
    },
}

/// Every `labels:` list of the front matter, in order.
fn front_matter_lists(lines: &[&str]) -> Vec<List> {
    let Some(matter) = markdown::front_matter(lines) else {
        return Vec::new();
    };
    let mut lists = Vec::new();
    for i in matter.clone() {
        let Some(value) = lines[i].trim().strip_prefix("labels:") else {
            continue;
        };
        if !value.trim().is_empty() {
            lists.push(List::Inline(i));
            continue;
        }
        let mut last = i;
        let mut indent = "  ".to_string();
        while last + 1 < matter.end
            && let Some(line) = lines.get(last + 1)
            && line.trim().starts_with("- ")
        {
            last += 1;
            indent = line[..line.len() - line.trim_start().len()].to_string();
        }
        lists.push(List::Block {
            key: i,
            last,
            indent,
        });
    }
    lists
}

/// A list item's label: unquoted and normalized.
fn item_name(raw: &str) -> Option<String> {
    normalize(raw.trim().trim_matches(['"', '\'']))
}

/// What an inline `[a, b]` list holds between its brackets, as written.
fn inline_list(value: &str) -> &str {
    value.trim().trim_start_matches('[').trim_end_matches(']')
}

/// The items of an inline `[a, b]` list, as written.
fn inline_items(value: &str) -> impl Iterator<Item = &str> {
    inline_list(value).split(',')
}

/// The index of the title line, outside the front matter and code.
fn h1(lines: &[&str]) -> Option<usize> {
    markdown::title_line(lines)
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
    fn the_most_used_labels_come_first() {
        let used = ["ux", "Bug", "feat", "bug"].map(String::from);
        assert_eq!(most_used(&used, 2), ["bug", "feat"]);
    }

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
        let mut config = UserConfig::parse(text, Path::new("iboard.toml"));
        (Labels::load(&mut config), config.problems)
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

        let (labels, problems) = load("[label_style]\nstand_out = \"bug\"\n");
        assert!(
            labels.style("bug").stand_out,
            "a bad value keeps the default"
        );
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn stand_out_is_stored_only_when_it_differs_from_the_default() {
        let scratch = tempfile::tempdir().unwrap();
        let file = scratch.path().join("iboard.toml");
        let save = |names: &[&str]| {
            let names: BTreeSet<String> = names.iter().map(|name| name.to_string()).collect();
            crate::settings::save(&file, &[style_section(&names).unwrap()]).unwrap();
            let back = Labels::load(&mut UserConfig::load(Some(&file)));
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
        crate::settings::save(&file, &[section(&table(&entries)).unwrap()]).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains("bug") && !text.contains("question"),
            "a preset left as is is not written: {text}"
        );
        assert!(text.contains("feature = \"#123456\""), "{text}");
        assert!(text.contains("ops = \"#abcdef\""), "{text}");
        assert!(!text.contains("broken"));
        assert!(text.contains("confirm = false"), "other sections stay");
        let labels = Labels::load(&mut UserConfig::load(Some(&file)));
        let color = |name| labels.style(name).color;
        assert_eq!(color("feature"), parse_hex("#123456").map(LabelColor::Own));
        assert_eq!(color("bug"), Some(LabelColor::Preset(Hue::Red)));
        // The presets left out of `entries` were removed and stay removed.
        assert!(text.contains("chore = \"\""), "{text}");
        assert_eq!(color("chore"), None);
        assert_eq!(color("discovery"), None);
        assert_eq!(table(&labels.entries()), table(&entries), "round trip");
    }

    #[test]
    fn a_label_never_lands_in_a_code_block() {
        let markdown = "```sh\n# comment\n```\n# Real title\n";
        assert_eq!(
            add_label(markdown, "bug").unwrap().unwrap(),
            "```sh\n# comment\n```\n# [bug] Real title\n"
        );
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
    fn a_label_comes_off_every_front_matter_list_however_spaced() {
        assert_eq!(
            remove_label("---\nlabels:\n  -  \"bug\"\n---\n# T\n", "bug").unwrap(),
            "---\nlabels:\n---\n# T\n"
        );
        let two = "---\nlabels: [ux]\nowner: me\nlabels:\n  - bug\n---\n# T\n";
        assert_eq!(front_matter_labels(two), ["ux", "bug"]);
        assert_eq!(add_label(two, "bug"), Ok(None));
        assert_eq!(
            remove_label(two, "bug").unwrap(),
            "---\nlabels: [ux]\nowner: me\nlabels:\n---\n# T\n"
        );
        assert_eq!(
            add_label(two, "ops").unwrap().unwrap(),
            "---\nlabels: [ux, ops]\nowner: me\nlabels:\n  - bug\n---\n# T\n"
        );
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
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let file = dir.join("t.md");
        std::fs::write(&file, "# [Discovery] [bug] T\n").unwrap();
        assert_eq!(rename_label_to_file(&file, "bug", "defect"), Ok(true));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# [Discovery] [defect] T\n"
        );
        assert_eq!(rename_label_to_file(&file, "bug", "defect"), Ok(false));
    }

    #[test]
    fn labelling_a_file_rewrites_only_the_title_line() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let file = dir.join("fix-login.md");
        std::fs::write(&file, "# Fix login\n\n## Context\n\n- [ ] x\n").unwrap();
        assert_eq!(add_label_to_file(&file, "bug"), Ok(true));
        assert_eq!(add_label_to_file(&file, "bug"), Ok(false));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# [bug] Fix login\n\n## Context\n\n- [ ] x\n"
        );
        assert!(file.exists(), "the file name never changes");
    }
}
