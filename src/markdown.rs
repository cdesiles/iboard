//! The structure iboard reads in an issue's markdown: a front matter opening
//! the file, fenced code blocks, and the `# H1` title. Every reader of these
//! goes through here, so a `# comment` in a code block is never a title.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser};

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
pub fn is_title(line: &str) -> bool {
    line.trim_start().starts_with("# ")
}

/// The byte ranges of the top-level blocks of `text`: paragraphs, headings,
/// whole lists, fences and tables, in order. The reader draws them one by one,
/// so each has a place a comment can hang on.
pub fn blocks(text: &str) -> Vec<Range<usize>> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES;
    let mut depth = 0usize;
    let mut blocks = Vec::new();
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
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
    blocks
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
}
