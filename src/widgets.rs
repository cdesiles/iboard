//! Controls of the design system that egui does not draw the same way:
//! buttons, selects, menus, pills, the pieces dialogs share, the checkbox,
//! keyboard hints and breadcrumbs. See `design/components/*/README.md` for
//! what each one is for.

use egui::{
    Color32, CornerRadius, FontId, Galley, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2, WidgetText,
};
use std::sync::Arc;
use std::time::Duration;

use crate::theme::{self, Icon};

mod button;
mod dialog;
mod dynamic_button;
mod key_hints;
mod menu;
mod modal;
mod pill;
mod select;

pub use button::{
    ButtonKind, CARD_ACTION, Tone, action_button, button, hover_fade, icon_button, split_button,
    text_button_width, wide_button,
};
pub use dialog::{
    Answer, Ask, FooterClick, confirm, confirm_buttons, dashed_rect, discard_bar, enter_confirms,
    field_edit, field_label, focus_halo, focus_ring, footer_bar, help, modal_footer,
    modal_footer_with, modal_frame, modal_header, paint_focus_halo, paint_focus_ring,
    popover_frame, rule, segmented, text_entry, text_field,
};
pub use dynamic_button::DynamicButton;
pub use key_hints::{key_hints, key_hints_shown, sliding_hints, sliding_kbd};
pub use menu::{
    ActionTone, Drag, MENU_ROW_H, MenuAction, MenuOption, RowLook, menu_action, menu_group,
    menu_head, menu_note, menu_option, menu_row, menu_row_lifted, menu_separator,
};
pub use modal::{focus_here, modal, modal_body_max, settle_modals};
pub use pill::{
    chip, filter_token, label_pill, label_pill_lit, pill, review_tag, tag, unfocusable_label_pill,
};
pub use select::select;

fn galley(ui: &Ui, text: &str, font: FontId, color: Color32) -> Arc<Galley> {
    WidgetText::from(egui::RichText::new(text).font(font).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    )
}

/// The Thinking mark, `size` square (28 in panels, 14 inline): an agent is
/// at work. It stands still when reduced motion is on.
pub fn thinking(ui: &mut Ui, size: f32) -> Response {
    let p = theme::palette(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Unknown, true, "Working"));
    let time = (!theme::reduced_motion(ui.ctx())).then(|| ui.input(|i| i.time));
    if time.is_some() {
        ui.ctx().request_repaint();
    }
    theme::thinking_mark(ui.painter(), rect, time, p.ink, p.accent);
    response
}

/// A live region nobody sees: VoiceOver says `text` when it appears and when
/// it changes, and only then.
pub fn announce(ui: &Ui, salt: &str, text: &str) {
    let id = ui.scope_id().with(salt);
    let response = ui.interact(Rect::ZERO, id, Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
    ui.ctx().accesskit_node_builder(id, |node| {
        node.set_live(egui::accesskit::Live::Polite);
    });
}

/// What the screen reader hears while an agent works: `status` once, and the
/// time spent every 10s, not every second.
pub fn announce_thinking(ui: &Ui, status: &str, elapsed: Duration) {
    announce(ui, "thinking-status", status);
    let tens = elapsed.as_secs() / 10 * 10;
    if tens > 0 {
        announce(ui, "thinking-elapsed", &format!("{tens} seconds"));
    }
}

/// Reports `ui`'s own node (a Modal's dialog, say) as busy while `busy`.
pub fn mark_busy(ui: &Ui, busy: bool) {
    if busy {
        ui.ctx()
            .accesskit_node_builder(ui.unique_id(), |node| node.set_busy());
    }
}

/// One pulsing Skeleton bar, `share` of the line wide.
pub fn skeleton_bar(ui: &mut Ui, share: f32, height: f32) {
    let p = theme::palette(ui);
    let width = ui.available_width() * share;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::RADIUS_SM),
        theme::skeleton_fill(ui.ctx(), p),
    );
}

