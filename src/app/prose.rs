//! Draws [`prose`] blocks at the reading sizes. The markdown viewer sets no
//! line height, draws a bullet before every task box and a blank line before
//! every heading, and has no option for any of them.

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, FontId, Layout, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use iboard::prose::{self, Item, Prose, Span};
use iboard::theme;

/// `read-body`'s line height.
const BODY_LINE: f32 = 24.0;
/// `read-h1`'s.
const H1_LINE: f32 = 26.0;
/// `read-h2`'s.
const H2_LINE: f32 = 22.0;
/// `read-h3`'s.
const H3_LINE: f32 = 20.0;
/// Under a paragraph or a list, and between a list's items.
const BLOCK_GAP: f32 = 10.0;
/// Under a `read-h2`.
const H2_BELOW: f32 = 6.0;
/// What a list indents by: a bullet, a number or a task box sits in here.
const MARKER_W: f32 = 20.0;
pub(super) const CODE_SIZE: f32 = 13.0;
/// A code span's height: its background, and its baseline against the text's.
const CODE_LINE: f32 = 20.0;
/// A narrow no-break space, the side padding of a code span's background.
const CODE_PAD: &str = "\u{202F}";

/// A task box clicked this frame: where it is and what it now says.
pub(super) type Ticked = Option<(Range<usize>, bool)>;

/// The space above and below a block; between two blocks the larger one wins.
pub(super) fn margins(block: &Prose) -> (f32, f32) {
    match block {
        Prose::Heading { level: 1, .. } => (0.0, theme::SPACE_4),
        Prose::Heading { level: 2, .. } => (theme::SPACE_6, H2_BELOW),
        Prose::Heading { .. } => (theme::SPACE_5, theme::SPACE_2),
        Prose::Paragraph(_) | Prose::List { .. } => (0.0, BLOCK_GAP),
    }
}

/// Draws `blocks`, top-level ones of the issue.
pub(super) fn show(ui: &mut Ui, blocks: &[Prose], ticked: &mut Ticked) {
    draw(ui, blocks, 0, ticked);
}

fn draw(ui: &mut Ui, blocks: &[Prose], depth: usize, ticked: &mut Ticked) {
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let mut below = 0.0;
    for (index, block) in blocks.iter().enumerate() {
        let (top, bottom) = margins(block);
        if index > 0 {
            ui.add_space(f32::max(below, top));
        }
        below = bottom;
        ui.push_id(index, |ui| match block {
            Prose::Heading { level, spans } => {
                let (font, line) = heading_font(*level);
                inline(ui, spans, &font, line);
            }
            Prose::Paragraph(spans) => paragraph(ui, spans),
            Prose::List { start, items } => list(ui, *start, items, depth, ticked),
        });
    }
}

fn heading_font(level: u8) -> (FontId, f32) {
    match level {
        1 => (theme::read_h1(), H1_LINE),
        2 => (theme::read_h2(), H2_LINE),
        _ => (theme::read_h3(), H3_LINE),
    }
}

fn semibold(size: f32) -> FontId {
    FontId::new(size, theme::read_h1().family)
}

fn paragraph(ui: &mut Ui, spans: &[Span]) {
    let width = ui.available_width().min(theme::READ_MEASURE);
    ui.allocate_ui(Vec2::new(width, 0.0), |ui| {
        inline(ui, spans, &theme::read_body(), BODY_LINE);
    });
}

fn list(ui: &mut Ui, start: Option<u64>, items: &[Item], depth: usize, ticked: &mut Ticked) {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            ui.add_space(BLOCK_GAP);
        }
        let number = start.map(|start| start + index as u64);
        let row = Vec2::new(ui.available_width(), 0.0);
        ui.push_id(index, |ui| {
            ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Min), |ui| {
                marker(ui, item, number, depth, ticked);
                ui.vertical(|ui| draw(ui, &item.blocks, depth + 1, ticked));
            });
        });
    }
}

