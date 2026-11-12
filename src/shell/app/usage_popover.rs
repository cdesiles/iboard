//! The subscription popover: one collapsible row per plan, opening to the
//! window's days and models, then the link to Settings › AI.

use chrono::{DateTime, Utc};
use std::collections::HashSet;

use super::{
    Align, CornerRadius, Frame, IboardApp, Layout, MarginF32, Pos2, Rect, RichText, Sense, Shape,
    Stroke, Vec2, settings_ui, theme, widgets,
};
use crate::agent::Cap;
use crate::domains::agent::artifacts::usage_log::{DayUse, Meter, ModelUse};
use crate::format::thousands;

/// The popover's width.
const WIDTH: f32 = 380.0;

/// The daily chart's well and its label row.
const DAY_WELL_H: f32 = 40.0;
const DAY_LABEL_H: f32 = 14.0;

/// How wide a model's gauge sits.
const MODEL_GAUGE_W: f32 = 64.0;

/// Where an open row's detail starts, indented under the name.
const DETAIL_INDENT: f32 = theme::SPACE_4 + widgets::RING + theme::SPACE_3;

/// The one usage popover, shared by every frame.
pub(super) fn popup_id() -> egui::Id {
    egui::Id::unique("subscription-usage")
}

/// The rows that are open, kept while the popover shows.
fn rows_id() -> egui::Id {
    popup_id().with("open-rows")
}

impl IboardApp {
    /// The popover above the usage pill: a row per plan, then Settings › AI.
    /// It closes on `esc` or a click outside, as egui's popup does.
    pub(super) fn usage_popover(
        &mut self,
        ui: &egui::Ui,
        anchor: Rect,
        meters: &[Meter],
        now: DateTime<Utc>,
    ) {
        let id = popup_id();
        if !egui::Popup::is_id_open(ui.ctx(), id) {
            ui.data_mut(|data| data.remove::<HashSet<String>>(rows_id()));
            return;
        }
        let mut settings = false;
        egui::Popup::new(id, ui.ctx().clone(), anchor, ui.layer_id())
            .open_memory(None)
            .width(WIDTH)
            .gap(theme::SPACE_2)
            .align(egui::RectAlign::TOP_END)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .frame(widgets::popover_frame(ui).inner_margin(0.0))
            .accessible_name("Subscription usage")
            .show(|ui| settings = body(ui, meters, now));
        if settings {
            egui::Popup::close_id(ui.ctx(), id);
            self.open_settings(settings_ui::Tab::Review);
        }
    }
}

/// The popover's content: its head, one row per plan, then its foot.
fn body(ui: &mut egui::Ui, meters: &[Meter], now: DateTime<Utc>) -> bool {
    let p = theme::palette(ui);
    ui.style_mut().interaction.selectable_labels = false;
    ui.set_width(WIDTH);
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    ui.allocate_ui_with_layout(
        Vec2::new(WIDTH, theme::BAR_H - 1.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add_space(theme::SPACE_4);
            ui.label(
                RichText::new("Subscription usage")
                    .font(theme::ui_title())
                    .color(p.ink),
            );
        },
    );
    widgets::rule(ui);
    let mut open = open_rows(ui, meters);
    let mut settings = false;
    for (at, meter) in meters.iter().enumerate() {
        if at > 0 {
            soft_rule(ui);
        }
        settings |= row(ui, meter, now, &mut open);
    }
    ui.data_mut(|data| data.insert_temp(rows_id(), open));
    widgets::rule(ui);
    settings |= foot(ui, meters);
    settings
}

/// The rows open at rest: the first plan, then whatever the user opened.
fn open_rows(ui: &egui::Ui, meters: &[Meter]) -> HashSet<String> {
    if let Some(open) = ui.data(|data| data.get_temp::<HashSet<String>>(rows_id())) {
        return open;
    }
    meters
        .first()
        .map(|meter| HashSet::from([meter.provider.clone()]))
        .unwrap_or_default()
}

/// One plan's disclosure: its summary, then its detail while open.
fn row(ui: &mut egui::Ui, meter: &Meter, now: DateTime<Utc>, open: &mut HashSet<String>) -> bool {
    let is_open = open.contains(&meter.provider);
    let response = summary(ui, meter, now, is_open);
    if response.clicked() {
        if is_open {
            open.remove(&meter.provider);
        } else {
            open.insert(meter.provider.clone());
        }
    }
    if open.contains(&meter.provider) {
        detail(ui, meter, now)
    } else {
        false
    }
}

