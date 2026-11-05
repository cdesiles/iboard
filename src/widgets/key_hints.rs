//! Key hints show only while ⌘ is held. Once it has been held alone for half
//! a second, every key slides out beside its control; letting go of it, or
//! pressing anything with it, puts them away, so a shortcut never flashes them.

use std::time::Duration;

use egui::{Context, Event, Id, InputState, Modifiers, Pos2, Sense, Ui, Vec2};

use crate::theme;

/// How long ⌘ is held alone before the hints show: longer than a shortcut
/// takes, so pressing one never shows them.
pub const HOLD: f64 = 0.5;

/// How long the hints take to slide out or away.
const SLIDE: f32 = 0.12;

/// How far a row of hints travels as it slides out.
const SLIDE_BY: f32 = theme::SPACE_3;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hold {
    Up,
    /// ⌘ alone since this time.
    Since(f64),
    /// Something joined ⌘: no hints until it is let go.
    Spent,
}

/// ⌘ and nothing else. On macOS the Control key is a modifier of its own.
fn command_alone(modifiers: Modifiers) -> bool {
    modifiers.command_only() && !(modifiers.mac_cmd && modifiers.ctrl)
}

/// What `hold` becomes with this pass's input.
fn next(hold: Hold, input: &InputState) -> Hold {
    if !input.modifiers.command || !input.focused {
        return Hold::Up;
    }
    let pressed = input.events.iter().any(|event| {
        matches!(
            event,
            Event::Key { pressed: true, .. }
                | Event::PointerButton { pressed: true, .. }
                | Event::Text(_)
                | Event::Copy
                | Event::Cut
                | Event::Paste(_)
        )
    });
    match hold {
        _ if pressed || !command_alone(input.modifiers) => Hold::Spent,
        Hold::Up => Hold::Since(input.time),
        held => held,
    }
}

/// How far out the key hints are, from 0 (put away) to 1.
pub fn key_hints(ctx: &Context) -> f32 {
    let id = Id::new("key-hints");
    let pass = ctx.cumulative_pass_nr();
    let hold = match ctx.data(|d| d.get_temp::<(u64, Hold)>(id)) {
        Some((seen, hold)) if seen == pass => hold,
        stored => {
            let before = stored.map_or(Hold::Up, |(_, hold)| hold);
            let hold = ctx.input(|input| next(before, input));
            ctx.data_mut(|d| d.insert_temp(id, (pass, hold)));
            hold
        }
    };
    let now = ctx.input(|input| input.time);
    let shown = match hold {
        Hold::Since(since) if now - since >= HOLD => true,
        Hold::Since(since) => {
            ctx.request_repaint_after(Duration::from_secs_f64(HOLD - (now - since)));
            false
        }
        Hold::Up | Hold::Spent => false,
    };
    ctx.animate_bool_with_time(id.with("slide"), shown, theme::motion(ctx, SLIDE))
}

/// Whether the key hints are out, even partly.
pub fn key_hints_shown(ctx: &Context) -> bool {
    key_hints(ctx) > 0.0
}

/// Draws a row of hints with the slide: `add` runs only while they are out,
/// and its hints fade in as they travel to where they rest.
pub fn sliding_hints(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let reveal = key_hints(ui.ctx());
    if reveal <= 0.0 {
        return;
    }
    ui.scope(|ui| {
        ui.multiply_opacity(reveal);
        ui.add_space((1.0 - reveal) * SLIDE_BY);
        add(ui);
    });
}