/// The 14pt checkbox: `accent` fill and an `on-accent` tick when checked.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, label: &str) -> Response {
    let ink = theme::palette(ui).ink;
    checkbox_named(ui, checked, label, label, (theme::ui_button(), ink))
}

/// A [`checkbox`] that sits inside a row, its text in `meta` `ink-muted`.
pub fn quiet_checkbox(ui: &mut Ui, checked: &mut bool, label: &str) -> Response {
    let ink = theme::palette(ui).ink_muted;
    checkbox_named(ui, checked, label, label, (theme::meta(), ink))
}

/// How wide a [`quiet_checkbox`] is.
pub fn quiet_checkbox_width(ui: &Ui, label: &str) -> f32 {
    CHECKBOX_SIZE + 6.0 + text_width(ui, label, theme::meta())
}

/// A [`checkbox`] with no text beside it, read aloud as `name`: what it ticks
/// is the field next to it.
pub fn bare_checkbox(ui: &mut Ui, checked: &mut bool, name: &str) -> Response {
    let ink = theme::palette(ui).ink;
    checkbox_named(ui, checked, "", name, (theme::ui_button(), ink))
}

const CHECKBOX_SIZE: f32 = 14.0;

fn checkbox_named(
    ui: &mut Ui,
    checked: &mut bool,
    label: &str,
    name: &str,
    (font, ink): (FontId, Color32),
) -> Response {
    let p = theme::palette(ui);
    let text = galley(ui, label, font, ink);
    let box_size = CHECKBOX_SIZE;
    let width = if label.is_empty() {
        box_size
    } else {
        box_size + 6.0 + text.size().x
    };
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *checked, name)
    });

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let box_rect = Rect::from_min_size(
            Pos2::new(rect.min.x, rect.center().y - box_size / 2.0),
            Vec2::splat(box_size),
        );
        if *checked {
            painter.rect_filled(box_rect, CornerRadius::same(3), p.accent);
            let at = |x: f32, y: f32| box_rect.min + Vec2::new(x, y);
            painter.line(
                vec![at(3.5, 7.0), at(6.0, 9.5), at(10.5, 4.5)],
                Stroke::new(2.0, p.on_accent),
            );
        } else {
            painter.rect(
                box_rect,
                CornerRadius::same(3),
                p.surface,
                Stroke::new(1.0, p.line_strong),
                StrokeKind::Inside,
            );
        }
        let pos = Pos2::new(box_rect.max.x + 6.0, rect.center().y - text.size().y / 2.0);
        painter.galley(pos, text, ink);
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// Draws a key cap (`⌘S`, `esc`) at `pos` (left, vertically centred) and
/// returns its width. Used where the hint sits on top of another widget.
pub fn paint_kbd(ui: &Ui, key: &str, left_center: Pos2) -> f32 {
    let p = theme::palette(ui);
    paint_kbd_in(ui, key, left_center, p.ink_muted)
}

/// A key cap in its own ink: `accent` on a picked choice. A hint, not a
/// control, so `surface-sunken` and no edge.
pub fn paint_kbd_in(ui: &Ui, key: &str, left_center: Pos2, ink: Color32) -> f32 {
    let p = theme::palette(ui);
    let text = galley(ui, key, theme::meta_mono(), ink);
    let size = Vec2::new((text.size().x + 8.0).max(18.0), 16.0);
    let rect = Rect::from_min_size(Pos2::new(left_center.x, left_center.y - size.y / 2.0), size);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_sunken);
    painter.galley(rect.center() - text.size() / 2.0, text, ink);
    size.x
}

/// A key cap followed by what it does, as a hint row in the status bar. Its
/// caller draws it only while the hints are out, through [`sliding_hints`].
pub fn hint(ui: &mut Ui, key: &str, action: &str) {
    let p = theme::palette(ui);
    let text = galley(ui, key, theme::meta_mono(), p.ink_muted);
    let key_width = (text.size().x + 8.0).max(18.0);
    ui.spacing_mut().item_spacing.x = theme::SPACE_2;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(key_width, 16.0), Sense::hover());
    paint_kbd(ui, key, Pos2::new(rect.min.x, rect.center().y));
    ui.label(
        egui::RichText::new(action)
            .font(theme::meta())
            .color(p.ink_muted),
    );
}

