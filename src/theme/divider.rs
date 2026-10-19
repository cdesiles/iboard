//! The PaneDivider: the gutter between two side-by-side panes is the resize
//! handle (see `design/components/PaneDivider`).

use std::ops::RangeInclusive;

use egui::{
    Align2, Context, CornerRadius, CursorIcon, Id, Key, Modifiers, Pos2, Rect, Response, Sense,
    Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};

use super::{RADIUS_PILL, RADIUS_SM, meta, motion, palette};

/// A drag within this distance of the default split lands on it.
const SNAP: f32 = 8.0;
/// The pointer rests this long on the gutter before its rule shows.
const HOVER_DELAY: f64 = 0.15;
const STEP: f32 = 16.0;
const BIG_STEP: f32 = 80.0;

/// The right pane's width for the pair of panes `id`, as last set by its
/// [`pane_divider`]; `default` until then.
pub fn pane_split(ctx: &Context, id: Id, default: f32) -> f32 {
    ctx.data(|d| d.get_temp(id)).unwrap_or(default)
}

/// Paints and handles the divider in `gutter`, the strip between two panes
/// laid out side by side in `panes`. It sets the right pane's width, kept in
/// `range` and remembered under `id` for [`pane_split`]. `inline` is the
/// variant inside one card: a 1pt `line` that thickens under the pointer.
pub fn pane_divider(
    ui: &mut Ui,
    id: Id,
    panes: Rect,
    gutter: Rect,
    range: RangeInclusive<f32>,
    default: f32,
    inline: bool,
) -> Response {
    let ctx = ui.ctx().clone();
    let clamp = |width: f32| width.clamp(*range.start(), range.end().max(*range.start()));
    let mut width = clamp(pane_split(&ctx, id, default));
    let response = ui.interact(gutter, id, Sense::click_and_drag());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, "Resize the panes"));

    let raw_id = id.with("raw");
    if response.drag_started() {
        ctx.data_mut(|d| d.insert_temp(raw_id, width));
    }
    if response.dragged() {
        let raw = ctx.data(|d| d.get_temp(raw_id)).unwrap_or(width) - response.drag_delta().x;
        ctx.data_mut(|d| d.insert_temp(raw_id, raw));
        width = if (raw - default).abs() <= SNAP {
            default
        } else {
            raw
        };
    }
    if response.double_clicked() {
        width = default;
    }
    if response.has_focus() {
        width = keys(&ctx, width, default);
    }
    let width = clamp(width);
    if pane_split(&ctx, id, default) != width {
        ctx.data_mut(|d| d.insert_temp(id, width));
        ctx.request_repaint();
    }
    if response.hovered() || response.dragged() {
        ctx.set_cursor_icon(CursorIcon::ResizeColumn);
    }
    paint(ui, &response, panes, gutter, width, inline);
    response
}

/// `← →` move the divider 16pt, with `⇧` 80pt; `↩` puts it back.
fn keys(ctx: &Context, width: f32, default: f32) -> f32 {
    ctx.input_mut(|i| {
        let mut width = width;
        for (modifiers, step) in [(Modifiers::SHIFT, BIG_STEP), (Modifiers::NONE, STEP)] {
            if i.consume_key(modifiers, Key::ArrowLeft) {
                width += step;
            }
            if i.consume_key(modifiers, Key::ArrowRight) {
                width -= step;
            }
        }
        if i.consume_key(Modifiers::NONE, Key::Enter) {
            width = default;
        }
        width
    })
}

