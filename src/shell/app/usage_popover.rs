//! The usage popover: the board's spend and the quick answer per plan window,
//! will it make the reset, then the links to the full Usage screen and
//! Settings › AI.

use chrono::{DateTime, Utc};

use super::status::BoardSpend;
use super::{
    Align, CornerRadius, Frame, IboardApp, Layout, MarginF32, Rect, RichText, Sense, Vec2,
    settings_ui, theme, widgets,
};
use crate::agent::{Cap, Measure, Plan, Quota};
use crate::domains::agent::artifacts::usage_log::{Consumption, Meter};
use crate::format::thousands;

/// The popover's width.
const WIDTH: f32 = 380.0;

/// What the foot asked for, closed with the popover.
enum Foot {
    Settings,
    Usage,
}

/// The one usage popover, shared by every frame.
pub(super) fn popup_id() -> egui::Id {
    egui::Id::unique("subscription-usage")
}

impl IboardApp {
    /// The popover above the usage trigger: the board's spend, then a row per
    /// window, then the links to Full usage and Settings › AI. It closes on
    /// `esc` or a click outside, as egui's popup does.
    pub(super) fn usage_popover(
        &mut self,
        ui: &egui::Ui,
        anchor: Rect,
        meters: &[Meter],
        spend: Option<&BoardSpend>,
        now: DateTime<Utc>,
    ) {
        let id = popup_id();
        if !egui::Popup::is_id_open(ui.ctx(), id) {
            return;
        }
        let mut foot = None;
        egui::Popup::new(id, ui.ctx().clone(), anchor, ui.layer_id())
            .open_memory(None)
            .width(WIDTH)
            .gap(theme::SPACE_2)
            .align(egui::RectAlign::TOP_END)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .frame(widgets::popover_frame(ui).inner_margin(0.0))
            .accessible_name("Usage")
            .show(|ui| foot = body(ui, meters, spend, now));
        if let Some(foot) = foot {
            egui::Popup::close_id(ui.ctx(), id);
            match foot {
                Foot::Settings => self.open_settings(settings_ui::Tab::Review),
                Foot::Usage => self.open_usage(),
            }
        }
    }
}

/// The popover's content: its head, the spend row, one plain row per window,
/// then its foot.
fn body(
    ui: &mut egui::Ui,
    meters: &[Meter],
    spend: Option<&BoardSpend>,
    now: DateTime<Utc>,
) -> Option<Foot> {
    let p = theme::palette(ui);
    ui.style_mut().interaction.selectable_labels = false;
    ui.set_width(WIDTH);
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    ui.allocate_ui_with_layout(
        Vec2::new(WIDTH, theme::BAR_H - 1.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add_space(theme::SPACE_4);
            ui.label(RichText::new("Usage").font(theme::ui_title()).color(p.ink));
        },
    );
    widgets::rule(ui);
    if let Some(spend) = spend {
        spend_row(ui, spend);
        if !meters.is_empty() {
            soft_rule(ui);
        }
    }
    for (at, meter) in meters.iter().enumerate() {
        if at > 0 {
            soft_rule(ui);
        }
        let multi = meters
            .iter()
            .filter(|other| other.provider == meter.provider)
            .count()
            > 1;
        row(ui, meter, now, multi);
    }
    widgets::rule(ui);
    foot(ui)
}

/// The board's spend, above the windows: `Today` or `Building`, today's
/// dollars at the right, then the week and the last session under it.
fn spend_row(ui: &mut egui::Ui, spend: &BoardSpend) {
    let p = theme::palette(ui);
    Frame::new()
        .inner_margin(MarginF32 {
            left: theme::SPACE_4,
            right: theme::SPACE_4,
            top: theme::SPACE_4,
            bottom: theme::SPACE_4,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            ui.horizontal(|ui| {
                let name = if spend.running { "Building" } else { "Today" };
                ui.label(RichText::new(name).font(theme::ui_strong()).color(p.ink));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let color = if spend.today > 0.0 {
                        p.ink
                    } else {
                        p.ink_muted
                    };
                    ui.label(
                        RichText::new(spend.figure())
                            .font(theme::ui_title())
                            .color(color),
                    );
                });
            });
            for (name, cost) in &spend.providers {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(name).font(theme::meta()).color(p.ink_muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let cost = format!("${cost:.2}");
                        ui.label(RichText::new(cost).font(theme::meta()).color(p.ink_muted));
                    });
                });
            }
            ui.horizontal(|ui| {
                let week = format!("This week · ${:.2}", spend.week);
                ui.label(RichText::new(week).font(theme::meta()).color(p.ink_muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(last) = spend.last {
                        let last = format!("Last session · ${last:.2}");
                        ui.label(RichText::new(last).font(theme::meta()).color(p.ink_muted));
                    }
                });
            });
        });
}

/// What a row is called: its provider, and the window when it has more than
/// one.
fn title(meter: &Meter, multi: bool) -> String {
    if multi {
        format!("{} · {}", meter.name, meter.window.name())
    } else {
        meter.name.clone()
    }
}

