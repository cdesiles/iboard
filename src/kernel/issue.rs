//! A single issue (one markdown file) and the heuristics used to
//! derive a display title and a short summary from it.
//!
//! Templates are intentionally inconsistent across projects, so nothing here is
//! strict: we fall back from a top-level heading to the first non-empty line,
//! then to a prettified file name.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::files;
use crate::kernel::labels;
use crate::markdown;

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
        Ok(Self {
            path: path.to_path_buf(),
            title,
            labels,
            summary,
            markdown,
            modified,
        })
    }

    pub fn file_name(&self) -> String {
        crate::files::file_name(&self.path)
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
    let lines: Vec<&str> = content.lines().collect();
    let prose = markdown::prose(&lines);
    if let Some(idx) = markdown::title_line(&lines) {
        let (labels, display) = labels::split_leading_labels(markdown::heading_text(lines[idx]));
        let heading = markdown::strip_inline(&display);
        let title = if heading.is_empty() {
            pretty_stem(path)
        } else {
            heading
        };
        return (title, labels, Some(idx));
    }
    for (idx, line) in lines.iter().enumerate().filter(|(idx, _)| prose[*idx]) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cleaned = markdown::strip_inline(trimmed.trim_start_matches('#').trim());
        if !cleaned.is_empty() {
            return (cleaned, Vec::new(), Some(idx));
        }
    }
    (pretty_stem(path), Vec::new(), None)
}

/// Builds a one-paragraph plain-text summary of the issue body.
fn extract_summary(content: &str, title_line: Option<usize>) -> String {
    let start = title_line.map(|i| i + 1).unwrap_or(0);
    let lines: Vec<&str> = content.lines().collect();
    let prose = markdown::prose(&lines);
    let mut out = String::new();

    for (index, line) in lines.iter().enumerate().skip(start) {
        if !prose[index] {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !out.is_empty() {
                break;
            }
            continue;
        }
        if trimmed.starts_with('#') || markdown::is_horizontal_rule(trimmed) {
            if out.is_empty() {
                continue;
            }
            break;
        }

        // A list item ends a prose summary; if there is no prose yet, the first
        // item is the best summary we have.
        if markdown::list_marker(trimmed).is_some() {
            if out.is_empty() {
                out.push_str(&markdown::clean_line(trimmed));
            }
            break;
        }

        let cleaned = markdown::clean_line(trimmed);
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

/// The file name the composer writes a new issue titled `title` to in
/// `dir`, its `[label]` groups left out, and whether that slug was taken, so
/// a `-2`, `-3`... suffix was added.
pub fn new_file_name(dir: &Path, title: &str) -> (String, bool) {
    let plain = format!("{}.md", files::slugify(&labels::display_title(title)));
    let name = files::unique_name(dir, &plain);
    let taken = name != plain;
    (name, taken)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn titles_skip_code_and_front_matter() {
        let path = Path::new("x.md");
        let title = |text: &str| extract_title(text, path).0;
        assert_eq!(title("```sh\n# not a title\n```\n\nBody.\n"), "Body.");
        assert_eq!(title("---\nlabels: bug\n---\n\nSome body.\n"), "Some body.");
        assert_eq!(title("~~~\n# no\n~~~\n# Yes\n"), "Yes");
        let content = "---\nlabels: bug\n---\n# T\n\nWhy it matters.\n";
        let (_, _, line) = extract_title(content, path);
        assert_eq!(extract_summary(content, line), "Why it matters.");
    }

    #[test]
    fn new_file_names_take_a_suffix_when_taken() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        assert_eq!(
            new_file_name(dir, "Fix it"),
            ("fix-it.md".to_string(), false)
        );
        assert_eq!(
            new_file_name(dir, "[Bug] [UI] Fix it"),
            ("fix-it.md".to_string(), false),
            "labels stay out of the file name"
        );
        std::fs::write(dir.join("fix-it.md"), "").unwrap();
        assert_eq!(
            new_file_name(dir, "Fix it"),
            ("fix-it-2.md".to_string(), true)
        );
        std::fs::write(dir.join("fix-it-2.md"), "").unwrap();
        assert_eq!(
            new_file_name(dir, "Fix it"),
            ("fix-it-3.md".to_string(), true)
        );
    }

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
    fn the_card_reads_the_heading_the_label_picker_edits() {
        let (title, label, line) = extract_title(
            "# [bug]\n\n# Real\n",
            &PathBuf::from("2026-01-02-fix-it.md"),
        );
        assert_eq!(
            (title.as_str(), label, line),
            ("Fix it", vec!["bug".to_string()], Some(0))
        );
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
    fn truncates_with_three_dots() {
        let long = "a".repeat(300);
        let out = truncate_with_ellipsis(&long, 10);
        assert_eq!(out, "aaaaaaaaaa...");
    }
}
