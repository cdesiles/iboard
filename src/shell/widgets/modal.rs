//! A Modal's life across frames: it fades and rises in, hands focus to its
//! first field and back to what held it, says what it is to VoiceOver, and
//! fades out from what it last painted, since its caller drops its state on
//! closing. See `design/components/Modal`.

use std::sync::Arc;

use egui::accesskit::Role;
use egui::epaint::ClippedShape;
use egui::{Context, FocusDirection, Frame, Id, LayerId, ModalResponse, Order, Ui};

use super::dialog::modal_frame;
use crate::shell::theme::{self, ModalSize};

const SCRIM_IN: f32 = 0.12;
const FRAME_IN: f32 = 0.16;
const FADE_OUT: f32 = 0.10;
const RISE: f32 = 8.0;

/// The height a Modal M's or L's body scrolls past, in `window`.
pub fn modal_body_max(window: egui::Rect) -> f32 {
    520.0_f32.min(theme::modal_size(ModalSize::M, window).y - 2.0 * theme::BAR_H)
}

#[derive(Clone)]
struct Open {
    layer: LayerId,
    opened_at: f64,
    opened_pass: u64,
    shown_pass: u64,
    /// What held the focus as it opened.
    opener: Option<Id>,
    wants_focus: bool,
    painted: Arc<Vec<ClippedShape>>,
}

#[derive(Clone)]
struct Closing {
    since: f64,
    painted: Arc<Vec<ClippedShape>>,
}

#[derive(Clone, Default)]
struct Modals {
    open: Vec<Open>,
    closing: Vec<Closing>,
}

fn with_modals<R>(ctx: &Context, f: impl FnOnce(&mut Modals) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default(Id::unique("modals"))))
}

/// How far along an animation of `secs` is at `age`, always done under
/// reduced motion.
fn progress(ctx: &Context, age: f32, secs: f32) -> f32 {
    match theme::motion(ctx, secs) {
        0.0 => 1.0,
        secs => (age / secs).clamp(0.0, 1.0),
    }
}

/// How long the modal on `layer` has been up, marking it shown this pass.
fn age_of(ctx: &Context, layer: LayerId) -> f32 {
    let now = ctx.input(|i| i.time);
    let pass = ctx.cumulative_pass_nr();
    let opener = ctx.memory(|m| m.focused());
    with_modals(ctx, |modals| {
        let at = modals.open.iter().position(|open| open.layer == layer);
        let at = at.unwrap_or_else(|| {
            modals.open.push(Open {
                layer,
                opened_at: now,
                opened_pass: pass,
                shown_pass: pass,
                opener,
                wants_focus: true,
                painted: Arc::default(),
            });
            modals.open.len() - 1
        });
        modals.open[at].shown_pass = pass;
        (now - modals.open[at].opened_at) as f32
    })
}

/// A Modal being built: [`egui::Modal`] with the spec's motion and role.
pub struct Modal {
    inner: egui::Modal,
    frame: Frame,
    fade: f32,
    role: Role,
    title: String,
}

/// A Modal of `size` called `title`: placed for its size, over the `scrim`,
/// in [`modal_frame`] with no margin of its own.
pub fn modal(ui: &Ui, id: &str, size: ModalSize, title: &str) -> Modal {
    let ctx = ui.ctx();
    let id = Id::unique(id);
    let window = ctx.content_rect();
    let age = age_of(ctx, LayerId::new(Order::Foreground, id));
    let scrim = progress(ctx, age, SCRIM_IN);
    let fade = progress(ctx, age, FRAME_IN);
    if fade < 1.0 {
        ctx.request_repaint();
    }
    let rise = RISE * (1.0 - egui::emath::easing::cubic_out(fade));
    let area = theme::modal_area(id, size, window, rise).fade_in(false);
    let role = match size {
        ModalSize::S => Role::AlertDialog,
        _ => Role::Dialog,
    };
    Modal {
        inner: egui::Modal::new(id)
            .area(area)
            .backdrop_color(theme::palette(ui).scrim.gamma_multiply(scrim)),
        frame: modal_frame(ui),
        fade,
        role,
        title: title.to_string(),
    }
}

impl Modal {
    /// Its frame, in place of a bare [`modal_frame`].
    pub fn frame(mut self, frame: Frame) -> Self {
        self.frame = frame;
        self
    }

    pub fn show<T>(self, ctx: &Context, content: impl FnOnce(&mut Ui) -> T) -> ModalResponse<T> {
        let Self {
            inner,
            frame,
            fade,
            role,
            title,
        } = self;
        inner
            .frame(frame.multiply_with_opacity(fade))
            .show(ctx, |ui| {
                ui.multiply_opacity(fade);
                ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                    node.set_role(role);
                    node.set_label(title);
                    node.set_modal();
                });
                content(ui)
            })
    }
}