/// The three lines of a row's summary: ring and name, gauge, verdict.
fn summary(ui: &mut egui::Ui, meter: &Meter, now: DateTime<Utc>, open: bool) -> egui::Response {
    let p = theme::palette(ui);
    let id = ui.scope_id().with(("usage-row", meter.provider.as_str()));
    let hover = ui.painter().add(Shape::Noop);
    let frame = Frame::new()
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
                    name_and_share(ui, meter);
                    widgets::gauge(
                        ui,
                        meter.share(),
                        meter.elapsed(),
                        widgets::GAUGE_H,
                        &window(meter),
                    );
                    verdict_line(ui, meter, now, open);
                });
            });
        });
    let response = ui
        .interact(frame.response.rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let label = row_label(meter, now);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), open, &label)
    });
    let fade = widgets::hover_fade(ui.ctx(), response.id, response.hovered());
    ui.painter().set(
        hover,
        Shape::rect_filled(
            frame.response.rect,
            CornerRadius::same(theme::RADIUS_SM),
            p.surface_hover.gamma_multiply(fade),
        ),
    );
    response
}

/// The plan's name, and its share at the right, or `No cap`.
fn name_and_share(ui: &mut egui::Ui, meter: &Meter) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(&meter.name)
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

/// The verdict on the left, and the reset with the chevron on the right. With
/// no cap there is no pace, so the reset is the verdict.
fn verdict_line(ui: &mut egui::Ui, meter: &Meter, now: DateTime<Utc>, open: bool) {
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
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            chevron(ui, open);
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
    match meter.cap_reached_on() {
        Some(day) => format!("Over pace · cap by {day}"),
        None if meter.over_pace() => "Over pace".to_string(),
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

/// What VoiceOver reads on a row, and a test finds it by.
fn row_label(meter: &Meter, now: DateTime<Utc>) -> String {
    match meter.share() {
        Some(share) => format!(
            "{} · {:.0}% · {} · {}",
            meter.name,
            share * 100.0,
            verdict(meter),
            super::status::reset_text(meter, now)
        ),
        None => format!(
            "{} · No cap · {}",
            meter.name,
            capitalized_reset(meter, now)
        ),
    }
}

/// The `the week` a gauge's label names.
fn window(meter: &Meter) -> String {
    format!("the {}", meter.plan.window.name().to_lowercase())
}

/// An open row: what it used, its days and its models, and with no cap the
/// link to set one. Returns whether that link was clicked.
fn detail(ui: &mut egui::Ui, meter: &Meter, now: DateTime<Utc>) -> bool {
    let p = theme::palette(ui);
    let mut settings = false;
    Frame::new()
        .inner_margin(MarginF32 {
            left: DETAIL_INDENT,
            right: theme::SPACE_4,
            top: 0.0,
            bottom: theme::SPACE_4,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                ui.label(
                    RichText::new(used_line(meter))
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(runs_line(meter))
                            .font(theme::meta())
                            .color(p.ink_muted),
                    );
                });
            });
            days(ui, meter, now);
            models(ui, meter);
            if meter.plan.cap.is_none() && meter.reported.is_none() {
                let link = RichText::new("Set a cap").font(theme::meta());
                if ui.link(link).clicked() {
                    settings = true;
                }
            }
        });
    settings
}

/// `9,212 of 30,000 AI credits · from GitHub`, `3.1M of 5M tokens`,
/// `$6.20 of $10.00`, or `3.1M tokens burnt`.
fn used_line(meter: &Meter) -> String {
    if let Some(quota) = &meter.reported {
        return format!(
            "{} of {} {} · from GitHub",
            thousands(quota.used as usize),
            thousands(quota.cap as usize),
            quota.unit
        );
    }
    let tokens = thousands(meter.consumed.tokens as usize);
    match meter.plan.cap {
        Some(Cap::Tokens(cap)) => format!("{tokens} of {} tokens", thousands(cap as usize)),
        Some(Cap::Money(cap)) => {
            format!("${:.2} of ${cap:.2}", meter.consumed.dollars(&meter.plan))
        }
        None => format!("{tokens} tokens burnt"),
    }
}

/// `$6.20 API-equivalent · 21 runs`.
fn runs_line(meter: &Meter) -> String {
    let runs = meter.consumed.runs;
    let noun = if runs == 1 { "run" } else { "runs" };
    format!(
        "{} · {runs} {noun}",
        meter.consumed.money_label(&meter.plan)
    )
}

/// The window's tokens, a bar per day: today in full `accent`, the days
/// before it softer, the days after it empty.
fn days(ui: &mut egui::Ui, meter: &Meter, now: DateTime<Utc>) {
    let days = meter.per_day();
    if days.is_empty() {
        return;
    }
    section_label(ui, "Tokens per day");
    let height = DAY_WELL_H + theme::SPACE_1 + DAY_LABEL_H;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let label = format!(
        "Tokens per day this {}",
        meter.plan.window.name().to_lowercase()
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &label));
    if ui.is_rect_visible(rect) {
        paint_days(ui, rect, &days, now);
    }
}

