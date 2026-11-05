//! The Refactor watch's painting: its verdict, the signals behind it and the
//! draft it offers; its rules are in `src/stats/watch.rs`.

use iboard::stats::{self, Limits, Signal, Stats};

use super::blocks::{Blocks, Part, block_head, framed, list_rows};
use super::{
    ButtonKind, CornerRadius, Duration, Frame, Icon, RichText, Sense, StatsAsk, TRENDS_H, Vec2,
    theme, widgets,
};

const NAME: &str = "Refactor watch";

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
    block_head(ui, NAME, clock);
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
    for signal in &signals {
        signal_row(ui, signal);
    }
    ask
}

fn signal_row(ui: &mut egui::Ui, signal: &Signal) {
    let p = theme::palette(ui);
    let (icon, word, color) = if signal.on {
        (Icon::Warning, "On", p.status_doing)
    } else {
        (Icon::Check, "Off", p.status_done)
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(theme::ICON_SIZE), Sense::hover());
        theme::paint_icon(ui.painter(), icon, rect, color);
        ui.label(RichText::new(word).font(theme::meta()).color(color));
        ui.add(
            egui::Label::new(
                RichText::new(signal.name)
                    .font(theme::ui_small())
                    .color(p.ink),
            )
            .wrap(),
        );
    });
    ui.add(
        egui::Label::new(
            RichText::new(&signal.detail)
                .font(theme::meta())
                .color(p.ink_muted),
        )
        .wrap(),
    );
}
