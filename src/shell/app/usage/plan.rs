//! A subscription's plan window at the top of its pane: the popover's ring,
//! gauge and pace, at full width and outside the range.

use chrono::{DateTime, Utc};

use super::super::status;
use super::super::usage_popover::{used_line_of, verdict_of};
use super::blocks;
use super::{Align, Layout, RichText, theme, widgets};
use crate::domains::insights::usage::PlanWindow;

/// The plan window's panel, when the provider reported one.
pub(super) fn plan_panel(ui: &mut egui::Ui, plan: &PlanWindow, now: DateTime<Utc>) {
    blocks::card(ui, |ui| {
        head(ui, plan);
        body(ui, plan, now);
    });
}

fn head(ui: &mut egui::Ui, plan: &PlanWindow) {
    let p = theme::palette(ui);
    let title = match plan.provider.as_str() {
        "copilot" => "Premium credits".to_string(),
        _ => "Plan window".to_string(),
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        ui.label(RichText::new(title).font(theme::ui_title()).color(p.ink));
        ui.label(
            RichText::new(format!(
                "This {}, whatever the range",
                plan.window.name().to_lowercase()
            ))
            .font(theme::meta())
            .color(p.ink_muted),
        );
    });
}

fn body(ui: &mut egui::Ui, plan: &PlanWindow, now: DateTime<Utc>) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        widgets::ring(ui, plan.share, plan.over_pace);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(used_line_of(
                        &plan.name,
                        &plan.plan,
                        &plan.consumed,
                        plan.reported.as_ref(),
                    ))
                    .font(theme::ui_strong())
                    .color(p.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (text, color) = match plan.share {
                        Some(share) => (format!("{:.0}%", share * 100.0), p.ink),
                        None => ("No cap".to_string(), p.ink_muted),
                    };
                    ui.label(RichText::new(text).font(theme::ui_title()).color(color));
                });
            });
            widgets::gauge(
                ui,
                plan.share,
                plan.pace,
                widgets::GAUGE_H,
                &format!("the {}", plan.window.name().to_lowercase()),
            );
            ui.horizontal(|ui| {
                verdict(ui, plan, now);
                if plan.share.is_some() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(status::reset_of(plan.reset, now))
                                .font(theme::meta())
                                .color(p.ink_muted),
                        );
                    });
                }
            });
        });
    });
}

/// `Under pace`, `Over pace · cap by 1 Dec`, or the reset when no cap is known.
fn verdict(ui: &mut egui::Ui, plan: &PlanWindow, now: DateTime<Utc>) {
    let p = theme::palette(ui);
    let (text, color) = match plan.share {
        Some(_) => {
            let text = verdict_of(plan.over_pace, plan.cap_reached_on.as_deref());
            let color = if plan.over_pace {
                p.status_doing
            } else {
                p.ink_muted
            };
            (text, color)
        }
        None => (
            capitalized_reset(&status::reset_of(plan.reset, now)),
            p.ink_muted,
        ),
    };
    ui.label(RichText::new(text).font(theme::meta()).color(color));
}

fn capitalized_reset(text: &str) -> String {
    match text.strip_prefix("resets ") {
        Some(rest) => format!("Resets {rest}"),
        None => text.to_string(),
    }
}