/// The room a [`crumb`] keeps on each side of its text. A plain segment after
/// a separator adds it, so the trail reads evenly.
pub const CRUMB_PAD: f32 = 6.0;

/// The id of [`project_trigger`], for the keys that open its menu.
pub fn project_trigger_id() -> egui::Id {
    egui::Id::unique("project-trigger")
}

/// The project menu's trigger: the project's name and a chevron. Transparent
/// at rest; `surface-hover` with no edge on hover and while the menu is open.
/// A long name truncates into the width left.
pub fn project_trigger(ui: &mut Ui, project: &str, open: bool) -> Response {
    const HEIGHT: f32 = 28.0;
    const PAD: f32 = 6.0;
    const GAP: f32 = 4.0;
    let p = theme::palette(ui);
    let chrome = 2.0 * PAD + GAP + theme::ICON_SIZE;
    let text = text_width(ui, project, theme::ui_title()).ceil();
    let width = (text + chrome).min(ui.available_width());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, HEIGHT), Sense::hover());
    let response = ui
        .interact(rect, project_trigger_id(), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Switch project")
    });
    let fade = hover_fade(ui.ctx(), response.id, response.hovered());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::RADIUS_SM),
        p.surface_hover
            .gamma_multiply(if open { 1.0 } else { fade }),
    );
    let name_rect = Rect::from_min_size(
        rect.min + Vec2::new(PAD, 0.0),
        Vec2::new(width - chrome, HEIGHT),
    );
    let mut name = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(name_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let label = egui::RichText::new(project)
        .font(theme::ui_title())
        .color(p.ink);
    name.add(egui::Label::new(label).truncate());
    let icon = Rect::from_center_size(
        Pos2::new(rect.max.x - PAD - theme::ICON_SIZE / 2.0, rect.center().y),
        Vec2::splat(theme::ICON_SIZE),
    );
    theme::paint_icon(ui.painter(), Icon::ChevronDown, icon, p.ink_muted);
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// Rotates what a layer painted this frame about `origin`, to lift something
/// off the page. Rects and text can't turn in epaint, so each shape becomes a
/// mesh first.
pub fn tilt_layer(ctx: &egui::Context, layer: egui::LayerId, origin: egui::Pos2, degrees: f32) {
    let font_size = ctx.fonts(|fonts| fonts.font_image_size());
    let options = ctx.tessellation_options(|options| *options);
    let mut tessellator =
        egui::epaint::Tessellator::new(ctx.pixels_per_point(), options, font_size, Vec::new());
    let rotation = egui::emath::Rot2::from_angle(degrees.to_radians());
    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        for index in 0..list.next_idx().0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                let mut mesh = egui::epaint::Mesh::default();
                let shape = std::mem::replace(&mut clipped.shape, egui::Shape::Noop);
                tessellator.tessellate_shape(shape, &mut mesh);
                mesh.rotate(rotation, origin);
                clipped.shape = egui::Shape::mesh(mesh);
                clipped.clip_rect = Rect::EVERYTHING;
            });
        }
    });
}

/// A breadcrumb segment that links back: `ink-muted`, `surface-hover` on hover.
pub fn crumb(ui: &mut Ui, text: &str, font: FontId) -> Response {
    crumb_marked(ui, text, font, None)
}

/// The room a [`crumb_marked`]'s mark takes, and the gap after it.
const CRUMB_MARK: f32 = theme::STATUS_ICON + CRUMB_PAD;

