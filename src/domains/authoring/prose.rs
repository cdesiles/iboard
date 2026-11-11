//! The shape of the markdown the reader lays out itself: headings, paragraphs
//! and lists. Anything else in a block (an image, a table, a fence, a quote)
//! makes [`parse`] give up, and the block goes to the markdown viewer whole.

use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{Event, OffsetIter, Parser, Tag, TagEnd};

use crate::markdown::OPTIONS;

/// A run of inline text with one style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub text: String,
    pub code: bool,
    pub strong: bool,
    pub emphasis: bool,
    pub strikethrough: bool,
    pub link: Option<String>,
}

/// A `- [ ]` box: its state and where the `[ ]` sits in the parsed block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub checked: bool,
    pub marker: Range<usize>,
}

/// One list item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub task: Option<Task>,
    pub blocks: Vec<Prose>,
}

/// A block of prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prose {
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Paragraph(Vec<Span>),
    List {
        start: Option<u64>,
        items: Vec<Item>,
    },
}

/// `block`, one of [`crate::markdown::blocks`], as prose; `None` when it holds
/// something else.
pub fn parse(block: &str) -> Option<Vec<Prose>> {
    let mut events = Parser::new_ext(block, OPTIONS)
        .into_offset_iter()
        .peekable();
    let blocks = blocks(&mut events, &mut None)?;
    events.peek().is_none().then_some(blocks)
}

/// `block` with the task box at `marker` set to `checked`.
pub fn with_task(block: &str, marker: Range<usize>, checked: bool) -> String {
    let mut block = block.to_string();
    block.replace_range(marker, if checked { "[x]" } else { "[ ]" });
    block
}

type Events<'a> = Peekable<OffsetIter<'a>>;

/// The blocks up to the end of the enclosing tag, which it leaves unread.
fn blocks(events: &mut Events, task: &mut Option<Task>) -> Option<Vec<Prose>> {
    let mut blocks = Vec::new();
    while let Some((event, _)) = events.peek() {
        match event {
            Event::End(_) => break,
            Event::Start(Tag::Paragraph) => {
                events.next();
                blocks.push(Prose::Paragraph(spans(events, task)));
                closes(events, |end| matches!(end, TagEnd::Paragraph))?;
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let level = *level as u8;
                events.next();
                let spans = spans(events, task);
                blocks.push(Prose::Heading { level, spans });
                closes(events, |end| matches!(end, TagEnd::Heading(_)))?;
            }
            Event::Start(Tag::List(start)) => {
                let start = *start;
                events.next();
                let items = items(events)?;
                blocks.push(Prose::List { start, items });
                closes(events, |end| matches!(end, TagEnd::List(_)))?;
            }
            // A tight list item holds its text with no paragraph around it.
            _ if starts_inline(event) => {
                let spans = spans(events, task);
                blocks.push(Prose::Paragraph(spans));
            }
            _ => return None,
        }
    }
    Some(blocks)
}

fn items(events: &mut Events) -> Option<Vec<Item>> {
    let mut items = Vec::new();
    while matches!(events.peek(), Some((Event::Start(Tag::Item), _))) {
        events.next();
        let mut task = None;
        let blocks = blocks(events, &mut task)?;
        closes(events, |end| matches!(end, TagEnd::Item))?;
        items.push(Item { task, blocks });
    }
    Some(items)
}

fn closes(events: &mut Events, expected: impl Fn(&TagEnd) -> bool) -> Option<()> {
    match events.next() {
        Some((Event::End(end), _)) if expected(&end) => Some(()),
        _ => None,
    }
}

fn starts_inline(event: &Event) -> bool {
    matches!(
        event,
        Event::Text(_)
            | Event::Code(_)
            | Event::InlineHtml(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::TaskListMarker(_)
            | Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. })
    )
}

