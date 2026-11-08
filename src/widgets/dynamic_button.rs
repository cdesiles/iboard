//! Dynamic icon buttons, the header's Edit, Add label and Delete: a 24pt icon
//! at rest, with a pill holding the label left of the row under the pointer or
//! the focus and the key sliding out while ⌘ is held. See
//! `design/components/HeaderBar`.

use egui::{
    Color32, CornerRadius, Id, LayerId, Order, Painter, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, Widget,
};

use super::{galley, key_hints};
use crate::theme::{self, Icon};

/// The icon's side padding in its 24pt box.
const ICON_PAD: f32 = 5.0;
const KEY_GAP: f32 = 4.0;
/// The pill's room on each side of its label, and its gap to the row.
const PILL_PAD: f32 = theme::SPACE_3;
const PILL_GAP: f32 = theme::SPACE_1;
/// The pill's label: `ui-strong` at 12pt.
const PILL_TEXT: f32 = 12.0;

/// A dynamic icon button. Its label is what a screen reader hears and what
/// the pill says; it has no tooltip, since the pill is the label.
pub struct DynamicButton<'a> {
    icon: Icon,
    label: &'a str,
    key: Option<&'a str>,
    danger: bool,
    open: bool,
}

impl<'a> DynamicButton<'a> {
    #[must_use]
    pub fn new(icon: Icon, label: &'a str) -> Self {
        Self {
            icon,
            label,
            key: None,
            danger: false,
            open: false,
        }
    }

    /// The key that slides out after the icon while ⌘ is held.
    #[must_use]
    pub fn key(mut self, key: Option<&'a str>) -> Self {
        self.key = key;
        self
    }

    /// Delete: `danger` on `danger-soft` under the pointer, the only red in
    /// the header.
    #[must_use]
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// The trigger of what is open: `accent` on `accent-soft`, hovered or not.
    #[must_use]
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    fn lit(&self, p: &theme::Palette) -> (Color32, Color32) {
        if self.open {
            (p.accent_soft, p.accent)
        } else if self.danger {
            (p.danger_soft, p.danger)
        } else {
            (p.surface_hover, p.ink)
        }
    }
}

/// Where the button showing its pill, and the pass it last did, are kept.
fn shown_id() -> Id {
    Id::unique("dynamic-button-shown")
}

/// Where the row's left edge is kept: this pass's so far, and the last one's.
fn row_id(ui: &Ui) -> Id {
    ui.scope_id().with("dynamic-row")
}

/// The left edge of the row this button sits in, as the last pass left it:
/// the pill grows from there, so it covers none of the icons.
fn row_left(ui: &Ui, rect: Rect) -> f32 {
    let pass = ui.ctx().cumulative_pass_nr();
    let id = row_id(ui);
    let (seen, now, last) = ui
        .ctx()
        .data(|d| d.get_temp::<(u64, f32, f32)>(id))
        .unwrap_or((pass, rect.min.x, rect.min.x));
    let (now, last) = if seen == pass {
        (now.min(rect.min.x), last)
    } else {
        (rect.min.x, now)
    };
    ui.ctx().data_mut(|d| d.insert_temp(id, (pass, now, last)));
    last.min(rect.min.x)
}

/// Whether another button showed its pill in the last pass: the pointer is
/// moving along the row.
fn switching(ui: &Ui, me: Id) -> bool {
    let pass = ui.ctx().cumulative_pass_nr();
    ui.ctx()
        .data(|d| d.get_temp::<(Id, u64)>(shown_id()))
        .is_some_and(|(id, seen)| id != me && seen + 1 >= pass)
}

/// How far the pill is out: over `motion-hint` going in, at once when another
/// button's pill was just showing, and at once going out.
fn pill_reveal(ui: &Ui, id: Id, on: bool, switching: bool) -> f32 {
    let secs = if on && !switching {
        theme::motion(ui.ctx(), theme::MOTION_HINT)
    } else {
        0.0
    };
    ui.ctx()
        .animate_bool_with_time(id.with("dynamic-pill"), on, secs)
}

