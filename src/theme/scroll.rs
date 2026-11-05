//! The overlay Scrollbar: a thumb floating 2pt inside the pane's edges that
//! never takes layout width (see `design/components/Scrollbar`).

use egui::emath::remap_clamp;
use egui::epaint::MarginF32;
use egui::scroll_area::{ScrollArea, ScrollAreaOutput};
use egui::style::{ScrollAnimation, ScrollStyle};
use std::sync::Arc;

use egui::{
    Color32, Context, CornerRadius, Id, Margin, Pos2, Rect, Sense, Style, Ui, Vec2, Vec2b,
    WidgetInfo, WidgetType, lerp,
};

use super::{
    RADIUS_PILL, SCROLL_HIT, SCROLL_THUMB, SCROLL_THUMB_HOVER, SCROLL_THUMB_MIN, motion, palette,
};

/// How long the thumb stays `ink-muted` after the last movement.
const SCROLLING_SECS: f64 = 0.8;
/// How long the thumb takes to change width or colour.
const CHANGE_SECS: f32 = 0.12;
/// How much of the pane a click in the strip pages by.
const PAGE: f32 = 0.9;
/// The gap between the thumb and the pane's edges.
const INSET: f32 = 2.0;
/// The right padding that keeps content clear of the widest thumb, with the
/// same 4pt gap the thumb keeps at rest.
pub const SCROLL_CLEAR: f32 = INSET + SCROLL_THUMB_HOVER + 4.0;

/// A pane's padding inside its scroll area, so it scrolls with the content
/// and the thumb floats over it: `x` on the left, at least [`SCROLL_CLEAR`]
/// on the right, `y` above and below.
pub fn scroll_margin(x: f32, y: f32) -> Margin {
    MarginF32 {
        left: x,
        right: x.max(SCROLL_CLEAR),
        top: y,
        bottom: y,
    }
    .into()
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
    area: Id,
    /// The widget that had the keyboard, for a click in the strip to give
    /// back: egui takes focus from it on any click elsewhere.
    holder: Option<Id>,
}

impl Seen {
    /// Whether `at` is in the strip along an edge that has a thumb.
    fn points(&self, at: Pos2) -> bool {
        let right = self.inner.with_min_x(self.inner.max.x - SCROLL_HIT);
        let bottom = self.inner.with_min_y(self.inner.max.y - SCROLL_HIT);
        self.overflow.y && right.contains(at) || self.overflow.x && bottom.contains(at)
    }

    /// Whether the mouse is down on one of this area's thumbs.
    fn dragged(&self, ctx: &Context) -> bool {
        (0..2usize).any(|axis| {
            ctx.read_response(self.area.with(axis))
                .is_some_and(|bar| bar.is_pointer_button_down_on())
        })
    }

    /// Whether the widget holding the keyboard is in this pane; `None` while
    /// no widget holds it.
    fn holds_focus(&self, ui: &Ui) -> Option<bool> {
        let holder = ui.ctx().memory(egui::Memory::focused)?;
        let holder = ui.ctx().read_response(holder)?;
        Some(holder.layer_id == ui.layer_id() && self.inner.intersects(holder.rect))
    }
}

/// What a scroll area hands its content.
struct Content {
    style: Arc<Style>,
    /// A page asked for by a click in the strip last frame.
    page: Vec2,
}

impl Content {
    /// Runs once the content is shown, so the page goes to this area and not
    /// to one nested in it.
    fn turn_page(&self, ui: &Ui) {
        if self.page != Vec2::ZERO {
            ui.scroll_with_delta_animation(-self.page, ScrollAnimation::none());
        }
    }
}

