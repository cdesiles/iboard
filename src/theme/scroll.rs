//! The overlay Scrollbar: a thumb floating 2pt inside the pane's edge that
//! never takes layout width (see `design/components/Scrollbar`).

use egui::scroll_area::{ScrollArea, ScrollAreaOutput};
use egui::style::ScrollStyle;
use std::sync::Arc;

use egui::{CornerRadius, Rect, Style, Ui, Vec2};

use super::{RADIUS_PILL, SCROLL_HIT, SCROLL_THUMB, SCROLL_THUMB_HOVER, SCROLL_THUMB_MIN, palette};

/// How long the thumb stays `ink-muted` after the last movement.
const SCROLLING_SECS: f64 = 0.8;

/// The widths every scroll area is drawn with; [`scroll_area`] adds colours.
pub(super) fn style() -> ScrollStyle {
    ScrollStyle {
        floating: true,
        bar_width: SCROLL_THUMB_HOVER,
        floating_width: SCROLL_THUMB,
        floating_allocated_width: 0.0,
        handle_min_length: SCROLL_THUMB_MIN,
        bar_inner_margin: 0.0,
        bar_outer_margin: 2.0,
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
    themed(ui, focused, |ui, content| {
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
    themed(ui, focused, |ui, content| {
        area.show_rows(ui, row_height, rows, |ui, range| {
            ui.set_style(content);
            add_contents(ui, range)
        })
    })
}

/// Runs `show` with the thumb's colours in the scroll area's style; `show`
/// hands the content back the style it had.
fn themed<R>(
    ui: &mut Ui,
    focused: bool,
    show: impl FnOnce(&mut Ui, Arc<Style>) -> ScrollAreaOutput<R>,
) -> ScrollAreaOutput<R> {
    let p = palette(ui);
    let key = ui.next_auto_id().with("scroll-thumb");
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let seen: Option<Seen> = ctx.data(|d| d.get_temp(key));
    let scrolling = seen.is_some_and(|seen| now - seen.moved_at < SCROLLING_SECS);
    let pointed = seen.is_some_and(|seen| {
        let strip = seen.inner.with_min_x(seen.inner.max.x - SCROLL_HIT);
        ctx.pointer_hover_pos().is_some_and(|at| strip.contains(at))
    });
    let window_focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
    let rest = if scrolling || pointed {
        p.ink_muted
    } else if focused && window_focused {
        p.line_strong
    } else {
        p.line
    };

    let content = ui.style().clone();
    let output = ui
        .scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            for (visuals, color) in [
                (&mut widgets.inactive, rest),
                (&mut widgets.hovered, p.ink_muted),
                (&mut widgets.active, p.accent),
            ] {
                visuals.fg_stroke.color = color;
                visuals.corner_radius = CornerRadius::same(RADIUS_PILL);
            }
            show(ui, content)
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
    ctx.data_mut(|d| {
        d.insert_temp(
            key,
            Seen {
                offset,
                moved_at,
                inner: output.inner_rect,
            },
        );
    });
    output
}