/// A lone key cap in a right-to-left row, as the filter has: it takes its room
/// as it slides out, coming from behind its right edge.
pub fn sliding_kbd(ui: &mut Ui, key: &str) {
    let reveal = key_hints(ui.ctx());
    if reveal <= 0.0 {
        return;
    }
    let width = super::text_width(ui, key, theme::meta()) + 8.0;
    let (cap, _) = ui.allocate_exact_size(Vec2::new(width * reveal, 16.0), Sense::hover());
    ui.scope(|ui| {
        ui.multiply_opacity(reveal);
        ui.shrink_clip_rect(cap);
        super::paint_kbd(ui, key, Pos2::new(cap.min.x, cap.center().y));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A context whose hints slide in one pass, so each pass reads 0 or 1.
    fn ctx() -> Context {
        let ctx = Context::default();
        theme::set_reduced_motion(&ctx, true);
        ctx
    }

    /// One pass at `time`, with `events`; returns how far out the hints are.
    fn pass(ctx: &Context, time: f64, events: Vec<Event>) -> f32 {
        let input = egui::RawInput {
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut out = 0.0;
        ctx.run_ui(input, |ui| out = key_hints(ui.ctx()))
            .textures_delta
            .clear();
        out
    }

    fn modifiers(modifiers: Modifiers) -> Vec<Event> {
        vec![Event::ModifiersChanged(modifiers)]
    }

    fn key_n() -> Vec<Event> {
        vec![Event::Key {
            key: egui::Key::N,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]
    }

    #[test]
    fn the_keys_show_once_command_is_held_alone_for_a_moment() {
        let ctx = ctx();
        assert_eq!(pass(&ctx, 0.0, vec![]), 0.0, "hidden at rest");
        assert_eq!(pass(&ctx, 1.0, modifiers(Modifiers::COMMAND)), 0.0);
        assert_eq!(pass(&ctx, 1.4, vec![]), 0.0, "not before half a second");
        assert_eq!(pass(&ctx, 1.5, vec![]), 1.0);
        assert_eq!(pass(&ctx, 1.6, modifiers(Modifiers::NONE)), 0.0, "let go");
    }

    #[test]
    fn a_shortcut_never_shows_them() {
        let ctx = ctx();
        pass(&ctx, 0.0, modifiers(Modifiers::COMMAND));
        assert_eq!(pass(&ctx, 0.2, key_n()), 0.0);
        assert_eq!(pass(&ctx, 2.0, vec![]), 0.0, "⌘ still down after ⌘N");
        pass(&ctx, 2.1, modifiers(Modifiers::NONE));
        pass(&ctx, 2.2, modifiers(Modifiers::COMMAND));
        assert_eq!(pass(&ctx, 2.7, vec![]), 1.0, "a fresh hold shows them");
    }

    #[test]
    fn another_modifier_or_leaving_the_window_puts_them_away() {
        let ctx = ctx();
        pass(&ctx, 0.0, modifiers(Modifiers::COMMAND));
        assert_eq!(pass(&ctx, 1.0, vec![]), 1.0);
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        assert_eq!(pass(&ctx, 1.1, modifiers(shift)), 0.0, "⌘⇧ is a shortcut");
        assert_eq!(pass(&ctx, 2.0, modifiers(Modifiers::COMMAND)), 0.0);

        pass(&ctx, 3.0, modifiers(Modifiers::NONE));
        pass(&ctx, 3.1, modifiers(Modifiers::COMMAND));
        assert_eq!(pass(&ctx, 4.0, vec![]), 1.0);
        let away = egui::RawInput {
            time: Some(4.1),
            focused: false,
            events: vec![Event::WindowFocused(false)],
            ..Default::default()
        };
        let mut out = 1.0;
        ctx.run_ui(away, |ui| out = key_hints(ui.ctx()))
            .textures_delta
            .clear();
        assert_eq!(out, 0.0, "⌘⇥ to another app");
    }

    #[test]
    fn a_row_of_hints_fades_and_travels_as_it_slides_out() {
        let ctx = Context::default();
        let step = |time: f64, events: Vec<Event>| {
            let input = egui::RawInput {
                time: Some(time),
                events,
                ..Default::default()
            };
            let mut seen = None;
            ctx.run_ui(input, |ui| {
                ui.horizontal(|ui| {
                    let start = ui.next_widget_position().x;
                    sliding_hints(ui, |ui| {
                        seen = Some((ui.opacity(), ui.next_widget_position().x - start));
                    });
                });
            })
            .textures_delta
            .clear();
            seen
        };
        assert_eq!(step(0.0, modifiers(Modifiers::COMMAND)), None);
        assert_eq!(step(HOLD - 0.01, vec![]), None);
        let (opacity, offset) = step(HOLD, vec![]).expect("past the hold");
        assert!(opacity > 0.0 && opacity < 1.0, "{opacity}");
        assert!(offset.abs() > 0.0, "{offset}");
        let mut last = None;
        for frame in 1..30 {
            last = step(HOLD + f64::from(frame) / 60.0, vec![]);
        }
        assert_eq!(
            last.map(|(opacity, _)| opacity),
            Some(1.0),
            "at rest once out"
        );
    }

    #[test]
    fn holding_command_wakes_the_frame_that_shows_them() {
        let ctx = ctx();
        let input = egui::RawInput {
            time: Some(0.0),
            events: modifiers(Modifiers::COMMAND),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            key_hints(ui.ctx());
        });
        output.textures_delta.clear();
        let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        assert!(delay <= Duration::from_secs_f64(HOLD), "{delay:?}");
    }
}