/// The inline run that starts here, up to the first event that is not inline,
/// which leaves [`blocks`] to give up on it.
fn spans(events: &mut Events, task: &mut Option<Task>) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut style = Span::default();
    while let Some((event, range)) = events.peek() {
        let mut text = |text: &str, code: bool| {
            spans.push(Span {
                text: text.to_string(),
                code,
                ..style.clone()
            });
        };
        match event {
            Event::Text(t) | Event::InlineHtml(t) => text(t, false),
            Event::Code(t) => text(t, true),
            Event::SoftBreak => text(" ", false),
            Event::HardBreak => text("\n", false),
            Event::TaskListMarker(checked) => {
                *task = Some(Task {
                    checked: *checked,
                    marker: range.clone(),
                });
            }
            Event::Start(Tag::Emphasis) => style.emphasis = true,
            Event::End(TagEnd::Emphasis) => style.emphasis = false,
            Event::Start(Tag::Strong) => style.strong = true,
            Event::End(TagEnd::Strong) => style.strong = false,
            Event::Start(Tag::Strikethrough) => style.strikethrough = true,
            Event::End(TagEnd::Strikethrough) => style.strikethrough = false,
            Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => style.link = None,
            _ => break,
        }
        events.next();
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Span {
        Span {
            text: text.to_string(),
            ..Span::default()
        }
    }

    #[test]
    fn a_heading_keeps_its_level_and_inline_style() {
        let parsed = parse("## Keep `this`").unwrap();
        let code = Span {
            text: "this".to_string(),
            code: true,
            ..Span::default()
        };
        assert_eq!(
            parsed,
            [Prose::Heading {
                level: 2,
                spans: vec![plain("Keep "), code],
            }]
        );
    }

    #[test]
    fn a_soft_break_is_a_space_and_styles_end_where_they_close() {
        let parsed = parse("one\n**two** [three](https://x.y)").unwrap();
        let Prose::Paragraph(spans) = &parsed[0] else {
            panic!("{parsed:?}");
        };
        assert_eq!(spans[1], plain(" "));
        assert!(spans[2].strong);
        assert!(!spans[3].strong);
        assert_eq!(spans[4].link.as_deref(), Some("https://x.y"));
    }

    #[test]
    fn tight_and_loose_task_items_find_their_box() {
        for block in ["- [ ] one\n- [x] two\n", "- [ ] one\n\n- [x] two\n"] {
            let parsed = parse(block).unwrap();
            let Prose::List { items, start: None } = &parsed[0] else {
                panic!("{parsed:?}");
            };
            let tasks: Vec<_> = items
                .iter()
                .map(|item| item.task.as_ref().unwrap().checked)
                .collect();
            assert_eq!(tasks, [false, true], "{block}");
            assert_eq!(&block[items[1].task.clone().unwrap().marker], "[x]");
            assert_eq!(items[0].blocks, [Prose::Paragraph(vec![plain("one")])]);
        }
    }

    #[test]
    fn a_nested_list_stays_inside_its_item_and_numbers_keep_their_start() {
        let parsed = parse("3. one\n   - two\n").unwrap();
        let Prose::List {
            start: Some(3),
            items,
        } = &parsed[0]
        else {
            panic!("{parsed:?}");
        };
        assert!(matches!(
            items[0].blocks[1],
            Prose::List { start: None, .. }
        ));
    }

    #[test]
    fn ticking_a_box_changes_only_that_box() {
        let block = "- [ ] one\n- [ ] two\n";
        let parsed = parse(block).unwrap();
        let Prose::List { items, .. } = &parsed[0] else {
            panic!("{parsed:?}");
        };
        let marker = items[1].task.clone().unwrap().marker;
        assert_eq!(with_task(block, marker, true), "- [ ] one\n- [x] two\n");
    }

    #[test]
    fn anything_else_goes_to_the_viewer() {
        for block in [
            "![shot](a.png)",
            "[![shot](a.png)](https://x.y)",
            "- see ![shot](a.png)",
            "- one\n\n  ```\n  code\n  ```\n",
            "> quoted",
            "```\ncode\n```",
            "| a |\n|---|\n| b |\n",
            "---",
            "<div>html</div>",
            "see[^1]\n\n[^1]: note",
        ] {
            assert_eq!(parse(block), None, "{block}");
        }
    }
}
