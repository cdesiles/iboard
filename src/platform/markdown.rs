//! The structure iboard reads in an issue's markdown: a front matter opening
//! the file, fenced code blocks, the `# H1` title and, within a line, a
//! heading's text, a list marker and inline markup. Every reader of these
//! goes through here, so a `# comment` in a code block is never a title.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag};

/// The extensions iboard parses an issue with.
pub const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_FOOTNOTES);

/// The lines between the `---` that opens a file and the `---` that closes
/// that block, as indices into `lines`. `None` when there is none, or when it
/// is never closed.
pub fn front_matter(lines: &[&str]) -> Option<Range<usize>> {
    if lines.first().map(|line| line.trim()) != Some("---") {
        return None;
    }
    let end = lines.iter().skip(1).position(|line| line.trim() == "---")? + 1;
    Some(1..end)
}

/// Whether the issue says nothing past its front matter and its `# ` title.
pub fn only_a_title(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    let body = front_matter(&lines).map_or(0, |matter| matter.end + 1);
    let mut said = lines[body..].iter().filter(|line| !line.trim().is_empty());
    said.next().is_none_or(|line| line.starts_with("# ")) && said.next().is_none()
}

/// Whether each line is prose: not the front matter, its delimiters included,
/// and not a fenced code block (```` ``` ```` or `~~~`), its fences included.
pub fn prose(lines: &[&str]) -> Vec<bool> {
    let mut prose = vec![true; lines.len()];
    let mut start = 0;
    if let Some(matter) = front_matter(lines) {
        prose[..=matter.end].fill(false);
        start = matter.end + 1;
    }
    let mut fence: Option<&str> = None;
    for (index, line) in lines.iter().enumerate().skip(start) {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, Some(marker)) => {
                fence = Some(marker);
                prose[index] = false;
            }
            (Some(open), Some(marker)) if open == marker => {
                fence = None;
                prose[index] = false;
            }
            (Some(_), _) => prose[index] = false,
            (None, None) => {}
        }
    }
    prose
}

/// The index of the title: the first `# ` heading in prose.
pub fn title_line(lines: &[&str]) -> Option<usize> {
    let prose = prose(lines);
    (0..lines.len()).find(|&index| prose[index] && is_title(lines[index]))
}

/// Whether `line` is a `# ` heading.
fn is_title(line: &str) -> bool {
    line.trim_start().starts_with("# ")
}

/// The text of a `# ` heading line, after the marker; empty for any other
/// line.
pub fn heading_text(line: &str) -> &str {
    line.trim_start()
        .strip_prefix("# ")
        .unwrap_or("")
        .trim_end()
}

/// The byte ranges of the top-level blocks of `text`: paragraphs, headings,
/// whole lists, fences and tables, in order. The reader draws them one by one,
/// so each has a place a comment can hang on.
pub fn blocks(text: &str) -> Vec<Range<usize>> {
    let matter = front_matter_len(text);
    let mut depth = 0usize;
    let mut blocks = Vec::new();
    for (event, range) in Parser::new_ext(text, OPTIONS).into_offset_iter() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    blocks.push(range);
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ if depth == 0 => blocks.push(range),
            _ => {}
        }
    }
    blocks.retain(|block| block.start >= matter);
    blocks
}

/// Whether the markdown `block` (one of [`blocks`]) is a table.
pub fn is_table(block: &str) -> bool {
    let mut events = Parser::new_ext(block, Options::ENABLE_TABLES);
    matches!(events.next(), Some(Event::Start(Tag::Table(_))))
}

/// How many bytes of `text` its front matter takes, both `---` lines
/// included; 0 when there is none (see [`front_matter`]).
fn front_matter_len(text: &str) -> usize {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    match front_matter(&lines) {
        Some(matter) => lines[..=matter.end].iter().map(|line| line.len()).sum(),
        None => 0,
    }
}

/// A list line's marker (`-`, `*`, `+`, `1.` or `1)`) and the text after it.
/// `None` when `line` is no list item.
pub fn list_marker(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return Some((&line[..1], rest));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let after = &line[digits..];
    let rest = after
        .strip_prefix(". ")
        .or_else(|| after.strip_prefix(") "))?;
    (digits > 0).then_some((&line[..=digits], rest))
}

/// Strips blockquote and list markers, then inline markdown, from a source line.
pub fn clean_line(line: &str) -> String {
    let mut s = line.trim();
    while let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }
    s = strip_list_marker(s).trim_start();
    strip_inline(s)
}

/// `line` without its list marker, and without the box of a `- [ ]` task.
fn strip_list_marker(line: &str) -> &str {
    let line = line.trim_start();
    match list_marker(line) {
        Some(("-", rest)) => ["[ ] ", "[x] ", "[X] "]
            .into_iter()
            .find_map(|task| rest.strip_prefix(task))
            .unwrap_or(rest),
        Some((_, rest)) => rest,
        None => line,
    }
}