/// A [`crumb`] with a column's status mark before its text.
pub fn crumb_marked(
    ui: &mut Ui,
    text: &str,
    font: FontId,
    mark: Option<theme::Status>,
) -> Response {
    let p = theme::palette(ui);
    let normal = galley(ui, text, font.clone(), p.ink_muted);
    let mark_w = if mark.is_some() { CRUMB_MARK } else { 0.0 };
    // A 24pt hit area, however small the text.
    let size = Vec2::new(mark_w + normal.size().x + 2.0 * CRUMB_PAD, theme::CONTROL_H);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, ui.is_enabled(), text));
    if ui.is_rect_visible(rect) {
        let text_rect = Rect::from_min_size(
            Pos2::new(
                rect.min.x + CRUMB_PAD + mark_w,
                rect.center().y - normal.size().y / 2.0,
            ),
            normal.size(),
        );
        let fade = hover_fade(ui.ctx(), response.id, response.hovered());
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            p.surface_hover.gamma_multiply(fade),
        );
        if let Some(status) = mark {
            let icon = Rect::from_center_size(
                Pos2::new(
                    rect.min.x + CRUMB_PAD + theme::STATUS_ICON / 2.0,
                    rect.center().y,
                ),
                Vec2::splat(theme::STATUS_ICON),
            );
            theme::status_icon(ui.painter(), status, icon, p);
        }
        let color = if response.hovered() {
            // Underlined 3pt below the baseline-ish bottom of the text.
            let y = text_rect.max.y + 1.0;
            ui.painter().line_segment(
                [Pos2::new(text_rect.min.x, y), Pos2::new(text_rect.max.x, y)],
                Stroke::new(1.0, p.ink),
            );
            p.ink
        } else {
            p.ink_muted
        };
        ui.painter()
            .galley_with_override_text_color(text_rect.min, normal, color);
    }
    focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

/// A galley clipped to `max_width`, ending in an ellipsis when it overflows.
fn galley_truncated(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    WidgetText::from(egui::RichText::new(text).font(font).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        max_width,
        egui::FontSelection::Default,
    )
}

/// The width `text` takes in `font`, for laying out a row before drawing it.
pub fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    galley(ui, text, font, Color32::PLACEHOLDER).size().x
}

/// Tallest a ghost tile stretches to fill its cell.
const GHOST_MAX_H: f32 = 96.0;

/// A ghost tile for a section the issue lacks: a dashed `line-strong` edge,
/// `+` and the heading in `ui-strong` `ink-muted`, then a `ui-small` hint. On
/// hover it turns solid `accent` on `accent-soft`.
pub fn ghost_section(ui: &mut Ui, heading: &str, hint: Option<&str>) -> Response {
    let p = theme::palette(ui);
    let pad = Vec2::new(theme::SPACE_4, theme::SPACE_3);
    let width = ui.available_width();
    let hint = hint.map(|hint| {
        let mut job = egui::text::LayoutJob::simple(
            hint.to_string(),
            theme::ui_small(),
            p.ink_muted,
            width - 2.0 * pad.x,
        );
        job.wrap.max_rows = 2;
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    });
    let head_h = 20.0;
    // Fills a grid cell, so tiles in a row line up whatever their hint.
    let height = (2.0 * pad.y + head_h + hint.as_ref().map_or(0.0, |h| 2.0 + h.size().y))
        .max(ui.available_height().min(GHOST_MAX_H));
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), heading));
    if ui.is_rect_visible(rect) {
        let lit = response.hovered() || response.has_focus();
        let ink = if lit { p.accent } else { p.ink_muted };
        let painter = ui.painter();
        let radius = CornerRadius::same(theme::RADIUS_SM);
        if lit {
            painter.rect(
                rect,
                radius,
                p.accent_soft,
                Stroke::new(1.0, p.accent),
                StrokeKind::Inside,
            );
        } else {
            dashed_rect(painter, rect, theme::RADIUS_SM, p.line_strong);
        }
        let head_center = rect.min.y + pad.y + head_h / 2.0;
        let icon = Rect::from_center_size(
            Pos2::new(rect.min.x + pad.x + theme::ICON_SIZE / 2.0, head_center),
            Vec2::splat(theme::ICON_SIZE),
        );
        theme::paint_icon(painter, Icon::Plus, icon, ink);
        let title = galley(ui, heading, theme::ui_strong(), ink);
        painter.galley(
            Pos2::new(icon.max.x + 6.0, head_center - title.size().y / 2.0),
            title,
            ink,
        );
        if let Some(hint) = hint {
            painter.galley(
                Pos2::new(rect.min.x + pad.x, rect.min.y + pad.y + head_h + 2.0),
                hint,
                p.ink_muted,
            );
        }
    }
    response
}

