//! The RefinePanel while the challenger works (see
//! `design/components/Thinking`): the pick pane's status line and placeholder
//! choices, and bars where the reasoning will land.

use super::{CornerRadius, Frame, MEASURE, Margin, RichText, Stroke, Turn, theme, widgets};

/// Past this many seconds the waiting line says it is slow.
const SLOW_SECS: u64 = 20;

/// The status line while the challenger works: the mark, what it waits for
/// and the whole seconds spent; past 20s, that it is slow and how to leave.
fn thinking_line(ui: &mut egui::Ui, elapsed: std::time::Duration, first: bool) {
    let p = theme::palette(ui);
    let slow = elapsed.as_secs() >= SLOW_SECS;
    let line = match (slow, first) {
        (true, _) => "Still working",
        (false, true) => "Challenger is reading the issue",
        (false, false) => "Challenger is weighing your answer",
    };
    widgets::announce_thinking(ui, line, elapsed);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        widgets::thinking(ui, 28.0);
        ui.label(RichText::new(line).font(theme::ui_button()).color(p.ink));
        ui.label(
            RichText::new(crate::format::clock(elapsed))
                .font(theme::meta())
                .color(p.ink_muted),
        );
    });
    if slow {
        widgets::help(
            ui,
            "Longer issues can take up to a minute. esc cancels and keeps your answer.",
            p.ink_muted,
        );
    }
}

/// The pick pane while the challenger works: the status line, four
/// placeholder choices and the note, not yet open.
pub(super) fn thinking_pick(ui: &mut egui::Ui, elapsed: std::time::Duration, turns: &[Turn]) {
    let p = theme::palette(ui);
    let first = !turns.iter().any(|turn| matches!(turn, Turn::Author { .. }));
    thinking_line(ui, elapsed, first);
    for key in 1..=4 {
        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_soft))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    ui.label(
                        RichText::new(key.to_string())
                            .font(theme::meta_mono())
                            .color(p.ink_faint),
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 7.0;
                        widgets::skeleton_bar(ui, 0.55, 10.0);
                        widgets::skeleton_bar(ui, 0.85, 8.0);
                    });
                });
            });
    }
    ui.add_space(theme::SPACE_2);
    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
    let label = ui.label(
        RichText::new("Your answer")
            .font(theme::ui_small())
            .color(p.ink_muted),
    );
    ui.add_enabled(
        false,
        egui::TextEdit::multiline(&mut "")
            .hint_text("Available once the Challenger answers")
            .font(theme::ui_body())
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    )
    .labelled_by(label.id);
}

/// The why pane while the challenger works: bars where the TL;DR and two
/// paragraphs will land, so nothing jumps when they do.
pub(super) fn thinking_why(ui: &mut egui::Ui) {
    ui.scope(|ui| {
        ui.set_max_width(MEASURE);
        ui.spacing_mut().item_spacing.y = 9.0;
        widgets::skeleton_bar(ui, 0.96, 12.0);
        widgets::skeleton_bar(ui, 0.70, 12.0);
        ui.add_space(10.0);
        for widths in [&[0.98, 0.94, 0.97, 0.60][..], &[0.95, 0.99, 0.42]] {
            for &width in widths {
                widgets::skeleton_bar(ui, width, 8.0);
            }
            ui.add_space(10.0);
        }
    });
}