/// One window: its ring and name, its share, gauge and pace tick, then its
/// verdict and reset. A row is not a control: the foot's two links are.
fn row(ui: &mut egui::Ui, meter: &Meter, now: DateTime<Utc>, multi: bool) {
    Frame::new()
        .inner_margin(MarginF32 {
            left: theme::SPACE_4,
            right: theme::SPACE_4,
            top: theme::SPACE_4,
            bottom: theme::SPACE_4,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                widgets::ring(ui, meter.share(), meter.over_pace());
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                    name_and_share(ui, meter, multi);
                    widgets::gauge(
                        ui,
                        meter.share(),
                        meter.elapsed(),
                        widgets::GAUGE_H,
                        &window(meter),
                    );
                    verdict_line(ui, meter, now);
                });
            });
        });
}

/// The window's name, and its share at the right, or `No cap`.
fn name_and_share(ui: &mut egui::Ui, meter: &Meter, multi: bool) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title(meter, multi))
                .font(theme::ui_strong())
                .color(p.ink),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (text, color) = match meter.share() {
                Some(share) => (format!("{:.0}%", share * 100.0), p.ink),
                None => ("No cap".to_string(), p.ink_muted),
            };
            ui.label(RichText::new(text).font(theme::ui_title()).color(color));
        });
    });
}

/// The verdict on the left, and the reset on the right. With no cap there is
/// no pace, so the reset is the verdict.
fn verdict_line(ui: &mut egui::Ui, meter: &Meter, now: DateTime<Utc>) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        let (text, color) = match meter.share() {
            Some(_) => {
                let over = meter.over_pace();
                (
                    verdict(meter),
                    if over { p.status_doing } else { p.ink_muted },
                )
            }
            None => (capitalized_reset(meter, now), p.ink_muted),
        };
        ui.label(RichText::new(text).font(theme::meta()).color(color));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if meter.share().is_some() {
                ui.label(
                    RichText::new(super::status::reset_text(meter, now))
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
            }
        });
    });
}

/// `Under pace`, or where the cap is reached at the window's average rate.
fn verdict(meter: &Meter) -> String {
    verdict_of(meter.over_pace(), meter.cap_reached_on().as_deref())
}

/// [`verdict`] from a plan window's own pace and projected day.
pub(super) fn verdict_of(over_pace: bool, cap_reached_on: Option<&str>) -> String {
    match cap_reached_on {
        Some(day) => format!("Over pace · cap by {day}"),
        None if over_pace => "Over pace".to_string(),
        None => "Under pace".to_string(),
    }
}

/// `resets in 3d` as the verdict a row with no cap shows.
fn capitalized_reset(meter: &Meter, now: DateTime<Utc>) -> String {
    let text = super::status::reset_text(meter, now);
    match text.strip_prefix("resets ") {
        Some(rest) => format!("Resets {rest}"),
        None => text,
    }
}

/// The `the week` a gauge's label names.
fn window(meter: &Meter) -> String {
    format!("the {}", meter.window.name().to_lowercase())
}

/// `9,212 of 30,000 AI credits · from GitHub`, `3.1M of 5M tokens`,
/// `$6.20 of $10.00`, or `3.1M tokens burnt`.
pub(super) fn used_line_of(
    name: &str,
    plan: &Plan,
    consumed: &Consumption,
    reported: Option<&Quota>,
) -> String {
    if let Some(quota) = reported {
        return match quota.measure {
            Measure::Share(share) => {
                format!("{:.0}% used · from {}", share * 100.0, name)
            }
            Measure::Count { used, cap, unit } => format!(
                "{} of {} {unit} · from GitHub",
                thousands(used as usize),
                thousands(cap as usize)
            ),
        };
    }
    let tokens = thousands(consumed.tokens as usize);
    match plan.cap {
        Some(Cap::Tokens(cap)) => format!("{tokens} of {} tokens", thousands(cap as usize)),
        Some(Cap::Money(cap)) => format!("${:.2} of ${cap:.2}", consumed.dollars(plan)),
        None => format!("{tokens} tokens burnt"),
    }
}

/// The foot: Full usage and Settings › AI at the right.
fn foot(ui: &mut egui::Ui) -> Option<Foot> {
    let mut foot = None;
    Frame::new()
        .inner_margin(MarginF32 {
            left: theme::SPACE_4,
            right: theme::SPACE_4,
            top: theme::SPACE_3,
            bottom: theme::SPACE_3,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let links = widgets::text_width(ui, "Full usage", theme::meta())
                    + theme::SPACE_4
                    + widgets::text_width(ui, "Settings › AI", theme::meta());
                ui.add_space((ui.available_width() - links).max(0.0));
                let usage = RichText::new("Full usage").font(theme::meta());
                if ui.link(usage).clicked() {
                    foot = Some(Foot::Usage);
                }
                ui.add_space(theme::SPACE_4);
                let settings = RichText::new("Settings › AI").font(theme::meta());
                if ui.link(settings).clicked() {
                    foot = Some(Foot::Settings);
                }
            });
        });
    foot
}

/// The hairline between two rows: `line-soft`, never a card's edge.
fn soft_rule(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, p.line_soft);
}
