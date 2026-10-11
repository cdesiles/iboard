//! What the reader sheet says about an issue beyond its text: checklist
//! progress, word count, and which standard sections are still missing.

use crate::issue::Issue;
use crate::labels;
use crate::markdown;

/// The sections the reader asks for when `[reader] sections` is not set.
pub const DEFAULT_SECTIONS: [&str; 4] = [
    "Context",
    "Acceptance criteria",
    "Out of scope",
    "Open questions",
];

/// `(ticked, total)` task-list items (`- [x]`, `* [ ]`, `1. [ ]`), outside
/// fenced code blocks.
pub fn checklist(markdown: &str) -> (usize, usize) {
    let mut done = 0;
    let mut total = 0;
    for line in prose_lines(markdown) {
        let line = line.trim_start();
        let rest = if let Some(rest) = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("+ "))
        {
            rest
        } else {
            let digits = line.chars().take_while(char::is_ascii_digit).count();
            match line[digits..].strip_prefix(". ") {
                Some(rest) if digits > 0 => rest,
                _ => continue,
            }
        };
        if rest.starts_with("[ ]") {
            total += 1;
        } else if rest.starts_with("[x]") || rest.starts_with("[X]") {
            total += 1;
            done += 1;
        }
    }
    (done, total)
}

/// Words in the issue, its title included, outside fenced code blocks.
pub fn word_count(markdown: &str) -> usize {
    prose_lines(markdown)
        .flat_map(str::split_whitespace)
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count()
}

/// The `sections` that have no `## <Section>` heading in the issue, in the
/// order given. Headings match case-insensitively.
pub fn missing_sections<'a>(markdown: &str, sections: &'a [String]) -> Vec<&'a str> {
    let written: Vec<String> = prose_lines(markdown)
        .filter_map(|line| line.trim().strip_prefix("## "))
        .map(|heading| heading.trim().trim_end_matches('#').trim().to_lowercase())
        .collect();
    sections
        .iter()
        .map(String::as_str)
        .filter(|section| !written.contains(&section.to_lowercase()))
        .collect()
}

/// `markdown` with `## <section>` appended after a blank line, followed by an
/// empty line to write in (a `- [ ] ` item for acceptance criteria). The caret
/// belongs at the end.
pub fn append_section(markdown: &str, section: &str) -> String {
    let mut text = markdown.trim_end().to_string();
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str("## ");
    text.push_str(section);
    text.push_str("\n\n");
    if section.eq_ignore_ascii_case("Acceptance criteria") {
        text.push_str("- [ ] ");
    }
    text
}

/// The hint a ghost block gives for a section, or `None` for a custom one.
pub fn section_hint(section: &str) -> Option<&'static str> {
    match section.to_lowercase().as_str() {
        "context" => Some("Why now, and what to know first?"),
        "acceptance criteria" => Some("What must be true when done?"),
        "out of scope" => Some("What will this deliberately not do?"),
        "open questions" => Some("What do we still need to decide?"),
        _ => None,
    }
}

/// The lines outside the front matter and fenced code blocks.
fn prose_lines(markdown: &str) -> impl Iterator<Item = &str> {
    let lines: Vec<&str> = markdown.lines().collect();
    let prose = markdown::prose(&lines);
    lines
        .into_iter()
        .zip(prose)
        .filter_map(|(line, prose)| prose.then_some(line))
}

/// `markdown` as the reader shows it: the first `# H1` without its leading
/// `[label]` groups, which the reader draws as tags. Also returns the line's
/// index and its original text, so an edit made through the rendered view
/// (ticking a checkbox) can be written back with [`restore_line`].
pub fn hide_title_labels(markdown: &str) -> (String, Option<(usize, String)>) {
    let mut lines: Vec<&str> = markdown.split_inclusive('\n').collect();
    let Some(index) = markdown::title_line(&lines) else {
        return (markdown.to_string(), None);
    };
    let original = lines[index];
    let heading = original.trim_start().trim_start_matches("# ");
    let (found, title) = labels::split_leading_labels(heading.trim_end());
    if found.is_empty() || title.is_empty() {
        return (markdown.to_string(), None);
    }
    let ending = &original[original.trim_end_matches(['\r', '\n']).len()..];
    let shown = format!("# {title}{ending}");
    lines[index] = &shown;
    (lines.concat(), Some((index, original.to_string())))
}

