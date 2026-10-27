//! The overlay Scrollbar: a thumb floating 2pt inside the pane's edges that
//! never takes layout width (see `design/components/Scrollbar`).

use egui::scroll_area::{ScrollArea, ScrollAreaOutput};
use egui::style::ScrollStyle;
use std::sync::Arc;

use egui::{CornerRadius, Margin, Pos2, Rect, Style, Ui, Vec2, Vec2b, lerp};

use super::{
    RADIUS_PILL, SCROLL_HIT, SCROLL_THUMB, SCROLL_THUMB_HOVER, SCROLL_THUMB_MIN, motion, palette,
};

/// How long the thumb stays `ink-muted` after the last movement.
const SCROLLING_SECS: f64 = 0.8;
/// How long the thumb takes to change width.
const WIDEN_SECS: f32 = 0.12;
/// The gap between the thumb and the pane's edges.
const INSET: f32 = 2.0;
/// The right padding that keeps content clear of the widest thumb, with the
/// same 4pt gap the thumb keeps at rest.
pub const SCROLL_CLEAR: f32 = INSET + SCROLL_THUMB_HOVER + 4.0;

/// A pane's padding inside its scroll area, so it scrolls with the content
/// and the thumb floats over it: `x` on the left, at least [`SCROLL_CLEAR`]
/// on the right, `y` above and below.
pub fn scroll_margin(x: f32, y: f32) -> Margin {
    Margin {
        left: x as i8,
        right: x.max(SCROLL_CLEAR) as i8,
        top: y as i8,
        bottom: y as i8,
    }
}

/// The widths every scroll area is drawn with; [`scroll_area`] adds colours.
pub(super) fn style() -> ScrollStyle {
    ScrollStyle {
        floating: true,
        bar_width: SCROLL_THUMB_HOVER,
        floating_width: SCROLL_THUMB,
        floating_allocated_width: 0.0,
        handle_min_length: SCROLL_THUMB_MIN,
        bar_inner_margin: 0.0,
        bar_outer_margin: INSET,
        foreground_color: true,
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.0,
        interact_background_opacity: 0.0,
        dormant_handle_opacity: 1.0,
        active_handle_opacity: 1.0,
        interact_handle_opacity: 1.0,
        ..ScrollStyle::floating()
    }
}

/// What the last frame saw of one scroll area.
#[derive(Clone, Copy)]
struct Seen {
    offset: Vec2,
    moved_at: f64,
    inner: Rect,
    overflow: Vec2b,
}

impl Seen {
    /// Whether `at` is in the strip along an edge that has a thumb.
    fn points(&self, at: Pos2) -> bool {
        let right = self.inner.with_min_x(self.inner.max.x - SCROLL_HIT);
        let bottom = self.inner.with_min_y(self.inner.max.y - SCROLL_HIT);
        self.overflow.y && right.contains(at) || self.overflow.x && bottom.contains(at)
    }
}

/// Shows `area` with the design system's thumb: `line-strong` at rest,
/// `ink-muted` while scrolling or pointed at, `accent` while dragged, and
/// `line` when the pane (or the window) does not have focus.
pub fn scroll_area<R>(
    ui: &mut Ui,
    area: ScrollArea,
    focused: bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> ScrollAreaOutput<R> {
    themed(ui, area, focused, |ui, area, content| {
        area.show(ui, |ui| {
            ui.set_style(content);
            add_contents(ui)
        })
    })
}

/// [`scroll_area`] for rows of one height, only the visible ones built.
pub fn scroll_rows<R>(
    ui: &mut Ui,
    area: ScrollArea,
    focused: bool,
    row_height: f32,
    rows: usize,
    add_contents: impl FnOnce(&mut Ui, std::ops::Range<usize>) -> R,
) -> ScrollAreaOutput<R> {
    themed(ui, area, focused, |ui, area, content| {
        area.show_rows(ui, row_height, rows, |ui, range| {
            ui.set_style(content);
            add_contents(ui, range)
        })
    })
}

/// Runs `show` with the thumb's widths and colours in the scroll area's
/// style; `show` hands the content back the style it had.
fn themed<R>(
    ui: &mut Ui,
    area: ScrollArea,
    focused: bool,
    show: impl FnOnce(&mut Ui, ScrollArea, Arc<Style>) -> ScrollAreaOutput<R>,
) -> ScrollAreaOutput<R> {
    let p = palette(ui);
    let key = ui.next_auto_id().with("scroll-thumb");
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let seen: Option<Seen> = ctx.data(|d| d.get_temp(key));
    let scrolling = seen.is_some_and(|seen| now - seen.moved_at < SCROLLING_SECS);
    let pointed = seen
        .zip(ctx.pointer_hover_pos())
        .is_some_and(|(seen, at)| seen.points(at));
    let window_focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
    let rest = if scrolling || pointed {
        p.ink_muted
    } else if focused && window_focused {
        p.line_strong
    } else {
        p.line
    };
    let widened = ctx.animate_bool_with_time(key.with("wide"), pointed, motion(&ctx, WIDEN_SECS));

    let content = ui.style().clone();
    let output = ui
        .scope(|ui| {
            let scroll = &mut ui.spacing_mut().scroll;
            scroll.floating_width = lerp(SCROLL_THUMB..=SCROLL_THUMB_HOVER, widened);
            let widgets = &mut ui.visuals_mut().widgets;
            for (visuals, color) in [
                (&mut widgets.inactive, rest),
                (&mut widgets.hovered, p.ink_muted),
                (&mut widgets.active, p.accent),
            ] {
                visuals.fg_stroke.color = color;
                visuals.corner_radius = CornerRadius::same(RADIUS_PILL);
            }
            // The rail is only known once shown: last frame's, moved to where
            // this frame starts, and redrawn below if its size changed.
            let area = match seen {
                Some(seen) => {
                    let at = ui.available_rect_before_wrap().min;
                    area.scroll_bar_rect(Rect::from_min_size(at, seen.inner.size()).shrink(INSET))
                }
                None => area,
            };
            show(ui, area, content)
        })
        .inner;

    let offset = output.state.offset;
    let moved_at = match seen {
        Some(seen) if seen.offset == offset => seen.moved_at,
        Some(_) => now,
        None => f64::NEG_INFINITY,
    };
    if now - moved_at < SCROLLING_SECS {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SCROLLING_SECS));
    }
    if seen.is_none_or(|seen| seen.inner.size() != output.inner_rect.size()) {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            key,
            Seen {
                offset,
                moved_at,
                inner: output.inner_rect,
                overflow: Vec2b::new(
                    output.content_size.x > output.inner_rect.width(),
                    output.content_size.y > output.inner_rect.height(),
                ),
            },
        );
    });
    output
}