/// A 48pt `status-done` meter on a `surface-sunken` track, `done` of `total`.
pub fn meter(ui: &mut Ui, done: usize, total: usize) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(48.0, 4.0), Sense::hover());
    let painter = ui.painter();
    let radius = CornerRadius::same(2);
    painter.rect(
        rect,
        radius,
        p.surface_sunken,
        Stroke::new(1.0, p.line),
        StrokeKind::Inside,
    );
    if total > 0 && done > 0 {
        let share = (done as f32 / total as f32).min(1.0);
        let filled = Rect::from_min_size(rect.min, Vec2::new(rect.width() * share, rect.height()));
        painter.rect_filled(filled, radius, p.status_done);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::preferences::labels::{LabelColor, LabelStyle, Rgb};

    #[test]
    fn a_key_cap_is_sunken_with_no_edge() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let mut palette = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            palette = Some(theme::palette(ui));
            paint_kbd(ui, "⌘S", Pos2::new(10.0, 10.0));
        });
        output.textures_delta.clear();
        let p = palette.unwrap();
        let cap = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect) => Some(rect.clone()),
                _ => None,
            });
        let cap = cap.expect("a key cap paints a box");
        assert_eq!(cap.fill, p.surface_sunken);
        assert_eq!(cap.stroke.width, 0.0);
    }

    #[test]
    fn modal_menu_and_popover_frames_use_line() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = theme::palette(ui);
            assert_eq!(popover_frame(ui).stroke.color, p.line);
            assert_eq!(modal_frame(ui).stroke.color, p.line);
            assert_eq!(ui.visuals().window_stroke.color, p.line);
        })
        .textures_delta
        .clear();
    }

    #[test]
    fn a_menu_row_renders_its_two_lines() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
            ..Default::default()
        };
        ctx.run_ui(raw, |ui| {
            let look = RowLook {
                current: true,
                ..RowLook::default()
            };
            let current = menu_row(ui, 300.0, "moon-cull", "~/code/moon-cull", look);
            assert!(current.rect.width() > 0.0 && current.rect.height() > 0.0);
            let other = menu_row(ui, 300.0, "other", "~/code/other", RowLook::default());
            assert!(other.rect.width() > 0.0);
            let off_a_char = RowLook {
                active: true,
                mark: Some(1..3),
                drag: Some(Drag::Placeholder),
                ..RowLook::default()
            };
            let left = menu_row(ui, 300.0, "über", "", off_a_char);
            assert_eq!(left.rect.height(), MENU_ROW_H);
            menu_row_lifted(ui, ("über", "~/code/über"), left.rect.center());
        })
        .textures_delta
        .clear();
    }

    #[test]
    fn a_lifted_row_is_tilted() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            menu_row_lifted(ui, ("moon-cull", "~/code"), egui::pos2(100.0, 100.0));
        });
        output.textures_delta.clear();
        let meshes = output
            .shapes
            .iter()
            .filter(|clipped| matches!(clipped.shape, egui::Shape::Mesh(_)))
            .count();
        assert!(meshes >= 2, "rect and text turned into meshes: {meshes}");
    }

    #[test]
    fn a_label_pill_renders_known_unknown_and_faint_colours() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
            ..Default::default()
        };
        ctx.run_ui(raw, |ui| {
            // A colour, an unknown label and a nearly-white colour that must
            // fall back to readable ink all draw at the same 18pt size, loud or
            // not.
            for (name, color) in [
                ("bug", Some(LabelColor::Own(Rgb(0xb1, 0x2f, 0x22)))),
                ("spike", None),
                ("faint", Some(LabelColor::Own(Rgb(0xf5, 0xf5, 0xf5)))),
            ] {
                let quiet = LabelStyle {
                    color,
                    stand_out: false,
                };
                let loud = LabelStyle {
                    color,
                    stand_out: true,
                };
                assert_eq!(label_pill(ui, name, loud).rect.height(), 18.0);
                let response = label_pill(ui, name, quiet);
                assert_eq!(response.rect.height(), 18.0);
                assert!(response.rect.width() > 0.0);
            }
        })
        .textures_delta
        .clear();
    }

    /// The status is said once, the time from 10s and in tens of seconds, so
    /// the live node only changes every 10s.
    #[test]
    fn thinking_announces_its_status_and_every_tenth_second() {
        use egui::accesskit::Live;
        use egui_kittest::kittest::{NodeT, Queryable};
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, seconds: &mut u64| {
                announce_thinking(ui, "Reading", Duration::from_secs(*seconds));
            },
            9,
        );
        harness.run();
        let status = harness.get_by_label("Reading");
        assert_eq!(status.accesskit_node().live(), Live::Polite);
        assert!(harness.query_by_label_contains("seconds").is_none());

        for seconds in [10, 19] {
            *harness.state_mut() = seconds;
            harness.run();
            let time = harness.get_by_label("10 seconds");
            assert_eq!(time.accesskit_node().live(), Live::Polite);
        }
        *harness.state_mut() = 20;
        harness.run();
        harness.get_by_label("20 seconds");
    }

    /// ConfirmDialog's padding is 20pt on every side, under the buttons too.
    #[test]
    fn the_confirm_dialog_keeps_20pt_under_its_buttons() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, themed: &mut bool| {
                if !std::mem::replace(themed, true) {
                    theme::install(ui.ctx());
                    return;
                }
                confirm(
                    ui,
                    &Ask {
                        id: "ask",
                        question: "Delete this issue?",
                        title: "An issue",
                        detail: "todo/an-issue.md",
                        consequence: "The file is moved to the macOS Trash.",
                        confirm: "Delete",
                    },
                );
            },
            false,
        );
        harness.run_steps(3);
        let modal = harness
            .ctx
            .memory(|m| m.area_rect(egui::Id::unique("ask")))
            .expect("the modal is up");
        let delete = harness.get_by_role_and_label(Role::Button, "Delete").rect();
        let cancel = harness.get_by_role_and_label(Role::Button, "Cancel").rect();
        assert_eq!(modal.max.y - delete.max.y, 21.0, "20pt and its 1pt edge");
        assert_eq!(modal.max.y - cancel.max.y, 21.0);
    }

    /// Pills, chips, tags and the select report their role and what they
    /// show, the token's × its tooltip, so VoiceOver reads them and a test
    /// finds them by label.
    #[test]
    fn pills_and_the_select_are_found_by_their_label() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, (themed, clicked): &mut (bool, Vec<&'static str>)| {
                if !std::mem::replace(themed, true) {
                    theme::install(ui.ctx());
                    return;
                }
                if label_pill(ui, "Bug", LabelStyle::default()).clicked() {
                    clicked.push("bug");
                }
                if filter_token(ui, "ux").clicked() {
                    clicked.push("ux");
                }
                chip(ui, "feature", false);
                tag(ui, "on-device");
                select(
                    ui,
                    egui::Id::unique("tool"),
                    "Claude Code",
                    Some("claude"),
                    false,
                );
            },
            (false, Vec::new()),
        );
        harness.get_by_role_and_label(Role::Button, "feature");
        harness.get_by_role_and_label(Role::Button, "on-device");
        harness.get_by_role_and_label(Role::ComboBox, "Claude Code");

        harness.get_by_label("bug").click();
        harness.run();
        harness.get_by_label("Remove ux").click();
        harness.run();
        assert_eq!(harness.state().1, ["bug", "ux"]);
    }
}
