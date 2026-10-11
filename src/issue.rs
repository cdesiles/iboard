//! A single issue (one markdown file) and the heuristics used to
//! derive a display title and a short summary from it.
//!
//! Templates are intentionally inconsistent across projects, so nothing here is
//! strict: we fall back from a top-level heading to the first non-empty line,
//! then to a prettified file name.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::labels;

/// Maximum number of characters shown in a card summary (before the ellipsis).
const SUMMARY_LIMIT: usize = 180;

#[derive(Debug, Clone)]
pub struct Issue {
    pub path: PathBuf,
    /// The display title, without its leading `[label]` groups.
    pub title: String,
    /// The labels: the `# H1`'s leading `[Brackets]`, then the front matter's
    /// `labels:`, normalized, merged and sorted alphabetically. Empty when
    /// there is none.
    pub labels: Vec<String>,
    pub summary: String,
    /// Full raw markdown, rendered in the detail view.
    pub markdown: String,
    pub modified: Option<SystemTime>,
    /// When the file was created, when the filesystem can say (ColumnFocus's
    /// "Recently created"). Falls back to `modified` sorts.
    pub created: Option<SystemTime>,
    /// When the issue was done; only set in the Done column.
    pub done_at: Option<DateTime<Local>>,
}

impl Issue {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let markdown = fs::read_to_string(path)?;
        let (title, mut labels, title_line) = extract_title(&markdown, path);
        for name in labels::front_matter_labels(&markdown) {
            if !labels.contains(&name) {
                labels.push(name);
            }
        }
        // Show a ticket's labels in a stable, alphabetical order.
        labels.sort();
        let summary = extract_summary(&markdown, title_line);
        let metadata = fs::metadata(path).ok();
        let modified = metadata.as_ref().and_then(|m| m.modified().ok());
        let created = metadata.as_ref().and_then(|m| m.created().ok());
        Ok(Self {
            path: path.to_path_buf(),
            title,
            labels,
            summary,
            markdown,
            modified,
            created,
            done_at: None,
        })
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string()
    }

    /// Case-insensitive match against the title, summary and file name.
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query = query.to_lowercase();
        self.title.to_lowercase().contains(&query)
            || self.summary.to_lowercase().contains(&query)
            || self.file_name().to_lowercase().contains(&query)
    }
}

/// Returns the display title, its labels and the line index it came from (so
/// the summary can skip that line and avoid repeating it).
fn extract_title(content: &str, path: &Path) -> (String, Vec<String>, Option<usize>) {
    // Prefer the first level-1 heading. Its leading `[label]` groups are split
    // off before the inline markup is stripped.
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let (labels, display) = labels::split_leading_labels(rest.trim());
            let heading = strip_inline(&display);
            if !heading.is_empty() {
                return (heading, labels, Some(idx));
            }
        }
    }
    // Otherwise the first non-empty line.
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cleaned = strip_inline(trimmed.trim_start_matches('#').trim());
        if !cleaned.is_empty() {
            return (cleaned, Vec::new(), Some(idx));
        }
    }
    (pretty_stem(path), Vec::new(), None)
}

/// Builds a one-paragraph plain-text summary of the issue body.
fn extract_summary(content: &str, title_line: Option<usize>) -> String {
    let start = title_line.map(|i| i + 1).unwrap_or(0);
    let mut out = String::new();
    let mut in_fence = false;

    for line in content.lines().skip(start) {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if trimmed.is_empty() {
            if !out.is_empty() {
                break;
            }
            continue;
        }
        if trimmed.starts_with('#') || is_horizontal_rule(trimmed) {
            if out.is_empty() {
                continue;
            }
            break;
        }

        // A list item ends a prose summary; if there is no prose yet, the first
        // item is the best summary we have.
        if is_list_line(trimmed) {
            if out.is_empty() {
                out.push_str(&clean_line(trimmed));
            }
            break;
        }

        let cleaned = clean_line(trimmed);
        if cleaned.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&cleaned);
        if out.chars().count() >= SUMMARY_LIMIT {
            break;
        }
    }

    truncate_with_ellipsis(out.trim(), SUMMARY_LIMIT)
}

/// Strips blockquote and list markers, then inline markdown, from a source line.
fn clean_line(line: &str) -> String {
    let mut s = line.trim();
    while let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }
    s = strip_list_marker(s).trim_start();
    strip_inline(s)
}

fn strip_list_marker(line: &str) -> &str {
    let t = line.trim_start();
    for marker in ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(marker) {
            return rest;
        }
    }
    // Ordered lists: "1. " or "1) ".
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let rest = &t[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return rest;
        }
    }
    t
}

/// True when the line starts with a bullet or ordered-list marker.
fn is_list_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    strip_list_marker(trimmed) != trimmed
}

