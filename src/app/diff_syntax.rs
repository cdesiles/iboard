//! A diff's syntax colours: each side of a hunk highlighted as one stream, so
//! a comment or a string opened on one line still colours the next ones.

use std::ops::Range;
use std::sync::OnceLock;

use egui::Color32;
use iboard::diff::{FileDiff, LineKind};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// One coloured run of a line's text, in bytes.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Span {
    pub(super) bytes: Range<usize>,
    pub(super) color: Color32,
    pub(super) italics: bool,
}

/// A file's colours, by hunk then line, for one theme.
pub(super) struct FileColours {
    pub(super) dark: bool,
    pub(super) hunks: Vec<Vec<Vec<Span>>>,
}

/// Highlights `file` with its extension's syntax. The old side reads context
/// and removed lines, the new side context and added ones; each hunk starts
/// afresh, as what lies between two hunks is unknown.
pub(super) fn colour_file(file: &FileDiff, dark: bool) -> FileColours {
    let ps = syntaxes();
    let ext = file.extension();
    let syntax = ps
        .find_syntax_by_name(ext)
        .or_else(|| ps.find_syntax_by_extension(ext));
    let Some(syntax) = syntax else {
        return FileColours {
            dark,
            hunks: Vec::new(),
        };
    };
    let theme = &themes().themes[if dark {
        "base16-mocha.dark"
    } else {
        "Solarized (light)"
    }];
    let hunks = file
        .hunks
        .iter()
        .map(|hunk| {
            let mut old = HighlightLines::new(syntax, theme);
            let mut new = HighlightLines::new(syntax, theme);
            hunk.lines
                .iter()
                .map(|line| {
                    let text = format!("{}\n", line.text);
                    match line.kind {
                        LineKind::Removed => spans(&mut old, &text),
                        LineKind::Added => spans(&mut new, &text),
                        LineKind::Context => {
                            spans(&mut old, &text);
                            spans(&mut new, &text)
                        }
                        LineKind::NoNewline => Vec::new(),
                    }
                })
                .collect()
        })
        .collect();
    FileColours { dark, hunks }
}

/// `text`'s runs, its trailing newline left out.
fn spans(highlighter: &mut HighlightLines, text: &str) -> Vec<Span> {
    let end = text.len() - 1;
    let Ok(runs) = highlighter.highlight_line(text, syntaxes()) else {
        return Vec::new();
    };
    let mut at = 0;
    runs.into_iter()
        .filter_map(|(style, run)| {
            let bytes = at..(at + run.len()).min(end);
            at += run.len();
            let fg = style.foreground;
            (!bytes.is_empty()).then(|| Span {
                bytes,
                color: Color32::from_rgb(fg.r, fg.g, fg.b),
                italics: style.font_style.contains(FontStyle::ITALIC),
            })
        })
        .collect()
}

/// Syntect's grammars with bat's extra ones (TypeScript, Swift, TOML…).
fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

fn themes() -> &'static ThemeSet {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    THEMES.get_or_init(ThemeSet::load_defaults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iboard::diff::{Hunk, Line};

    fn file(path: &str, lines: &[(LineKind, &str)]) -> FileDiff {
        FileDiff {
            new_path: Some(path.to_string()),
            hunks: vec![Hunk {
                header: "@@ -1 +1 @@".to_string(),
                lines: lines
                    .iter()
                    .map(|(kind, text)| Line {
                        kind: *kind,
                        old: None,
                        new: None,
                        text: (*text).to_string(),
                    })
                    .collect(),
            }],
            ..FileDiff::default()
        }
    }

    fn first_colour(colours: &FileColours, line: usize) -> Color32 {
        colours.hunks[0][line][0].color
    }

    #[test]
    fn a_comment_colours_the_lines_it_spans() {
        let colours = colour_file(
            &file(
                "src/lib.rs",
                &[
                    (LineKind::Context, "/* opens"),
                    (LineKind::Context, "let still_comment = 1;"),
                    (LineKind::Context, "closes */ let code = 1;"),
                ],
            ),
            true,
        );
        let comment = first_colour(&colours, 0);
        assert_eq!(first_colour(&colours, 1), comment);
        let last = &colours.hunks[0][2];
        assert_ne!(last.last().map(|span| span.color), Some(comment));
    }

    #[test]
    fn each_side_reads_its_own_lines() {
        let colours = colour_file(
            &file(
                "src/lib.rs",
                &[
                    (LineKind::Removed, "/* gone"),
                    (LineKind::Added, "let kept = 1;"),
                ],
            ),
            false,
        );
        assert_ne!(first_colour(&colours, 1), first_colour(&colours, 0));
    }

    #[test]
    fn spans_cover_the_text_and_not_its_newline() {
        let text = "fn main() {}";
        let colours = colour_file(&file("a.rs", &[(LineKind::Added, text)]), true);
        let line = &colours.hunks[0][0];
        assert_eq!(line.first().map(|span| span.bytes.start), Some(0));
        assert_eq!(line.last().map(|span| span.bytes.end), Some(text.len()));
        assert!(line.len() > 1);
    }

    #[test]
    fn an_unknown_syntax_has_no_colours() {
        let colours = colour_file(&file("notes.zzz", &[(LineKind::Added, "x")]), true);
        assert!(colours.hunks.is_empty());
    }
}
