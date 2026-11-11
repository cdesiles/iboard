//! The overlay Scrollbar: a thumb floating 2pt inside the pane's edges that
//! never takes layout width (see `design/components/Scrollbar`).

use egui::emath::remap_clamp;
use egui::epaint::MarginF32;
use egui::scroll_area::{ScrollArea, ScrollAreaOutput};
use egui::style::{ScrollAnimation, ScrollStyle};
use std::sync::Arc;

use egui::{
    Color32, Context, CornerRadius, Id, Margin, Pos2, Rect, Role, Sense, Style, Ui, Vec2, Vec2b,
    WidgetInfo, lerp,
};

use super::{
    RADIUS_PILL, SCROLL_FADE, SCROLL_HIT, SCROLL_LINGER, SCROLL_THUMB, SCROLL_THUMB_HOVER,
    SCROLL_THUMB_MIN, motion, palette,
};

/// How long the thumb takes to change width or colour.
const CHANGE_SECS: f32 = 0.12;
/// How much of the pane a click in the strip pages by.
const PAGE: f32 = 0.9;
/// The gap between the thumb and the pane's edges.
const INSET: f32 = 2.0;
/// The right padding that keeps content clear of the widest thumb, with a
/// 4pt gap.
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
    /// The last time it scrolled or the pointer held the thumb up.
    stirred_at: f64,
    shown: f32,
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