/// The dot grip, and the rule that shows under the pointer (after a short
/// delay, so passing through does not flash), while dragging and focused.
fn paint(ui: &Ui, response: &Response, panes: Rect, gutter: Rect, width: f32, inline: bool) {
    let p = palette(ui);
    let ctx = ui.ctx();
    let active = response.dragged() || response.has_focus();
    let since_id = response.id.with("hover-since");
    let now = ctx.input(|i| i.time);
    let since = if response.hovered() {
        let since = ctx.data(|d| d.get_temp(since_id)).unwrap_or(now);
        ctx.data_mut(|d| d.insert_temp(since_id, since));
        Some(since)
    } else {
        ctx.data_mut(|d| d.remove::<f64>(since_id));
        None
    };
    let waited = since.is_some_and(|since| now - since >= HOVER_DELAY);
    if since.is_some() && !waited {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(HOVER_DELAY));
    }
    let shown = ctx.animate_bool_with_time(response.id.with("rule"), waited, motion(ctx, 0.12));

    let painter = ui.painter();
    let x = gutter.center().x;
    let grip = Rect::from_center_size(Pos2::new(x, gutter.center().y), Vec2::new(10.0, 18.0));
    let (rule, dots) = if active {
        (Some(p.accent), p.accent)
    } else if shown > 0.0 {
        (Some(p.line_strong.gamma_multiply(shown)), p.ink_muted)
    } else {
        (None, p.ink_faint)
    };
    if inline && rule.is_none() {
        painter.vline(x, gutter.y_range(), Stroke::new(1.0, p.line));
    }
    if let Some(color) = rule {
        let stroke = Stroke::new(2.0, color);
        painter.vline(x, gutter.top()..=grip.top() - 2.0, stroke);
        painter.vline(x, grip.bottom() + 2.0..=gutter.bottom(), stroke);
    }
    if inline || rule.is_some() {
        let ground = if inline { p.surface } else { p.bg };
        painter.rect_filled(grip, CornerRadius::same(RADIUS_PILL), ground);
    }
    for column in [-2.5, 2.5] {
        for row in [-5.0, 0.0, 5.0] {
            painter.circle_filled(grip.center() + Vec2::new(column, row), 1.25, dots);
        }
    }
    if response.has_focus() {
        painter.rect_stroke(
            grip.expand(2.0),
            CornerRadius::same(RADIUS_PILL),
            Stroke::new(2.0, p.accent),
            StrokeKind::Outside,
        );
    }
    if active {
        let left = gutter.left() - panes.left();
        let text = format!("{:.0} · {:.0}", left.max(0.0), width);
        let galley = painter.layout_no_wrap(text, meta(), p.ink);
        let at = Pos2::new(x, grip.top() - 22.0);
        let rect = Align2::CENTER_CENTER
            .anchor_size(at, galley.size())
            .expand2(Vec2::new(6.0, 1.0));
        painter.rect(
            rect,
            CornerRadius::same(RADIUS_SM),
            p.surface,
            Stroke::new(1.0, p.line_strong),
            StrokeKind::Inside,
        );
        painter.galley(rect.min + Vec2::new(6.0, 1.0), galley, p.ink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// `←` widens the right pane by 16pt, `⇧←` by 80 up to its range, and
    /// `↩` puts the split back to its default.
    #[test]
    fn keys_move_the_divider_within_its_range() {
        let id = Id::new("split");
        let mut harness = Harness::new_ui_state(
            |ui, themed: &mut bool| {
                if !std::mem::replace(themed, true) {
                    super::super::install(ui.ctx());
                    return;
                }
                let panes = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 400.0));
                let gutter = Rect::from_x_y_ranges(400.0..=416.0, 0.0..=400.0);
                pane_divider(ui, id, panes, gutter, 300.0..=480.0, 384.0, false);
            },
            false,
        );
        harness.run();
        harness.get_by_label("Resize the panes").focus();
        harness.run();
        let split = |harness: &Harness<bool>| pane_split(&harness.ctx, id, 384.0);

        harness.key_press(Key::ArrowLeft);
        harness.run();
        assert_eq!(split(&harness), 400.0);
        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft);
        harness.run();
        assert_eq!(split(&harness), 480.0, "kept in its range");
        harness.key_press(Key::Enter);
        harness.run();
        assert_eq!(split(&harness), 384.0);
    }
}