/// The item's first-line marker, in its column.
fn marker(ui: &mut Ui, item: &Item, number: Option<u64>, depth: usize, ticked: &mut Ticked) {
    // A line's glyphs sit at the top of its height: markers centre on the glyphs.
    let glyphs = ui.fonts_mut(|fonts| fonts.row_height(&theme::read_body()));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(MARKER_W, glyphs), Sense::hover());
    if let Some(task) = &item.task {
        let mut checked = task.checked;
        let hit = rect.with_max_x(rect.left() + ui.spacing().icon_width);
        let response = ui
            .scope(|ui| {
                // Any larger, and the box centres itself away from the column's edge.
                ui.spacing_mut().interact_size = Vec2::ZERO;
                ui.put(hit, egui::Checkbox::without_text(&mut checked))
            })
            .inner;
        response.widget_info(|| {
            let label = item_text(item);
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, task.checked, label)
        });
        if response.clicked() {
            *ticked = Some((task.marker.clone(), checked));
        }
        return;
    }
    let ink = theme::palette(ui).ink;
    match number {
        Some(number) => {
            ui.painter().text(
                Pos2::new(rect.right() - theme::SPACE_2, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                format!("{number}."),
                theme::read_body(),
                ink,
            );
        }
        None => paint_bullet(ui, rect, depth > 0, ink),
    }
}

fn paint_bullet(ui: &Ui, rect: Rect, hollow: bool, ink: Color32) {
    let center = Pos2::new(rect.left() + theme::SPACE_3, rect.center().y);
    if hollow {
        ui.painter()
            .circle_stroke(center, 2.0, Stroke::new(1.0, ink));
    } else {
        ui.painter().circle_filled(center, 2.5, ink);
    }
}

/// What VoiceOver reads for a task box: the words of its item.
fn item_text(item: &Item) -> String {
    let mut text = String::new();
    for block in &item.blocks {
        if let Prose::Paragraph(spans) = block {
            spans.iter().for_each(|span| text.push_str(&span.text));
        }
    }
    text
}

/// The runs of one block, wrapped together, a link being a widget of its own.
fn inline(ui: &mut Ui, spans: &[Span], font: &FontId, line: f32) {
    let layout = Layout::left_to_right(Align::BOTTOM).with_main_wrap(true);
    let size = Vec2::new(ui.available_width(), 0.0);
    ui.allocate_ui_with_layout(size, layout, |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.set_row_height(line);
        let mut job = LayoutJob::default();
        let mut rest = spans;
        while let Some(span) = rest.first() {
            let Some(url) = &span.link else {
                append(ui, &mut job, span, font, line);
                rest = &rest[1..];
                continue;
            };
            flush(ui, &mut job);
            let run = rest
                .iter()
                .take_while(|next| next.link.as_ref() == Some(url))
                .count();
            let mut text = LayoutJob::default();
            for span in &rest[..run] {
                append(ui, &mut text, span, font, line);
            }
            ui.hyperlink_to(text, url);
            rest = &rest[run..];
        }
        flush(ui, &mut job);
    });
}

fn flush(ui: &mut Ui, job: &mut LayoutJob) {
    if !job.is_empty() {
        ui.label(std::mem::take(job));
    }
}

fn append(ui: &Ui, job: &mut LayoutJob, span: &Span, font: &FontId, line: f32) {
    let p = theme::palette(ui);
    let mut format = TextFormat {
        font_id: font.clone(),
        line_height: Some(line),
        color: if span.link.is_some() {
            Color32::PLACEHOLDER
        } else {
            p.ink
        },
        italics: span.emphasis,
        valign: Align::BOTTOM,
        ..TextFormat::default()
    };
    if span.strikethrough {
        format.strikethrough = Stroke::new(1.0, format.color);
    }
    if span.strong {
        format.font_id = semibold(font.size);
    }
    if span.code {
        format.font_id = FontId::monospace(CODE_SIZE);
        format.background = p.surface_sunken;
        format.line_height = Some(CODE_LINE);
        format.valign = Align::Center;
        format.expand_bg = 0.0;
        let text = format!("{CODE_PAD}{}{CODE_PAD}", span.text);
        job.append(&text, 0.0, format);
    } else {
        job.append(&span.text, 0.0, format);
    }
}

/// `block` after this frame's click on a task box in it.
pub(super) fn apply(block: &str, ticked: Ticked) -> Option<String> {
    ticked.map(|(marker, checked)| prose::with_task(block, marker, checked))
}