/// Shows `area` with the design system's thumb: none at rest, 4pt `scrollbar`
/// while scrolling, 8pt `scrollbar-active` with the pointer at the edge and
/// 8pt `accent` while dragged.
pub fn scroll_area<R>(
    ui: &mut Ui,
    area: ScrollArea,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> ScrollAreaOutput<R> {
    themed(ui, area, |ui, area, content| {
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
    row_height: f32,
    rows: usize,
    add_contents: impl FnOnce(&mut Ui, std::ops::Range<usize>) -> R,
) -> ScrollAreaOutput<R> {
    themed(ui, area, |ui, area, content| {
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
    let offset = output.state.clamped_offset()[axis];
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
            strip.widget_info(|| WidgetInfo::labeled(Role::ScrollBar, true, label));
            if strip.clicked() {
                page[axis] += sign * PAGE * output.inner_rect.size()[axis];
            }
        }
    }
    page
}

/// How the thumb looks this frame.
struct Thumb {
    colour: Color32,
    widened: f32,
    /// From 0, hidden, to 1, fully there.
    shown: f32,
    /// The pointer keeps it up, at the edge or dragging it.
    held: bool,
}

impl Thumb {
    fn now(ui: &Ui, key: Id, seen: Option<Seen>) -> Self {
        let ctx = ui.ctx();
        let p = palette(ui);
        let pointed = seen.zip(ctx.pointer_hover_pos()).is_some_and(|(seen, at)| {
            seen.points(at) && ctx.layer_id_at(at) == Some(ui.layer_id())
        });
        let dragged = seen.is_some_and(|seen| seen.dragged(ctx));
        let held = pointed || dragged;
        let now = ctx.input(|i| i.time);
        let lingering = seen.is_some_and(|seen| now - seen.stirred_at < f64::from(SCROLL_LINGER));
        let fade = if held { 0.0 } else { motion(ctx, SCROLL_FADE) };
        let shown = ctx.animate_bool_with_time(key.with("shown"), held || lingering, fade);
        let target = if dragged {
            p.accent
        } else if pointed {
            p.scrollbar_active
        } else {
            p.scrollbar
        };
        let was_hidden = seen.is_none_or(|seen| seen.shown == 0.0);
        let change = if was_hidden {
            0.0
        } else {
            motion(ctx, CHANGE_SECS)
        };
        Self {
            colour: animate_color(ctx, key.with("colour"), target, change),
            widened: ctx.animate_bool_with_time(key.with("wide"), held, change),
            shown,
            held,
        }
    }
}

/// The thumb's width, colour and opacity, in `ui`'s style.
fn style_thumb(ui: &mut Ui, thumb: &Thumb) {
    let scroll = &mut ui.spacing_mut().scroll;
    scroll.floating_width = lerp(SCROLL_THUMB..=SCROLL_THUMB_HOVER, thumb.widened);
    scroll.dormant_handle_opacity = thumb.shown;
    scroll.active_handle_opacity = thumb.shown;
    scroll.interact_handle_opacity = thumb.shown;
    let widgets = &mut ui.visuals_mut().widgets;
    for visuals in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        visuals.fg_stroke.color = thumb.colour;
        visuals.corner_radius = CornerRadius::same(RADIUS_PILL);
    }
}

/// Runs `show` with the thumb's widths and colours in the scroll area's
/// style; `show` hands the content back the style it had.
fn themed<R>(
    ui: &mut Ui,
    area: ScrollArea,
    show: impl FnOnce(&mut Ui, ScrollArea, Content) -> ScrollAreaOutput<R>,
) -> ScrollAreaOutput<R> {
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
    let thumb = Thumb::now(ui, key, seen);

    let content = Content {
        style: ui.style().clone(),
        page: ctx
            .data_mut(|d| d.remove_temp(page_key))
            .unwrap_or_default(),
    };
    let output = ui
        .scope(|ui| {
            style_thumb(ui, &thumb);
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

    let offset = output.state.clamped_offset();
    let moved = seen.is_some_and(|seen| seen.area == output.id && seen.offset != offset);
    let stirred_at = if moved || thumb.held {
        now
    } else {
        seen.map_or(f64::NEG_INFINITY, |seen| seen.stirred_at)
    };
    let linger_left = f64::from(SCROLL_LINGER) - (now - stirred_at);
    if moved {
        ctx.request_repaint();
    } else if !thumb.held && linger_left > 0.0 {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(linger_left));
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
                stirred_at,
                shown: thumb.shown,
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
    use crate::shell::theme;
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
        scroll_area(ui, area, |ui| {
            ui.add(egui::TextEdit::singleline(field).id(Id::unique("field")));
            ui.allocate_space(Vec2::new(10.0, 1000.0));
        })
        .state
        .clamped_offset()
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
        let id = Id::unique("field");
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

    /// One frame at `time`; returns the width and colour of the thumb painted.
    fn painted(
        ctx: &Context,
        time: f64,
        events: Vec<Event>,
        draw: impl FnMut(&mut Ui),
    ) -> Option<(f32, Color32)> {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(PANE))),
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, draw);
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.max.x == PANE - INSET
                        && rect.rect.height() >= SCROLL_THUMB_MIN
                        && rect.fill.a() > 0 =>
                {
                    Some((rect.rect.width(), rect.fill))
                }
                _ => None,
            })
    }

    /// A pane called `salt` of `height`, jumped to `jump` when asked.
    fn pane(ui: &mut Ui, salt: &str, height: f32, jump: Option<f32>) {
        let mut area = ScrollArea::vertical().id_salt(salt).auto_shrink(false);
        if let Some(jump) = jump {
            area = area.vertical_scroll_offset(jump);
        }
        scroll_area(ui, area, |ui| ui.allocate_space(Vec2::new(10.0, height)));
    }

    /// The colours of the thumb's states, as the theme has them.
    fn colours(ctx: &Context) -> &'static theme::Palette {
        let mut seen = None;
        frame(ctx, vec![], |ui| seen = Some(palette(ui)));
        seen.unwrap()
    }

    /// A context with the pane settled, just after time 0.
    fn settled(reduce: bool) -> Context {
        let ctx = themed_ctx();
        theme::set_reduced_motion(&ctx, reduce);
        for at in 0..3 {
            painted(&ctx, f64::from(at) / 60.0, vec![], |ui| {
                pane(ui, "pane", 1000.0, None);
            });
        }
        ctx
    }

    const LINGER: f64 = SCROLL_LINGER as f64;
    const FADE: f64 = SCROLL_FADE as f64;

    #[test]
    fn nothing_shows_at_rest() {
        let ctx = settled(false);
        let thumb = painted(&ctx, 5.0, vec![], |ui| pane(ui, "pane", 1000.0, None));
        assert_eq!(thumb, None);
    }

    #[test]
    fn scrolling_fades_a_narrow_thumb_in_and_out_after_the_linger() {
        let ctx = settled(false);
        let p = colours(&ctx);
        let at = |ctx: &Context, time: f64, jump: Option<f32>| {
            painted(ctx, time, vec![], |ui| pane(ui, "pane", 1000.0, jump))
        };
        at(&ctx, 1.0, Some(300.0));
        let (width, fading_in) = at(&ctx, 1.0 + 1.0 / 60.0, None).expect("it fades in");
        assert_eq!(width, SCROLL_THUMB);
        assert!(fading_in.a() < 255, "not there at once: {fading_in:?}");
        for step in 2..30 {
            at(&ctx, 1.0 + f64::from(step) / 60.0, None);
        }
        let (width, colour) = at(&ctx, 1.0 + LINGER - 0.1, None).expect("it lingers");
        assert_eq!((width, colour), (SCROLL_THUMB, p.scrollbar));
        at(&ctx, 1.0 + LINGER + 0.01, None);
        let (_, fading_out) = at(&ctx, 1.0 + LINGER + 0.02, None).expect("it fades out");
        assert!(fading_out.a() < 255, "not gone at once: {fading_out:?}");
        for step in 0..20 {
            at(&ctx, 1.0 + LINGER + 0.03 + f64::from(step) / 60.0, None);
        }
        assert_eq!(at(&ctx, 1.0 + LINGER + FADE + 1.0, None), None);
    }

    #[test]
    fn reduced_motion_skips_the_fades() {
        let ctx = settled(true);
        let p = colours(&ctx);
        let at = |ctx: &Context, time: f64, jump: Option<f32>| {
            painted(ctx, time, vec![], |ui| pane(ui, "pane", 1000.0, jump))
        };
        at(&ctx, 1.0, Some(300.0));
        assert_eq!(
            at(&ctx, 1.0 + 1.0 / 60.0, None),
            Some((SCROLL_THUMB, p.scrollbar)),
            "in one frame"
        );
        at(&ctx, 1.0 + LINGER + 0.01, None);
        assert_eq!(at(&ctx, 1.0 + LINGER + 0.02, None), None, "out in one");
    }

    #[test]
    fn the_edge_strip_shows_the_wide_thumb_at_once_until_the_linger_ends() {
        let ctx = settled(false);
        let p = colours(&ctx);
        let edge = Pos2::new(PANE - SCROLL_HIT + 1.0, 100.0);
        let draw = |ui: &mut Ui| pane(ui, "pane", 1000.0, None);
        painted(&ctx, 1.0, vec![Event::PointerMoved(edge)], draw);
        assert_eq!(
            painted(&ctx, 1.0 + 1.0 / 60.0, vec![], draw),
            Some((SCROLL_THUMB_HOVER, p.scrollbar_active)),
            "at once"
        );
        assert!(
            painted(&ctx, 4.0, vec![], draw).is_some(),
            "it stays while the pointer does"
        );
        let away = Pos2::new(50.0, 100.0);
        painted(&ctx, 4.1, vec![Event::PointerMoved(away)], draw);
        assert!(
            painted(&ctx, 4.0 + LINGER - 0.1, vec![], draw).is_some(),
            "it lingers once the pointer left"
        );
        painted(&ctx, 4.1 + LINGER + 0.01, vec![], draw);
        for step in 0..20 {
            painted(&ctx, 4.2 + LINGER + f64::from(step) / 60.0, vec![], draw);
        }
        assert_eq!(painted(&ctx, 7.0, vec![], draw), None);
    }

    #[test]
    fn a_drag_is_wide_and_accent_even_with_the_pointer_out_of_the_strip() {
        let ctx = settled(true);
        let p = colours(&ctx);
        let on = Pos2::new(PANE - 3.0, 10.0);
        let out = Pos2::new(50.0, 60.0);
        let draw = |ui: &mut Ui| pane(ui, "pane", 1000.0, None);
        painted(&ctx, 1.0, vec![Event::PointerMoved(on)], draw);
        painted(&ctx, 1.1, vec![Event::PointerMoved(on)], draw);
        painted(&ctx, 1.2, vec![button(on, true)], draw);
        painted(&ctx, 1.3, vec![Event::PointerMoved(out)], draw);
        let dragging = Some((SCROLL_THUMB_HOVER, p.accent));
        assert_eq!(painted(&ctx, 1.4, vec![], draw), dragging);
        assert_eq!(
            painted(&ctx, 5.0, vec![], draw),
            dragging,
            "no linger ends it"
        );
    }

    #[test]
    fn a_pane_with_room_to_spare_has_no_thumb() {
        let ctx = themed_ctx();
        let draw = |ui: &mut Ui| pane(ui, "pane", 10.0, None);
        let edge = Pos2::new(PANE - 3.0, 100.0);
        for at in 0..3 {
            painted(&ctx, f64::from(at), vec![Event::PointerMoved(edge)], draw);
        }
        assert_eq!(painted(&ctx, 4.0, vec![], draw), None);
    }

    #[test]
    fn the_strip_under_something_on_top_does_not_wake_the_thumb() {
        let ctx = settled(true);
        let edge = Pos2::new(PANE - 3.0, 100.0);
        let draw = |ui: &mut Ui| {
            pane(ui, "pane", 1000.0, None);
            egui::Area::new(Id::unique("over"))
                .order(egui::Order::Foreground)
                .fixed_pos(Pos2::ZERO)
                .show(ui.ctx(), |ui| ui.allocate_space(Vec2::splat(PANE)));
        };
        painted(&ctx, 1.0, vec![Event::PointerMoved(edge)], draw);
        for at in 2..5 {
            painted(&ctx, f64::from(at), vec![], draw);
        }
        assert_eq!(painted(&ctx, 6.0, vec![], draw), None);
    }

    #[test]
    fn opening_another_pane_in_the_same_place_is_no_scrolling() {
        let ctx = settled(true);
        painted(&ctx, 1.0, vec![], |ui| {
            pane(ui, "pane", 1000.0, Some(300.0))
        });
        painted(&ctx, 5.0, vec![], |ui| pane(ui, "pane", 1000.0, None));
        painted(&ctx, 6.0, vec![], |ui| pane(ui, "other", 1000.0, None));
        let thumb = painted(&ctx, 6.1, vec![], |ui| pane(ui, "other", 1000.0, None));
        assert_eq!(thumb, None);
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
                    colour = animate_color(ui.ctx(), Id::unique("thumb"), target, secs);
                });
            }
            colour
        };
        assert_eq!(eased(true), Color32::WHITE);
        assert_ne!(eased(false), Color32::WHITE);
    }
}