/// Call just before a modal's first field: the next focusable widget takes
/// the focus, once, as the modal opens. Widgets take no focus in a sizing
/// pass or under the modal still on top, so the claim waits for those.
pub fn focus_here(ui: &Ui) {
    let layer = ui.layer_id();
    let ctx = ui.ctx();
    if ui.is_sizing_pass() || !ctx.memory(|m| m.allows_interaction(layer)) {
        return;
    }
    let claimed = with_modals(ctx, |modals| {
        modals
            .open
            .iter_mut()
            .find(|open| open.layer == layer)
            .is_some_and(|open| std::mem::take(&mut open.wants_focus))
    });
    if claimed {
        ctx.memory_mut(|m| {
            if let Some(focused) = m.focused() {
                m.surrender_focus(focused);
            }
            m.move_focus(FocusDirection::Next);
        });
    }
}

/// Call once a frame after every modal drew: what no longer showed has
/// closed, so the focus goes back and its last paint fades out.
pub fn settle_modals(ctx: &Context) {
    let now = ctx.input(|i| i.time);
    let pass = ctx.cumulative_pass_nr();
    let fades = theme::motion(ctx, FADE_OUT) > 0.0;
    let mut modals = with_modals(ctx, std::mem::take);
    let (mut open, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut modals.open)
        .into_iter()
        .partition(|open| open.shown_pass == pass);
    for closed in gone {
        match open.last_mut().filter(|top| top.opened_pass == pass) {
            Some(replacing) => replacing.opener = closed.opener.or(replacing.opener),
            None => {
                if let Some(opener) = closed.opener {
                    ctx.memory_mut(|m| m.request_focus(opener));
                }
            }
        }
        if fades {
            modals.closing.push(Closing {
                since: now,
                painted: closed.painted,
            });
        }
    }
    for shown in &mut open {
        shown.painted = ctx.graphics(|layers| {
            let list = layers.get(shown.layer);
            Arc::new(list.map_or_else(Vec::new, |l| l.all_entries().cloned().collect()))
        });
    }
    modals.open = open;
    modals
        .closing
        .retain(|closing| paint_closing(ctx, closing, now));
    with_modals(ctx, |slot| *slot = modals);
}

/// One frame of a closed modal's fade; false once it is over.
fn paint_closing(ctx: &Context, closing: &Closing, now: f64) -> bool {
    let fade = progress(ctx, (now - closing.since) as f32, FADE_OUT);
    if fade >= 1.0 {
        return false;
    }
    let layer = LayerId::new(Order::Foreground, Id::unique("modal-closing"));
    let mut painter = ctx.layer_painter(layer);
    painter.multiply_opacity(1.0 - fade);
    for ClippedShape {
        clip_rect, shape, ..
    } in closing.painted.iter()
    {
        painter.with_clip_rect(*clip_rect).add(shape.clone());
    }
    ctx.request_repaint();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pass, the modal up or not: its frame's top, and how many shapes
    /// the fade-out paints.
    fn pass(ctx: &Context, up: bool) -> (Option<f32>, usize) {
        let mut seen = (None, 0);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            if up {
                let ctx = ui.ctx().clone();
                modal(ui, "m", ModalSize::M, "M").show(&ctx, |ui| {
                    ui.set_min_size(egui::vec2(200.0, 100.0));
                    ui.label("Body");
                });
                let layer = LayerId::new(Order::Foreground, Id::unique("m"));
                seen.0 = ctx.memory(|m| m.area_rect(layer.id)).map(|rect| rect.top());
            }
            settle_modals(ui.ctx());
            let closing = LayerId::new(Order::Foreground, Id::unique("modal-closing"));
            seen.1 = ui
                .ctx()
                .graphics(|layers| layers.get(closing).map_or(0, |l| l.all_entries().len()));
        })
        .textures_delta
        .clear();
        seen
    }

    fn themed(reduce: bool) -> Context {
        let ctx = Context::default();
        theme::install(&ctx);
        theme::set_reduced_motion(&ctx, reduce);
        ctx
    }

    #[test]
    fn a_modal_rises_into_place_and_fades_out_from_its_last_paint() {
        let ctx = themed(false);
        pass(&ctx, true);
        let early = pass(&ctx, true).0.unwrap();
        for _ in 0..20 {
            pass(&ctx, true);
        }
        let settled = pass(&ctx, true).0.unwrap();
        assert!(early > settled, "rising: {early} to {settled}");
        assert!(early - settled <= RISE);
        assert!(pass(&ctx, false).1 > 0, "its last paint fades out");
        for _ in 0..10 {
            pass(&ctx, false);
        }
        assert_eq!(pass(&ctx, false).1, 0, "and is gone");
    }

    #[test]
    fn reduced_motion_opens_and_closes_in_one_frame() {
        let ctx = themed(true);
        pass(&ctx, true);
        let first = pass(&ctx, true).0.unwrap();
        for _ in 0..20 {
            pass(&ctx, true);
        }
        assert_eq!(pass(&ctx, true).0.unwrap(), first, "no rise");
        assert_eq!(pass(&ctx, false).1, 0, "no fade-out");
        assert_eq!(progress(&ctx, 0.0, FRAME_IN), 1.0);
    }
}