/// Removes emphasis markers, inline code and links/images, keeping their text.
fn strip_inline(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '`' | '*' | '_' | '~' => {}
            '!' => {
                // Only drop the bang when it precedes a link (image syntax).
                if chars.peek() != Some(&'[') {
                    out.push(c);
                }
            }
            '[' => {
                let mut text = String::new();
                let mut depth = 1;
                for cc in chars.by_ref() {
                    match cc {
                        '[' => {
                            depth += 1;
                            text.push(cc);
                        }
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            text.push(cc);
                        }
                        _ => text.push(cc),
                    }
                }
                // Drop a following "(url)" target, if any.
                if chars.peek() == Some(&'(') {
                    chars.next();
                    let mut depth = 1;
                    for cc in chars.by_ref() {
                        match cc {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                out.push_str(&text);
            }
            _ => out.push(c),
        }
    }
    out
}

fn is_horizontal_rule(line: &str) -> bool {
    let mut seen = 0;
    let mut marker = None;
    for c in line.chars() {
        if c == ' ' || c == '\t' {
            continue;
        }
        if c != '-' && c != '*' && c != '_' {
            return false;
        }
        match marker {
            None => marker = Some(c),
            Some(m) if m != c => return false,
            _ => {}
        }
        seen += 1;
    }
    seen >= 3
}

/// Turns `2026-05-29-merge-nut-exporter` into `Merge nut exporter`.
fn pretty_stem(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");

    let mut s = stem;
    loop {
        let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            break;
        }
        let rest = &s[digits..];
        if let Some(rest) = rest.strip_prefix('-').or_else(|| rest.strip_prefix('_')) {
            s = rest;
        } else {
            break;
        }
    }

    let spaced = s.replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Untitled".to_string(),
    }
}

fn truncate_with_ellipsis(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let truncated: String = text.chars().take(limit).collect();
    format!("{}...", truncated.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn heading_wins_over_first_line() {
        let (title, label, line) = extract_title(
            "# 10 — Validate on a real card\n\nNeeds: a card\n",
            &PathBuf::from("x.md"),
        );
        assert_eq!(title, "10 — Validate on a real card");
        assert!(label.is_empty());
        assert_eq!(line, Some(0));
        let summary = extract_summary(
            "# 10 — Validate on a real card\n\nNeeds: a card\n- [ ] do it\n",
            line,
        );
        assert_eq!(summary, "Needs: a card");
    }

    #[test]
    fn a_leading_label_is_split_off_the_heading() {
        let (title, label, line) = extract_title(
            "# [Discovery] Hey look!\n\nThe body.\n",
            &PathBuf::from("x.md"),
        );
        assert_eq!(title, "Hey look!");
        assert_eq!(label, ["discovery"]);
        assert_eq!(line, Some(0));
        let summary = extract_summary("# [Discovery] Hey look!\n\nThe body.\n", line);
        assert_eq!(summary, "The body.");
    }

    #[test]
    fn a_mid_heading_bracket_is_not_a_label() {
        // Not a label; the existing inline stripping still drops the brackets
        // and keeps the words (`split_leading_labels` leaves it literal).
        let (title, label, _) =
            extract_title("# Hey [there] you\n\nBody\n", &PathBuf::from("x.md"));
        assert_eq!(title, "Hey there you");
        assert!(label.is_empty());
    }

    #[test]
    fn plain_text_falls_back_to_first_line() {
        let content = "console.jokester.fr\nand build something on top\n";
        let (title, label, line) =
            extract_title(content, &PathBuf::from("2026-08-23-dashboard.md"));
        assert_eq!(title, "console.jokester.fr");
        assert!(label.is_empty());
        assert_eq!(summary_for(content, line), "and build something on top");
    }

    #[test]
    fn filename_fallback_is_pretty() {
        let (title, _, _) = extract_title("", &PathBuf::from("2026-05-29-merge-nut-exporter.md"));
        assert_eq!(title, "Merge nut exporter");
    }

    fn summary_for(content: &str, line: Option<usize>) -> String {
        extract_summary(content, line)
    }

    #[test]
    fn labels_merge_and_sort_alphabetically() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let file = dir.join("t.md");
        std::fs::write(&file, "---\nlabels: [zebra, apple]\n---\n# [mango] T\n").unwrap();
        let issue = Issue::load(&file).unwrap();
        assert_eq!(issue.labels, ["apple", "mango", "zebra"]);
    }

    #[test]
    fn strips_inline_markup() {
        assert_eq!(
            strip_inline("**bold** and `code` and [link](http://x)"),
            "bold and code and link"
        );
        assert_eq!(strip_inline("![alt](img.png) rest"), "alt rest");
    }

    #[test]
    fn truncates_with_three_dots() {
        let long = "a".repeat(300);
        let out = truncate_with_ellipsis(&long, 10);
        assert_eq!(out, "aaaaaaaaaa...");
    }
}
