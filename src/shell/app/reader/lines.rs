//! The reader's lines: where a click lands in the file, and how the blocks a
//! ticked checkbox changed go back into it.

use std::ops::Range;

use super::{Reader, Rect};
use crate::sheet;

/// One block of the issue on the page, and the lines of the file it holds.
pub(super) struct BlockLines {
    pub(super) rect: Rect,
    pub(super) first: usize,
    pub(super) lines: usize,
}

impl Reader {
    /// The line of the file drawn at height `y`: the block's first for prose,
    /// and in a block of several lines the one as far down as `y` is. Between
    /// blocks, the next one's.
    pub(in crate::shell::app) fn line_at(&self, y: f32) -> Option<usize> {
        let block = self
            .blocks
            .iter()
            .find(|block| y <= block.rect.bottom())
            .or(self.blocks.last())?;
        let down = (y - block.rect.top()) / block.rect.height().max(1.0);
        let within = (down.clamp(0.0, 1.0) * block.lines as f32) as usize;
        Some(block.first + within.min(block.lines.saturating_sub(1)))
    }
}

/// The issue's text with the blocks a ticked box changed written back, its
/// title's labels put back where they were. Edits replace byte ranges, so they
/// apply from the last one.
pub(super) fn restored_text(
    text: &str,
    title_line: Option<&(usize, String)>,
    edits: Vec<(Range<usize>, String)>,
) -> String {
    let mut text = text.to_string();
    for (range, block) in edits.into_iter().rev() {
        text.replace_range(range, &block);
    }
    if let Some((index, line)) = title_line {
        text = sheet::restore_line(&text, *index, line);
    }
    text
}
