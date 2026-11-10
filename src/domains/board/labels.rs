//! Adding, removing and renaming labels on an issue file.
//!
//! The names themselves (their normalization, the leading `[label]` groups
//! and the front matter list) live in `kernel::labels`; their colours are a
//! user preference, see `domains/preferences/labels.rs`.

use std::path::Path;

pub use crate::kernel::labels::{
    MAX_LABEL_CHARS, bracket_groups, display_title, front_matter_labels, is_origin, most_used,
    normalize, split_leading_labels,
};

use crate::files;
use crate::kernel::labels::{inline_items, inline_list, item_name};
use crate::markdown;

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

/// The index of the title line, outside the front matter and code.
fn h1(lines: &[&str]) -> Option<usize> {
    markdown::title_line(lines)
}

/// A line without its line ending, and the ending.
fn split_eol(line: &str) -> (&str, &str) {
    let body = line.trim_end_matches(['\n', '\r']);
    (body, &line[body.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(add("# [A] [] T", "c"), "# [A] [c] [] T");
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
        assert_eq!(remove("# [A] [] T", "a"), "# [] T");
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