fn paint_days(ui: &egui::Ui, rect: Rect, days: &[DayUse], now: DateTime<Utc>) {
    let p = theme::palette(ui);
    let painter = ui.painter_at(rect);
    let count = days.len();
    let gap = if count > 7 {
        theme::SPACE_1
    } else {
        theme::SPACE_2
    };
    let slot = ((rect.width() - gap * (count - 1) as f32) / count as f32).max(1.0);
    let max = days.iter().map(|day| day.tokens).max().unwrap_or(0);
    let today = now.date_naive();
    for (at, day) in days.iter().enumerate() {
        let left = rect.left() + at as f32 * (slot + gap);
        let well = Rect::from_min_size(Pos2::new(left, rect.top()), Vec2::new(slot, DAY_WELL_H));
        painter.line_segment(
            [well.left_bottom(), well.right_bottom()],
            Stroke::new(1.0, p.line),
        );
        if max > 0 && day.day <= today && day.tokens > 0 {
            let height = (DAY_WELL_H - 1.0) * day.tokens as f32 / max as f32;
            let bar =
                Rect::from_min_max(Pos2::new(left, well.bottom() - height), well.right_bottom());
            let color = if day.today {
                p.accent
            } else {
                p.accent.gamma_multiply(0.55)
            };
            painter.rect_filled(
                bar,
                CornerRadius {
                    nw: 2,
                    ne: 2,
                    ..CornerRadius::ZERO
                },
                color,
            );
        }
        if let Some(label) = day_label(day, at, count) {
            let color = if day.today { p.ink } else { p.ink_faint };
            painter.text(
                Pos2::new(well.center().x, well.bottom() + theme::SPACE_1),
                egui::Align2::CENTER_TOP,
                label,
                theme::meta_mono(),
                color,
            );
        }
    }
}

/// A day's initial in a week, or only the first day's number in a month.
fn day_label(day: &DayUse, at: usize, count: usize) -> Option<String> {
    if count <= 7 {
        return day
            .day
            .format("%a")
            .to_string()
            .chars()
            .next()
            .map(|initial| initial.to_string());
    }
    (at == 0).then(|| day.day.format("%-d").to_string())
}

/// The window's models, largest first, each with a 3pt gauge and its tokens.
fn models(ui: &mut egui::Ui, meter: &Meter) {
    let models = meter.by_model();
    if models.is_empty() {
        return;
    }
    section_label(ui, "By model");
    for model in models {
        model_row(ui, &model);
    }
}

fn model_row(ui: &mut egui::Ui, model: &ModelUse) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let tokens = super::status::short_tokens(model.tokens);
        let tokens_w = widgets::text_width(ui, &tokens, theme::meta());
        let name_w =
            (ui.available_width() - MODEL_GAUGE_W - tokens_w - 2.0 * theme::SPACE_3).max(40.0);
        ui.add_sized(
            [name_w, 16.0],
            egui::Label::new(
                RichText::new(&model.model)
                    .font(theme::ui_small())
                    .color(p.ink),
            )
            .truncate(),
        );
        ui.allocate_ui_with_layout(
            Vec2::new(MODEL_GAUGE_W, widgets::GAUGE_SLIM_H),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                widgets::gauge(
                    ui,
                    Some(model.share),
                    None,
                    widgets::GAUGE_SLIM_H,
                    "the tokens",
                );
            },
        );
        ui.label(RichText::new(tokens).font(theme::meta()).color(p.ink_muted));
    });
}

/// The foot: the even-pace legend when a cap is known, and Settings › AI.
fn foot(ui: &mut egui::Ui, meters: &[Meter]) -> bool {
    let mut settings = false;
    Frame::new()
        .inner_margin(MarginF32 {
            left: theme::SPACE_4,
            right: theme::SPACE_4,
            top: theme::SPACE_3,
            bottom: theme::SPACE_3,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if meters.iter().any(|meter| meter.share().is_some()) {
                    legend(ui);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let link = RichText::new("Settings › AI").font(theme::meta());
                    if ui.link(link).clicked() {
                        settings = true;
                    }
                });
            });
        });
    settings
}

/// The `Even pace` legend: the gauge's tick and its word.
fn legend(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(2.0, 10.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::ZERO, p.ink);
        ui.label(
            RichText::new("Even pace")
                .font(theme::meta())
                .color(p.ink_muted),
        );
    });
}

/// An uppercase `ui-label`, as a section of an open row is headed.
fn section_label(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(
        RichText::new(text.to_uppercase())
            .font(theme::ui_label())
            .color(p.ink_muted),
    );
}

/// The chevron of a disclosure: down at rest, up while open.
fn chevron(ui: &mut egui::Ui, open: bool) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(theme::ICON_SIZE), Sense::hover());
    let center = rect.center();
    let stroke = Stroke::new(1.6, p.ink_muted);
    let (left, right) = if open {
        (center.y + 2.0, center.y - 2.0)
    } else {
        (center.y - 2.0, center.y + 2.0)
    };
    let painter = ui.painter();
    painter.line_segment(
        [Pos2::new(center.x - 4.0, left), Pos2::new(center.x, right)],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(center.x, right), Pos2::new(center.x + 4.0, left)],
        stroke,
    );
}

/// The hairline between two rows: `line-soft`, never a card's edge.
fn soft_rule(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, p.line_soft);
}
