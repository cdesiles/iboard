//! Label names: the leading `[label]` groups of an issue's `# H1`, its front
//! matter `labels:` list, and the display title they leave behind.
//!
//! A name is display data and is lowercased (`discovery`); its colour is a
//! user preference, see `domains/preferences/labels.rs`. The order of the
//! theme's `label-*` palette is shared by Design and Preferences, so [`Hue`]
//! lives here.

use std::collections::BTreeMap;

use crate::markdown;

pub const MAX_LABEL_CHARS: usize = 25;

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
/// empty `[]` and a markdown link (`[text](url)`) end the run, as in
/// [`bracket_groups`].
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

/// The leading `[…]` label groups of a heading, as written, and the text
/// after them. A markdown link (`[text](url)`) or a group that is no label
/// (`[]`) ends the run and stays in the text.
pub fn bracket_groups(heading: &str) -> (Vec<&str>, &str) {
    let mut groups = Vec::new();
    let mut tail = heading.trim_start();
    while let Some(inner) = tail.strip_prefix('[')
        && let Some(close) = inner.find(']')
        && !inner[close + 1..].starts_with('(')
        && normalize(&inner[..close]).is_some()
    {
        groups.push(&inner[..close]);
        tail = inner[close + 1..].trim_start();
    }
    (groups, tail)
}

/// A list item's label: unquoted and normalized.
pub(crate) fn item_name(raw: &str) -> Option<String> {
    normalize(raw.trim().trim_matches(['"', '\'']))
}

/// What an inline `[a, b]` list holds between its brackets, as written.
pub(crate) fn inline_list(value: &str) -> &str {
    value.trim().trim_start_matches('[').trim_end_matches(']')
}

/// The items of an inline `[a, b]` list, as written.
pub(crate) fn inline_items(value: &str) -> impl Iterator<Item = &str> {
    inline_list(value).split(',')
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
            (vec![], "[] nothing".to_string())
        );
        assert_eq!(
            split_leading_labels("[bug] [] Fix"),
            (vec!["bug".to_string()], "[] Fix".to_string())
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
}
