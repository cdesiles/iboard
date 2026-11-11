//! Settings › AI review: what each provider bills, and what its window holds.

use egui::RichText;

use super::Dialog;
use crate::agent::{Billing, Cap, Plan, Provider, Window};
use crate::shell::theme;
use crate::shell::widgets::{self, field_label, help};

/// Width of a plan's numeric field.
const NUMBER_W: f32 = 150.0;

/// The three states of a plan's cap.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CapChoice {
    None,
    Money,
    Tokens,
}

impl CapChoice {
    const ALL: [(CapChoice, &'static str); 3] = [
        (CapChoice::None, "No cap"),
        (CapChoice::Money, "Money"),
        (CapChoice::Tokens, "Tokens"),
    ];

    fn of(plan: &Plan) -> Self {
        match plan.cap {
            Some(Cap::Money(_)) => CapChoice::Money,
            Some(Cap::Tokens(_)) => CapChoice::Tokens,
            None => CapChoice::None,
        }
    }

    /// Changes the cap, keeping its amount when the kind stays the same.
    fn set(self, plan: &mut Plan) {
        match self {
            CapChoice::None => plan.cap = None,
            CapChoice::Money if !matches!(plan.cap, Some(Cap::Money(_))) => {
                plan.cap = Some(Cap::Money(0.0));
            }
            CapChoice::Tokens if !matches!(plan.cap, Some(Cap::Tokens(_))) => {
                plan.cap = Some(Cap::Tokens(0));
            }
            CapChoice::Money | CapChoice::Tokens => {}
        }
    }
}

/// The provider's name in Settings.
fn name(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "Anthropic API",
        Provider::DeepSeek => "DeepSeek",
        Provider::Claude => "Claude.ai",
        Provider::Copilot => "Copilot",
    }
}

/// A label and its control on one line.
fn field(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).font(theme::meta()).color(p.ink_muted));
        add(ui);
    });
}

/// A dollars field; true when it changed.
fn money_field(ui: &mut egui::Ui, value: &mut f64) -> bool {
    ui.add_sized(
        [NUMBER_W, theme::CONTROL_H],
        egui::DragValue::new(value)
            .speed(1.0)
            .range(0.0..=1_000_000.0)
            .prefix("$ ")
            .fixed_decimals(2),
    )
    .changed()
}

/// A token count field; true when it changed.
fn tokens_field(ui: &mut egui::Ui, value: &mut u64) -> bool {
    ui.add_sized(
        [NUMBER_W, theme::CONTROL_H],
        egui::DragValue::new(value)
            .speed(1_000.0)
            .range(0u64..=1_000_000_000_000u64),
    )
    .changed()
}

/// What the plan means for the spend read against it.
fn plan_help(plan: &Plan) -> &'static str {
    match (plan.billing, plan.cap) {
        (Billing::Metered, None) => {
            "Pay as you go: the log's dollars are money spent, with no cap."
        }
        (Billing::Metered, Some(_)) => {
            "Pay as you go, against a cap you set: the log's dollars are money spent."
        }
        (Billing::Subscription, None) => {
            "A subscription: its dollars are API-equivalent, never spend. Without a cap its share is unknown."
        }
        (Billing::Subscription, Some(Cap::Money(_))) => {
            "A subscription: its dollars are API-equivalent, never spend. The cap is the money it includes."
        }
        (Billing::Subscription, Some(Cap::Tokens(_))) => {
            "A subscription: its dollars are API-equivalent, never spend. The cap is the tokens it includes."
        }
    }
}

impl Dialog {
    /// Settings › AI review: the four known providers' plans, then the rule
    /// for one iboard has not seen.
    pub(super) fn usage_body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.add_space(theme::SPACE_5);
        field_label(ui, "Usage plans");
        help(
            ui,
            "What each provider bills. A subscription's dollars are API-equivalent, never counted as spend.",
            p.ink_muted,
        );
        for provider in Provider::ALL {
            ui.add_space(theme::SPACE_5);
            let mut plan = self.usage.plan(provider.key());
            let mut changed = false;
            ui.label(
                RichText::new(name(provider))
                    .font(theme::ui_strong())
                    .color(p.ink),
            );
            ui.add_space(theme::SPACE_2);

            let kinds = [
                (Billing::Metered, Billing::Metered.name()),
                (Billing::Subscription, Billing::Subscription.name()),
            ];
            if let Some(billing) = widgets::segmented(ui, plan.billing, &kinds) {
                plan.billing = billing;
                changed = true;
            }

            if plan.billing == Billing::Subscription {
                field(ui, "Window", |ui| {
                    let windows = Window::ALL.map(|window| (window, window.name()));
                    if let Some(window) = widgets::segmented(ui, plan.window, &windows) {
                        plan.window = window;
                        changed = true;
                    }
                });
                field(ui, "Price", |ui| {
                    let mut price = plan.price.unwrap_or(0.0);
                    if money_field(ui, &mut price) {
                        plan.price = (price > 0.0).then_some(price);
                        changed = true;
                    }
                    ui.label(
                        RichText::new(format!("per {}", plan.window.name().to_lowercase()))
                            .font(theme::meta())
                            .color(p.ink_muted),
                    );
                });
            }

            field(ui, "Cap", |ui| {
                if let Some(choice) = widgets::segmented(ui, CapChoice::of(&plan), &CapChoice::ALL)
                {
                    choice.set(&mut plan);
                    changed = true;
                }
                match &mut plan.cap {
                    Some(Cap::Money(cap)) => changed |= money_field(ui, cap),
                    Some(Cap::Tokens(cap)) => changed |= tokens_field(ui, cap),
                    None => {}
                }
            });

            ui.add_space(theme::SPACE_2);
            help(ui, plan_help(&plan), p.ink_muted);
            if changed {
                self.usage.set(provider.key(), plan);
            }
        }
        ui.add_space(theme::SPACE_5);
        help(
            ui,
            "A provider iboard hasn't seen is metered, with no cap.",
            p.ink_muted,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choosing_a_cap_keeps_the_amount_of_the_same_kind() {
        let mut plan = Plan {
            cap: Some(Cap::Money(12.0)),
            ..Plan::default()
        };
        CapChoice::Money.set(&mut plan);
        assert_eq!(plan.cap, Some(Cap::Money(12.0)));
        CapChoice::Tokens.set(&mut plan);
        assert_eq!(plan.cap, Some(Cap::Tokens(0)));
        CapChoice::None.set(&mut plan);
        assert_eq!(plan.cap, None);
    }

    #[test]
    fn each_provider_has_a_name_and_a_plan() {
        let plans = crate::agent::Plans::default();
        for provider in Provider::ALL {
            assert!(!name(provider).is_empty());
            assert_eq!(
                plans.plan(provider.key()),
                Plan::default_for(provider.key())
            );
        }
    }
}