/// Shows `area` with the design system's thumb: `line-strong` at rest,
/// `ink-muted` while scrolling or pointed at, `accent` while dragged, and
/// `line` when the pane (or the window) does not have focus.
///
/// A widget holding the keyboard decides by where it is. `focused` says
/// whether this pane has the keyboard while no widget holds it.
pub fn scroll_area<R>(
    ui: &mut Ui,
    area: ScrollArea,
    focused: bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> ScrollAreaOutput<R> {
    themed(ui, area, focused, |ui, area, content| {
        area.show(ui, |ui| {
            ui.set_style(content.style.clone());
            let inner = add_contents(ui);
            content.turn_page(ui);
            inner
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
            ui.set_style(content.style.clone());
            let inner = add_contents(ui, range);
            content.turn_page(ui);
            inner
        })
    })
}

/// `target`, reached over `secs` from the colour `id` last had.
fn animate_color(ctx: &Context, id: Id, target: Color32, secs: f32) -> Color32 {
    let [r, g, b, a] = target.to_array();
    let channel = |name: &str, value: u8| {
        ctx.animate_value_with_time(id.with(name), f32::from(value), secs)
            .round() as u8
    };
    Color32::from_rgba_premultiplied(
        channel("r", r),
        channel("g", g),
        channel("b", b),
        channel("a", a),
    )
}

/// The stretches of the strip along `axis` on either side of the thumb, as
/// egui places it: `(before, after)`.
fn strip_around_thumb<R>(output: &ScrollAreaOutput<R>, axis: usize) -> (Rect, Rect) {
    let inner = output.inner_rect;
    let track = inner.shrink(INSET);
    let (track_min, track_max) = (track.min[axis], track.max[axis]);
    let content = output.content_size[axis];
    let shown = inner.size()[axis];
    let offset = output.state.offset[axis];
    let on_track = |at: f32| remap_clamp(at, 0.0..=content, track_min..=track_max);
    let length = (on_track(offset + shown) - on_track(offset)).max(SCROLL_THUMB_MIN);
    let start = remap_clamp(
        offset,
        0.0..=content - shown,
        track_min..=track_max - length,
    );
    let strip = if axis == 0 {
        inner.with_min_y(inner.max.y - SCROLL_HIT)
    } else {
        inner.with_min_x(inner.max.x - SCROLL_HIT)
    };
    let (mut before, mut after) = (strip, strip);
    before.max[axis] = start;
    after.min[axis] = start + length;
    (before, after)
}

/// Lays the strip over the bar on either side of the thumb, where egui would
/// jump the thumb to the click, and returns the page a click there asks for.
fn page_strips<R>(ui: &Ui, output: &ScrollAreaOutput<R>, overflow: Vec2b) -> Vec2 {
    let mut page = Vec2::ZERO;
    let axes = [
        (0, ["Page left", "Page right"]),
        (1, ["Page up", "Page down"]),
    ];
    for (axis, labels) in axes {
        if !overflow[axis] {
            continue;
        }
        let (before, after) = strip_around_thumb(output, axis);
        for (rect, sign, label) in [(before, -1.0, labels[0]), (after, 1.0, labels[1])] {
            let id = output.id.with(("page", label));
            let strip = ui.interact(rect, id, Sense::CLICK | Sense::DRAG);
            strip.widget_info(|| WidgetInfo::labeled(WidgetType::ScrollBar, true, label));
            if strip.clicked() {
                page[axis] += sign * PAGE * output.inner_rect.size()[axis];
            }
        }
    }
    page
}

/// The thumb's width and colour, in `ui`'s style.
fn style_thumb(ui: &mut Ui, thumb: Color32, widened: f32) {
    let scroll = &mut ui.spacing_mut().scroll;
    scroll.floating_width = lerp(SCROLL_THUMB..=SCROLL_THUMB_HOVER, widened);
    let widgets = &mut ui.visuals_mut().widgets;
    for visuals in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        visuals.fg_stroke.color = thumb;
        visuals.corner_radius = CornerRadius::same(RADIUS_PILL);
    }
}

