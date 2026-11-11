//! The Refactor watch's painting: its verdict, the signals behind it and the
//! draft it offers; its rules are in `src/stats/watch.rs`.

use crate::stats::{self, Limits, Signal, Stats};

use super::blocks::{Blocks, Part, clock_note, framed, list_rows, panel_head};
use super::{
    ButtonKind, CornerRadius, Duration, Frame, Icon, Rect, RichText, Sense, StatsAsk, TRENDS_H,
    Vec2, theme, widgets,
};

const NAME: &str = "Refactor watch";
/// Above and below the hairline between two signals.
const ROW_PAD: f32 = 10.0;
/// A signal's icon column; its text starts past it, its detail included.
const ICON_COLUMN: Vec2 = Vec2::new(16.0, 18.0);

/// The Refactor watch block, on its own job.
pub(super) fn watch(ui: &mut egui::Ui, blocks: &mut Blocks, limits: &Limits) -> Option<StatsAsk> {
    framed(
        ui,
        blocks,
        (Part::Watch, NAME, TRENDS_H),
        list_rows,
        |ui, stats, clock| verdict(ui, stats, limits, clock),
    )
    .flatten()
}

/// The verdict, the four signals behind it, and the draft it offers.
fn verdict(
    ui: &mut egui::Ui,
    stats: &Stats,
    limits: &Limits,
    clock: Option<Duration>,
) -> Option<StatsAsk> {
    let signals = stats::signals(stats, limits);
    let on = signals.iter().filter(|s| s.on).count();
    let p = theme::palette(ui);
    panel_head(ui, NAME, "", |ui| clock_note(ui, clock));
    let mut ask = None;
    Frame::new()
        .fill(p.surface_sunken)
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(theme::SPACE_3)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !stats::refactor_due(&signals) {
                ui.label(
                    RichText::new("No refactor needed now")
                        .font(theme::ui_strong())
                        .color(p.ink),
                );
                return;
            }
            ui.label(
                RichText::new("Time to plan a refactor.")
                    .font(theme::ui_strong())
                    .color(p.ink),
            );
            let files = stats::culprits(&stats.files).len();
            ui.label(
                RichText::new(format!(
                    "{on} of {} signals are on, mostly from {files} files.",
                    signals.len()
                ))
                .font(theme::ui_small())
                .color(p.ink),
            );
            if widgets::button(
                ui,
                ButtonKind::Primary,
                None,
                "Draft a refactor issue",
                None,
            )
            .clicked()
            {
                let (title, body) = stats::refactor_draft(stats, &signals);
                ask = Some(StatsAsk::DraftRefactor(title, body));
            }
        });
    ui.add_space(theme::SPACE_2);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (at, signal) in signals.iter().enumerate() {
            if at > 0 {
                ui.add_space(ROW_PAD);
                widgets::rule(ui);
                ui.add_space(ROW_PAD);
            }
            signal_row(ui, signal);
        }
    });
    ask
}

fn signal_row(ui: &mut egui::Ui, signal: &Signal) {
    let p = theme::palette(ui);
    let (icon, color) = if signal.on {
        (Icon::Warning, p.status_doing)
    } else {
        (Icon::Check, p.status_done)
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let (column, _) = ui.allocate_exact_size(ICON_COLUMN, Sense::hover());
        let mark = Rect::from_min_size(
            column.min + Vec2::new(0.0, theme::SPACE_1),
            Vec2::splat(theme::ICON_SIZE),
        );
        theme::paint_icon(ui.painter(), icon, mark, color);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            ui.add(
                egui::Label::new(
                    RichText::new(&signal.headline)
                        .font(theme::ui_body())
                        .color(p.ink),
                )
                .wrap(),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(&signal.detail)
                        .font(theme::meta())
                        .color(p.ink_muted),
                )
                .wrap(),
            );
        });
    });
}