/// `markdown` with line `index` put back to `line` (see [`hide_title_labels`]).
pub fn restore_line(markdown: &str, index: usize, line: &str) -> String {
    let mut lines: Vec<&str> = markdown.split_inclusive('\n').collect();
    if let Some(slot) = lines.get_mut(index) {
        *slot = line;
    }
    lines.concat()
}

/// A wide card's facts: each standard section, `true` when it is written (and
/// so gets a check; acceptance criteria carries its checklist count), `false`
/// and `Name —` when it is missing. A bare issue is one "No sections yet".
pub fn facts(issue: &Issue, sections: &[String]) -> Vec<(String, bool)> {
    let missing = missing_sections(&issue.markdown, sections);
    if sections
        .iter()
        .all(|section| missing.contains(&section.as_str()))
    {
        return vec![("No sections yet".to_string(), false)];
    }
    let (done, total) = checklist(&issue.markdown);
    sections
        .iter()
        .map(|section| {
            let present = !missing.contains(&section.as_str());
            if !present {
                (format!("{section} —"), false)
            } else if section.eq_ignore_ascii_case("Acceptance criteria") {
                (format!("Criteria {done}/{total}"), true)
            } else {
                (section.clone(), true)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facts_check_written_sections_and_mark_missing() {
        let sections: Vec<String> = DEFAULT_SECTIONS
            .iter()
            .map(|section| section.to_string())
            .collect();
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let file = dir.join("x.md");
        std::fs::write(
            &file,
            "# T\n\n## Context\n\nWhy.\n\n## Acceptance criteria\n\n- [x] a\n- [ ] b\n",
        )
        .unwrap();
        let issue = Issue::load(&file).unwrap();
        let row = facts(&issue, &sections);
        assert!(row.iter().any(|(text, ok)| text == "Context" && *ok));
        assert!(row.iter().any(|(text, ok)| text == "Criteria 1/2" && *ok));
        assert!(
            row.iter()
                .any(|(text, ok)| text == "Out of scope —" && !*ok)
        );

        std::fs::write(&file, "# Bare\n").unwrap();
        let issue = Issue::load(&file).unwrap();
        assert_eq!(
            facts(&issue, &sections),
            vec![("No sections yet".to_string(), false)]
        );
    }

    #[test]
    fn the_title_labels_hide_and_come_back() {
        let file = "---\nx: 1\n---\n# [Bug][UI] Fix it\n\n- [ ] one\n";
        let (shown, original) = hide_title_labels(file);
        assert_eq!(shown, "---\nx: 1\n---\n# Fix it\n\n- [ ] one\n");
        let (index, line) = original.unwrap();
        let ticked = shown.replace("- [ ]", "- [x]");
        assert_eq!(
            restore_line(&ticked, index, &line),
            "---\nx: 1\n---\n# [Bug][UI] Fix it\n\n- [x] one\n"
        );
        assert_eq!(hide_title_labels("# Plain\n").1, None);
    }

    fn defaults() -> Vec<String> {
        DEFAULT_SECTIONS.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn counts_ticked_and_open_items_outside_code() {
        let md = "# T\n\n- [x] one\n- [ ] two\n* [X] three\n1. [ ] four\n- plain\n\n```\n- [ ] code\n```\n";
        assert_eq!(checklist(md), (2, 4));
        assert_eq!(checklist("# Just a title"), (0, 0));
    }

    #[test]
    fn counts_words_but_not_markers_or_code() {
        assert_eq!(
            word_count("# Fix login\n\nIt - breaks.\n\n```\nlet x = 1;\n```"),
            4
        );
    }

    #[test]
    fn lists_the_sections_not_written_in_order() {
        let sections = defaults();
        let md = "# T\n\n## context\n\nwhy\n\n## Open Questions ##\n\n```\n## Out of scope\n```\n";
        assert_eq!(
            missing_sections(md, &sections),
            vec!["Acceptance criteria", "Out of scope"]
        );
        assert!(missing_sections(md, &[]).is_empty());
    }

    #[test]
    fn appends_a_section_ready_to_write() {
        assert_eq!(
            append_section("# T\n\nBody.\n\n", "Context"),
            "# T\n\nBody.\n\n## Context\n\n"
        );
        assert_eq!(
            append_section("# T", "Acceptance criteria"),
            "# T\n\n## Acceptance criteria\n\n- [ ] "
        );
    }
}