/// Runs `show` with the thumb's widths and colours in the scroll area's
/// style; `show` hands the content back the style it had.
fn themed<R>(
    ui: &mut Ui,
    area: ScrollArea,
    focused: bool,
    show: impl FnOnce(&mut Ui, ScrollArea, Content) -> ScrollAreaOutput<R>,
) -> ScrollAreaOutput<R> {
    let p = palette(ui);
    let key = ui.next_auto_id().with("scroll-thumb");
    let (page_key, refocus_key) = (key.with("page"), key.with("refocus"));
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let seen: Option<Seen> = ctx.data(|d| d.get_temp(key));
    if let Some(holder) = ctx.data_mut(|d| {
        let holder = d.get_temp::<Id>(refocus_key);
        d.remove::<Id>(refocus_key);
        holder
    }) {
        ctx.memory_mut(|m| m.request_focus(holder));
    }
    let scrolling = seen.is_some_and(|seen| now - seen.moved_at < SCROLLING_SECS);
    let pointed = seen
        .zip(ctx.pointer_hover_pos())
        .is_some_and(|(seen, at)| seen.points(at));
    let dragged = seen.is_some_and(|seen| seen.dragged(&ctx));
    let holds = seen.and_then(|seen| seen.holds_focus(ui));
    let active = ctx.input(|i| i.viewport().focused.unwrap_or(true))
        && ctx.memory(|m| m.is_above_modal_layer(ui.layer_id()));
    let target = if dragged {
        p.accent
    } else if scrolling || pointed {
        p.ink_muted
    } else if holds.unwrap_or(focused) && active {
        p.line_strong
    } else {
        p.line
    };
    let change = motion(&ctx, CHANGE_SECS);
    let thumb = animate_color(&ctx, key.with("colour"), target, change);
    let widened = ctx.animate_bool_with_time(key.with("wide"), pointed || dragged, change);

    let content = Content {
        style: ui.style().clone(),
        page: ctx
            .data_mut(|d| d.remove_temp(page_key))
            .unwrap_or_default(),
    };
    let output = ui
        .scope(|ui| {
            style_thumb(ui, thumb, widened);
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

    let has_bar = |axis: usize| ctx.read_response(output.id.with(axis)).is_some();
    let overflow = Vec2b::new(
        has_bar(0) && output.content_size.x > output.inner_rect.width(),
        has_bar(1) && output.content_size.y > output.inner_rect.height(),
    );
    let page = page_strips(ui, &output, overflow);
    if page != Vec2::ZERO {
        ctx.data_mut(|d| d.insert_temp(page_key, page));
        ctx.request_repaint();
    }
    let clicked = page != Vec2::ZERO
        || (0..2usize).any(|axis| {
            ctx.read_response(output.id.with(axis))
                .is_some_and(|bar| bar.clicked())
        });
    if let Some(holder) = seen.and_then(|seen| seen.holder).filter(|_| clicked) {
        ctx.data_mut(|d| d.insert_temp(refocus_key, holder));
        ctx.request_repaint();
    }

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
    let holder = ctx.memory(egui::Memory::focused);
    ctx.data_mut(|d| {
        d.insert_temp(
            key,
            Seen {
                offset,
                moved_at,
                inner: output.inner_rect,
                overflow,
                area: output.id,
                holder,
            },
        );
    });
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;
    use egui::{Event, PointerButton, RawInput};

    const PANE: f32 = 200.0;

    fn themed_ctx() -> Context {
        let ctx = Context::default();
        theme::install(&ctx);
        theme::set_reduced_motion(&ctx, true);
        ctx
    }

    fn frame(ctx: &Context, events: Vec<Event>, draw: impl FnMut(&mut Ui)) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(PANE))),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, draw).textures_delta.clear();
    }

    fn button(at: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    /// A pane of 1000pt of content under a text field; returns its offset.
    fn tall(ui: &mut Ui, field: &mut String) -> f32 {
        let area = ScrollArea::vertical().id_salt("tall").auto_shrink(false);
        scroll_area(ui, area, true, |ui| {
            ui.add(egui::TextEdit::singleline(field).id(Id::new("field")));
            ui.allocate_space(Vec2::new(10.0, 1000.0));
        })
        .state
        .offset
        .y
    }

    /// Runs one frame per entry of `frames`, and returns the last offset.
    fn play(ctx: &Context, field: &mut String, frames: Vec<Vec<Event>>) -> f32 {
        let mut offset = 0.0;
        for events in frames {
            frame(ctx, events, |ui| offset = tall(ui, field));
        }
        offset
    }

    /// Clicks `at`, with the frames egui needs to settle.
    fn click(ctx: &Context, field: &mut String, at: Pos2) -> f32 {
        let frames = vec![
            vec![Event::PointerMoved(at)],
            vec![button(at, true)],
            vec![button(at, false)],
            vec![],
            vec![],
        ];
        play(ctx, field, frames)
    }

    #[test]
    fn a_click_in_the_strip_pages_by_ninety_percent_towards_it() {
        let ctx = themed_ctx();
        let field = &mut String::new();
        play(&ctx, field, vec![vec![]; 3]);
        let below = Pos2::new(PANE - 3.0, PANE - 10.0);
        let page = PAGE * PANE;
        assert_eq!(click(&ctx, field, below), page);
        assert_eq!(click(&ctx, field, below), 2.0 * page);
        let above = Pos2::new(PANE - 3.0, 4.0);
        assert_eq!(click(&ctx, field, above), page);
    }

    #[test]
    fn the_thumb_still_drags() {
        let ctx = themed_ctx();
        let field = &mut String::new();
        play(&ctx, field, vec![vec![]; 3]);
        let on = Pos2::new(PANE - 3.0, 10.0);
        let to = Pos2::new(PANE - 3.0, 60.0);
        let offset = play(
            &ctx,
            field,
            vec![
                vec![Event::PointerMoved(on)],
                vec![button(on, true)],
                vec![Event::PointerMoved(to)],
                vec![Event::PointerMoved(to)],
                vec![button(to, false)],
            ],
        );
        assert!(offset > 100.0, "the drag moved it: {offset}");
    }

    #[test]
    fn clicking_the_strip_or_the_thumb_leaves_keyboard_focus_where_it_was() {
        let ctx = themed_ctx();
        let field = &mut String::new();
        let id = Id::new("field");
        play(&ctx, field, vec![vec![]; 3]);
        ctx.memory_mut(|m| m.request_focus(id));
        play(&ctx, field, vec![vec![]; 2]);
        click(&ctx, field, Pos2::new(PANE - 3.0, PANE - 10.0));
        assert!(ctx.memory(|m| m.has_focus(id)), "the strip");
        click(&ctx, field, Pos2::new(PANE - 3.0, 4.0));
        assert!(ctx.memory(|m| m.has_focus(id)), "above the thumb");
        click(&ctx, field, Pos2::new(PANE - 3.0, 60.0));
        assert!(ctx.memory(|m| m.has_focus(id)), "the thumb");
    }

    /// The thumb's colour once settled, for a pane beside a text field, with
    /// the keyboard on nothing, on a field in the pane or on the one beside.
    fn settled_colour(focused: bool, holder: Option<&str>) -> (Color32, &'static theme::Palette) {
        let ctx = themed_ctx();
        let (mut colour, mut seen_palette) = (Color32::TRANSPARENT, None);
        let (mut inside, mut beside) = (String::new(), String::new());
        for _ in 0..4 {
            frame(&ctx, vec![], |ui| {
                seen_palette = Some(palette(ui));
                let area = ScrollArea::vertical()
                    .id_salt("pane")
                    .auto_shrink(false)
                    .max_height(100.0);
                themed(ui, area, focused, |ui, area, _| {
                    colour = ui.visuals().widgets.inactive.fg_stroke.color;
                    area.show(ui, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut inside).id(Id::new("inside")));
                        ui.allocate_space(Vec2::new(10.0, 400.0));
                    })
                });
                ui.add(egui::TextEdit::singleline(&mut beside).id(Id::new("beside")));
                if let Some(holder) = holder {
                    ui.memory_mut(|m| m.request_focus(Id::new(holder)));
                }
            });
        }
        (colour, seen_palette.unwrap())
    }

    #[test]
    fn only_the_pane_with_the_keyboard_keeps_its_rest_colour() {
        let (colour, p) = settled_colour(true, None);
        assert_eq!(colour, p.line_strong, "the pane that has the keyboard");
        let (colour, p) = settled_colour(false, None);
        assert_eq!(colour, p.line, "another pane has it");
        let (colour, p) = settled_colour(false, Some("inside"));
        assert_eq!(colour, p.line_strong, "it holds the focused field");
        let (colour, p) = settled_colour(true, Some("beside"));
        assert_eq!(colour, p.line, "a field elsewhere holds the keyboard");
    }

    #[test]
    fn the_colour_eases_and_lands_at_once_with_reduced_motion() {
        let eased = |reduce: bool| {
            let ctx = themed_ctx();
            theme::set_reduced_motion(&ctx, reduce);
            let mut colour = Color32::TRANSPARENT;
            for target in [Color32::BLACK, Color32::WHITE] {
                frame(&ctx, vec![], |ui| {
                    let secs = motion(ui.ctx(), CHANGE_SECS);
                    colour = animate_color(ui.ctx(), Id::new("thumb"), target, secs);
                });
            }
            colour
        };
        assert_eq!(eased(true), Color32::WHITE);
        assert_ne!(eased(false), Color32::WHITE);
    }
}
