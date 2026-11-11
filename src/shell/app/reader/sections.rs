//! The issue's blocks as the reader draws them, one top-level block at a time,
//! and the lines of the file each one holds.

use std::ops::Range;

use super::{
    BlockLines, CommonMarkCache, CommonMarkViewer, NoteAction, Notes, Path, Rect, Stroke,
    block_notes, images, outdated_notes, prose_ui, theme,
};
use crate::{markdown, prose};

/// What drawing the issue's blocks did.
pub(super) struct DrawnBlocks {
    pub(super) edits: Vec<(Range<usize>, String)>,
    pub(super) actions: Vec<NoteAction>,
    pub(super) missing: Vec<String>,
    /// Where the first `# ` heading ends, if there is one.
    pub(super) title_bottom: Option<f32>,
    pub(super) blocks: Vec<BlockLines>,
}

/// The issue's text, one top-level block at a time so each has a margin to
/// hang comments on (`notes`, for an issue that takes them). Returns the
/// blocks a ticked checkbox changed, what was asked of the comments, and the
/// images that could not be shown, drawn as placeholders.
pub(super) fn issue_blocks(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    images: &mut images::ImageChecks,
    path: &Path,
    text: &str,
    mut notes: Option<&mut Notes>,
) -> DrawnBlocks {
    let blocks = markdown::blocks(text);
    let mut edits = Vec::new();
    let mut actions = Vec::new();
    let mut missing = Vec::new();
    let mut drawn = Vec::new();
    let mut below: f32 = 0.0;
    let mut title_bottom = None;
    for (index, range) in blocks.iter().enumerate() {
        let block = &text[range.clone()];
        let parsed = prose::parse(block);
        let (top, bottom) = parsed
            .as_ref()
            .and_then(|parsed| parsed.first())
            .map_or((theme::SPACE_3, theme::SPACE_3), prose_ui::margins);
        if index > 0 {
            ui.add_space(below.max(top));
        }
        below = bottom;
        let shown = ui
            .push_id(("block", index), |ui| match &parsed {
                Some(parsed) => {
                    let mut ticked = None;
                    let drawn = ui.vertical(|ui| prose_ui::show(ui, parsed, &mut ticked));
                    BlockDrawn {
                        rect: drawn.response.rect,
                        edited: prose_ui::apply(block, ticked),
                    }
                }
                None => viewer_block(ui, cache, images, path, block, &mut missing),
            })
            .inner;
        drawn.push(BlockLines {
            rect: shown.rect,
            first: text[..range.start].matches('\n').count(),
            lines: block.trim_end().lines().count(),
        });
        if title_bottom.is_none() && block.trim_start().starts_with("# ") {
            title_bottom = Some(shown.rect.bottom());
        }
        if let Some(edited) = shown.edited {
            edits.push((range.clone(), edited));
        }
        if let Some(notes) = notes.as_deref_mut() {
            let anchor = text[range.clone()].trim();
            let row = shown.rect.with_max_x(ui.max_rect().right());
            actions.extend(block_notes(ui, notes, anchor, row, index));
        }
    }
    if let Some(notes) = notes {
        actions.extend(outdated_notes(ui, notes, text, &blocks));
    }
    DrawnBlocks {
        edits,
        actions,
        missing,
        title_bottom,
        blocks: drawn,
    }
}

/// Where a block was drawn, and its text when a ticked box changed it.
struct BlockDrawn {
    rect: Rect,
    edited: Option<String>,
}

/// A block that is more than prose, drawn by the markdown viewer: its lines
/// keep the viewer's height. `missing` receives the images it could not show.
fn viewer_block(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    images: &mut images::ImageChecks,
    path: &Path,
    block: &str,
    missing: &mut Vec<String>,
) -> BlockDrawn {
    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
    let mut edited = block.to_string();
    let broken = images.missing(path, block);
    let table = markdown::is_table(block);
    if !table {
        quiet_code_blocks(ui);
    }
    let mut draw = |ui: &mut egui::Ui| {
        let viewer = CommonMarkViewer::new().default_implicit_uri_scheme(image_base_uri(path));
        if broken.is_empty() {
            viewer.show_mut(ui, cache, &mut edited).response
        } else {
            let placeheld = images::with_placeholders(block, &broken);
            viewer.show(ui, cache, &placeheld).response
        }
    };
    // Table cells do not wrap: a wide table scrolls within the page.
    let shown = match table {
        true => egui::ScrollArea::horizontal().show(ui, draw).inner,
        false => draw(ui),
    };
    missing.extend(broken.into_iter().map(|link| link.url));
    BlockDrawn {
        rect: shown.rect,
        edited: shown.changed().then_some(edited),
    }
}

/// The viewer's code block paints its frame twice, from the two strokes it
/// reads here, and centres its copy icon `2 * button_padding.y` under the top.
fn quiet_code_blocks(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    let widgets = &mut ui.visuals_mut().widgets;
    widgets.noninteractive.bg_stroke = Stroke::NONE;
    widgets.inactive.bg_stroke = Stroke::new(1.0, p.line_soft);
    ui.spacing_mut().button_padding.y = 0.0;
}

/// The `file://` base a relative image link in `issue` resolves against: the
/// issue's own folder, so `assets/<stem>/ui.png` finds the mockup written beside
/// it. Passed to the markdown viewer as its implicit URI scheme.
pub(in crate::shell::app) fn image_base_uri(issue: &Path) -> String {
    let dir = issue.parent().unwrap_or(Path::new("."));
    format!("file://{}/", dir.display())
}