impl Widget for DynamicButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let p = theme::palette(ui);
        let reveal = self.key.map_or(0.0, |_| key_hints(ui.ctx()));
        let (lit_fill, lit_ink) = self.lit(p);
        let key = self
            .key
            .filter(|_| reveal > 0.0)
            .map(|key| galley(ui, key, theme::meta_mono(), p.ink_muted));
        let key_room = key
            .as_ref()
            .map_or(0.0, |key| reveal * (KEY_GAP + key.size().x));
        let size = Vec2::new(theme::CONTROL_H + key_room, theme::CONTROL_H);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), self.label)
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let on = ui.is_enabled() && (response.hovered() || response.has_focus());
        let switching = switching(ui, response.id);
        if on {
            let pass = ui.ctx().cumulative_pass_nr();
            ui.ctx()
                .data_mut(|d| d.insert_temp(shown_id(), (response.id, pass)));
        }
        let pill = pill_reveal(ui, response.id, on, switching);
        let left = row_left(ui, rect);
        // Over the crumbs, under menus and modals.
        let layer = LayerId::new(Order::Middle, response.id.with("dynamic-layer"));
        let painter = ui.ctx().layer_painter(layer);
        let ink = if !ui.is_enabled() {
            p.ink_faint
        } else if on || self.open {
            lit_ink
        } else {
            p.ink_muted
        };
        if on || self.open {
            painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), lit_fill);
        }
        let label = on.then(|| {
            let mut font = theme::ui_strong();
            font.size = PILL_TEXT;
            let text = galley(ui, self.label, font, lit_ink);
            let right = left - PILL_GAP;
            let width = pill * (2.0 * PILL_PAD + text.size().x);
            let shape = Rect::from_x_y_ranges(right - width..=right, rect.y_range());
            painter.rect_filled(shape, CornerRadius::same(theme::RADIUS_SM), lit_fill);
            let at = Pos2::new(
                right - PILL_PAD - text.size().x,
                rect.center().y - text.size().y / 2.0,
            );
            painter
                .with_clip_rect(shape)
                .galley_with_override_text_color(at, text, lit_ink.gamma_multiply(pill));
            shape
        });
        paint_face(&painter, self.icon, rect, ink);
        if let Some(key) = key {
            let left = rect.min.x + ICON_PAD + theme::ICON_SIZE + KEY_GAP;
            let at = Pos2::new(left, rect.center().y - key.size().y / 2.0);
            painter.galley_with_override_text_color(at, key, ink.gamma_multiply(reveal));
        }
        if response.has_focus() {
            for shape in std::iter::once(rect).chain(label) {
                painter.rect_stroke(
                    shape.expand(2.0),
                    CornerRadius::same(theme::RADIUS_SM + 2),
                    Stroke::new(2.0, p.accent),
                    StrokeKind::Outside,
                );
            }
        }
        response
    }
}

fn paint_face(painter: &Painter, icon: Icon, rect: Rect, ink: Color32) {
    let at = Rect::from_min_size(
        Pos2::new(
            rect.min.x + ICON_PAD,
            rect.center().y - theme::ICON_SIZE / 2.0,
        ),
        Vec2::splat(theme::ICON_SIZE),
    );
    theme::paint_icon(painter, icon, at, ink);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[derive(Default)]
    struct Row {
        themed: bool,
        clicked: Vec<&'static str>,
    }

    fn row() -> Harness<'static, Row> {
        let mut harness = Harness::builder().with_step_dt(0.1).build_ui_state(
            |ui, row: &mut Row| {
                if !std::mem::replace(&mut row.themed, true) {
                    theme::install(ui.ctx());
                    theme::set_reduced_motion(ui.ctx(), true);
                    return;
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_1;
                    ui.add_space(200.0);
                    for (icon, label, key) in
                        [(Icon::Edit, "Edit", "E"), (Icon::TagPlus, "Add label", "L")]
                    {
                        let button = DynamicButton::new(icon, label).key(Some(key));
                        if ui.add(button).clicked() {
                            row.clicked.push(label);
                        }
                    }
                    let delete = DynamicButton::new(Icon::Trash, "Delete").danger();
                    if ui.add(delete).clicked() {
                        row.clicked.push("Delete");
                    }
                });
            },
            Row::default(),
        );
        harness.run_steps(3);
        harness
    }

    #[test]
    fn at_rest_each_takes_24pt_and_2pt_apart() {
        let harness = row();
        let edit = harness.get_by_label("Edit").rect();
        let add = harness.get_by_label("Add label").rect();
        assert_eq!(edit.width(), 24.0);
        assert_eq!(add.width(), 24.0);
        assert_eq!(add.min.x - edit.max.x, theme::SPACE_1);
    }

    /// The pill is paint only: nothing moves under the pointer, and the
    /// neighbour it covers still takes the pointer and the click.
    #[test]
    fn the_pill_moves_nothing_and_leaves_the_neighbours_clickable() {
        let mut harness = row();
        let rest = harness.get_by_label("Edit").rect();
        harness.get_by_label("Add label").hover();
        harness.run_steps(2);
        assert_eq!(harness.get_by_label("Edit").rect(), rest);
        assert_eq!(harness.get_by_label("Add label").rect().width(), 24.0);

        harness.get_by_label("Edit").click();
        harness.run_steps(2);
        assert_eq!(harness.state().clicked, ["Edit"]);
    }

    /// The pill grows left of the whole row, so every icon stays in sight.
    #[test]
    fn the_pill_sits_left_of_the_row_and_covers_no_icon() {
        let mut harness = row();
        harness.get_by_label("Delete").hover();
        harness.run_steps(3);
        let edit = harness.get_by_label("Edit").rect();
        let label = harness
            .output()
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.text() == "Delete" => {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
            .expect("the pill's label");
        assert!(label.max.x < edit.min.x, "{label:?} covers {edit:?}");
    }

    /// ⌘ brings each key out in layout; a button with no key stays 24pt.
    #[test]
    fn command_slides_the_key_out_in_layout() {
        let mut harness = row();
        harness.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
        harness.run_steps(7);
        assert!(harness.get_by_label("Edit").rect().width() > 24.0);
        assert_eq!(harness.get_by_label("Delete").rect().width(), 24.0);
        harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        harness.run_steps(1);
        assert_eq!(harness.get_by_label("Edit").rect().width(), 24.0);
    }

    /// `↩` on the focused button clicks it, like any other.
    #[test]
    fn the_keyboard_reaches_it() {
        let mut harness = row();
        harness.get_by_label("Delete").focus();
        harness.run_steps(2);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(harness.state().clicked, ["Delete"]);
    }
}