/// Removes emphasis markers, inline code and links/images, keeping their text.
pub fn strip_inline(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                if let Some(&next) = chars.get(i + 1) {
                    out.push(next);
                    i += 1;
                }
            }
            // A code span keeps its text as written: `parse_color` stays.
            '`' => {
                if let Some(len) = chars[i + 1..].iter().position(|&cc| cc == '`') {
                    out.extend(&chars[i + 1..i + 1 + len]);
                    i += len + 1;
                }
            }
            '*' => {}
            // Emphasis, unless inside a word: `snake_case` stays.
            '_' => {
                let inside_word = i > 0
                    && chars[i - 1].is_alphanumeric()
                    && chars.get(i + 1).is_some_and(|next| next.is_alphanumeric());
                if inside_word {
                    out.push('_');
                }
            }
            // `~~` is strikethrough; a single `~` stays, as in `~/notes`.
            '~' => {
                if chars.get(i + 1) == Some(&'~') {
                    i += 1;
                } else {
                    out.push('~');
                }
            }
            '!' => {
                // Only drop the bang when it precedes a link (image syntax).
                if chars.get(i + 1) != Some(&'[') {
                    out.push(c);
                }
            }
            '[' => {
                let mut text = String::new();
                let mut depth = 1;
                i += 1;
                while i < chars.len() {
                    let cc = chars[i];
                    if cc == '[' {
                        depth += 1;
                    } else if cc == ']' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    text.push(cc);
                    i += 1;
                }
                // Drop a following "(url)" target, if any.
                if chars.get(i + 1) == Some(&'(') {
                    i += 2;
                    let mut depth = 1;
                    while i < chars.len() {
                        match chars[i] {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                }
                out.push_str(&strip_inline(&text));
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// Whether `line` is a `---`, `***` or `___` rule, spaced or not.
pub fn is_horizontal_rule(line: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<&str> {
        text.split_inclusive('\n').collect()
    }

    #[test]
    fn a_front_matter_is_the_closed_block_opening_the_file() {
        assert_eq!(
            front_matter(&lines("---\nlabels: [a]\n---\n# T\n")),
            Some(1..2)
        );
        assert_eq!(
            front_matter(&lines("---\nlabels: [a]\n# never closed\n")),
            None
        );
        assert_eq!(front_matter(&lines("# T\n---\nx\n---\n")), None);
    }

    #[test]
    fn a_table_block_is_told_apart() {
        let text = "# T\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\npipe | in prose\n";
        let tables: Vec<bool> = blocks(text)
            .into_iter()
            .map(|b| is_table(&text[b]))
            .collect();
        assert_eq!(tables, [false, true, false]);
    }

    #[test]
    fn the_front_matter_is_no_block() {
        let text = "---\ntitre: T\nstatut: En cours\n---\n\n# T\n\nBody\n";
        let shown: Vec<&str> = blocks(text).into_iter().map(|b| text[b].trim()).collect();
        assert_eq!(shown, ["# T", "Body"]);
        assert_eq!(blocks("# T\n---\nx\n").len(), 3, "a later rule stays");
    }

    #[test]
    fn code_and_front_matter_are_not_prose() {
        let text = "---\nlabels: [a]\n---\n# T\n```sh\n# comment\n~~~\n```\n~~~\n```\n~~~\nafter\n";
        let flags = prose(&lines(text));
        assert_eq!(
            flags,
            [
                false, false, false, true, false, false, false, false, false, false, false, true
            ]
        );
    }

    #[test]
    fn the_title_is_the_first_heading_in_prose() {
        assert_eq!(
            title_line(&lines("```\n# not this\n```\n# This\n")),
            Some(3)
        );
        assert_eq!(title_line(&lines("---\n# not this\n---\nbody\n")), None);
        assert_eq!(title_line(&lines("## Sub\n#NoSpace\n# Yes\n")), Some(2));
    }

    #[test]
    fn a_heading_s_text_is_what_follows_its_marker() {
        assert_eq!(heading_text("  # [bug] Fix  \r\n"), "[bug] Fix");
        assert_eq!(heading_text("# # x"), "# x");
        assert_eq!(heading_text("## Sub"), "");
    }

    #[test]
    fn a_list_marker_comes_with_the_text_after_it() {
        assert_eq!(list_marker("  - [ ] a"), Some(("-", "[ ] a")));
        assert_eq!(list_marker("12) b"), Some(("12)", "b")));
        assert_eq!(list_marker("1.b"), None);
        assert_eq!(list_marker("-b"), None);
    }

    #[test]
    fn blocks_keep_lists_and_fences_whole() {
        let text = "# Title\n\nOne paragraph\nwrapped.\n\n- a\n\n- b\n\n```\nx\n\ny\n```\n\n---\n";
        let blocks: Vec<&str> = blocks(text)
            .into_iter()
            .map(|range| text[range].trim_end())
            .collect();
        assert_eq!(
            blocks,
            [
                "# Title",
                "One paragraph\nwrapped.",
                "- a\n\n- b",
                "```\nx\n\ny\n```",
                "---"
            ]
        );
    }

    #[test]
    fn inline_markup_goes_but_code_and_words_stay() {
        assert_eq!(
            strip_inline("Fix `parse_color` for *dark* themes"),
            "Fix parse_color for dark themes"
        );
        assert_eq!(
            strip_inline("Rename snake_case files, _not_ this"),
            "Rename snake_case files, not this"
        );
        assert_eq!(
            strip_inline("Moved to ~/notes, ~~old~~ gone"),
            "Moved to ~/notes, old gone"
        );
        assert_eq!(
            strip_inline("See [**the** doc](https://x) and ![shot](a.png)"),
            "See the doc and shot"
        );
    }

    #[test]
    fn strips_inline_markup() {
        assert_eq!(
            strip_inline("**bold** and `code` and [link](http://x)"),
            "bold and code and link"
        );
        assert_eq!(strip_inline("![alt](img.png) rest"), "alt rest");
    }
}
