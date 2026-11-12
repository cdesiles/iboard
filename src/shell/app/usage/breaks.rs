//! The Usage screen's breakdowns: by model, by project, and by provider on
//! All, each a list with a share bar and its tokens.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::blocks::{self, Read};
use super::chart::compact_tokens;
use super::{
    Align, Color32, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Shape, Vec2, theme,
};
use crate::agent::Billing;
use crate::domains::insights::usage::{ProviderUsage, Range, UsageView};
use crate::format::thousands;
use crate::kernel::labels::Hue;

/// Two panels share the width above this; under it they wrap.
const PAIR_MIN: f32 = 620.0;
/// A row, its name, its share bar and the two right-hand columns.
const ROW_H: f32 = 26.0;
const BAR_W: f32 = 56.0;
const BAR_H: f32 = 4.0;
const TOKENS_W: f32 = 56.0;
const SPEND_W: f32 = 64.0;

/// What a breakdown asked for: a provider to open, or another read.
pub(super) enum Asked {
    Provider(String),
    Retry,
}

/// One row of a breakdown.
struct Item {
    name: String,
    mono: bool,
    share: f32,
    hue: Option<Hue>,
    idle: bool,
    tokens: u64,
    runs: usize,
    spend: Option<f64>,
    provider: Option<String>,
}

/// By model and, in a group, by project; on All, by provider instead.
pub(super) fn breakdowns(
    ui: &mut egui::Ui,
    view: &UsageView,
    chosen: Option<&ProviderUsage>,
    range: Range,
    grouped: bool,
    read: &Read,
) -> Option<Asked> {
    let mut asked = None;
    if let Some(error) = read.failed() {
        blocks::card(ui, |ui| {
            if blocks::failure(ui, error) {
                asked = Some(Asked::Retry);
            }
        });
        return asked;
    }
    if let Some(pulse) = read.waiting() {
        skeleton(ui, grouped, pulse);
        return None;
    }
    match chosen {
        Some(provider) => {
            let models = model_items(provider, range);
            let projects = project_items(provider, range);
            let (mut models_asked, mut projects_asked) = (None, None);
            pair(
                ui,
                grouped,
                |ui| {
                    list_panel(ui, "By model", "", &models, &mut models_asked);
                },
                |ui| {
                    list_panel(ui, "By project", "", &projects, &mut projects_asked);
                },
            );
            asked = models_asked.or(projects_asked);
        }
        None => {
            let providers = provider_items(view, range);
            let projects = all_project_items(view, range);
            let (mut providers_asked, mut projects_asked) = (None, None);
            pair(
                ui,
                grouped,
                |ui| {
                    list_panel(
                        ui,
                        "By provider",
                        "click one to open it",
                        &providers,
                        &mut providers_asked,
                    );
                },
                |ui| {
                    list_panel(ui, "By project", "", &projects, &mut projects_asked);
                },
            );
            asked = providers_asked.or(projects_asked);
        }
    }
    asked
}

fn provider_items(view: &UsageView, range: Range) -> Vec<Item> {
    let total = view.totals(range).tokens;
    view.providers
        .iter()
        .map(|provider| {
            let totals = provider.totals(range);
            Item {
                name: provider.name.clone(),
                mono: false,
                share: share(totals.tokens, total),
                hue: Some(provider.hue),
                idle: !provider.has_run(),
                tokens: totals.tokens,
                runs: totals.runs,
                spend: (provider.billing == Billing::Metered).then_some(totals.spent),
                provider: Some(provider.provider.clone()),
            }
        })
        .collect()
}

fn model_items(provider: &ProviderUsage, range: Range) -> Vec<Item> {
    let metered = provider.billing == Billing::Metered;
    provider
        .models(range)
        .iter()
        .map(|model| Item {
            name: model.name.clone(),
            mono: true,
            share: model.share,
            hue: Some(provider.hue),
            idle: false,
            tokens: model.tokens,
            runs: model.runs,
            spend: metered.then_some(model.spent),
            provider: None,
        })
        .collect()
}

fn project_items(provider: &ProviderUsage, range: Range) -> Vec<Item> {
    let metered = provider.billing == Billing::Metered;
    provider
        .projects(range)
        .iter()
        .map(|project| Item {
            name: project.name.clone(),
            mono: false,
            share: project.share,
            hue: Some(provider.hue),
            idle: false,
            tokens: project.tokens,
            runs: project.runs,
            spend: metered.then_some(project.spent),
            provider: None,
        })
        .collect()
}

fn all_project_items(view: &UsageView, range: Range) -> Vec<Item> {
    let mut by_path: BTreeMap<PathBuf, (String, u64, usize, f64)> = BTreeMap::new();
    for provider in &view.providers {
        for project in provider.projects(range) {
            let entry = by_path
                .entry(project.path.clone())
                .or_insert_with(|| (project.name.clone(), 0, 0, 0.0));
            entry.1 += project.tokens;
            entry.2 += project.runs;
            entry.3 += project.spent;
        }
    }
    let total: u64 = by_path.values().map(|(_, tokens, _, _)| *tokens).sum();
    let mut items: Vec<Item> = by_path
        .into_values()
        .map(|(name, tokens, runs, spent)| Item {
            name,
            mono: false,
            share: share(tokens, total),
            hue: None,
            idle: false,
            tokens,
            runs,
            spend: (spent > 0.0).then_some(spent),
            provider: None,
        })
        .collect();
    items.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.name.cmp(&b.name)));
    items
}

fn share(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}

/// `first` and, when `second` is shown, `second` beside it or under it.
fn pair(
    ui: &mut egui::Ui,
    second: bool,
    first: impl FnOnce(&mut egui::Ui),
    then: impl FnOnce(&mut egui::Ui),
) {
    if !second {
        first(ui);
        return;
    }
    let width = ui.available_width();
    if width < PAIR_MIN {
        first(ui);
        then(ui);
        return;
    }
    let gap = theme::SPACE_4;
    let half = ((width - gap) / 2.0).max(1.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        column(ui, half, first);
        column(ui, half, then);
    });
}

fn column(ui: &mut egui::Ui, width: f32, draw: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        draw(ui);
    });
}

/// A panel of items, largest first, with a header row when it has any.
fn list_panel(
    ui: &mut egui::Ui,
    title: &str,
    caption: &str,
    items: &[Item],
    asked: &mut Option<Asked>,
) {
    let with_spend = items.iter().any(|item| item.spend.is_some());
    blocks::card(ui, |ui| {
        panel_head(ui, title, caption);
        if items.is_empty() {
            let p = theme::palette(ui);
            ui.label(
                RichText::new("No run in this range.")
                    .font(theme::meta())
                    .color(p.ink_faint),
            );
            return;
        }
        header_row(ui, header_name(title), with_spend);
        for item in items {
            row(ui, item, with_spend, asked);
        }
    });
}

/// The header's name column, named after what the panel lists.
fn header_name(title: &str) -> &str {
    match title {
        "By model" => "Model",
        "By project" => "Project",
        _ => "Provider",
    }
}

fn panel_head(ui: &mut egui::Ui, title: &str, caption: &str) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        ui.label(RichText::new(title).font(theme::ui_title()).color(p.ink));
        if !caption.is_empty() {
            ui.label(
                RichText::new(caption)
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        }
    });
}

fn header_row(ui: &mut egui::Ui, label: &str, with_spend: bool) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label.to_uppercase())
                .font(theme::ui_label())
                .color(p.ink_muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if with_spend {
                ui.label(
                    RichText::new("Spend")
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
                ui.add_space(theme::SPACE_4);
            }
            ui.label(
                RichText::new("Tokens")
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        });
    });
}

/// One row; a provider row opens that provider when clicked.
fn row(ui: &mut egui::Ui, item: &Item, with_spend: bool, asked: &mut Option<Asked>) {
    let p = theme::palette(ui);
    let hover = ui.painter().add(Shape::Noop);
    let inner = ui.horizontal(|ui| {
        ui.set_min_height(ROW_H);
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let spend = if with_spend { SPEND_W } else { 0.0 };
        let name_w =
            (ui.available_width() - BAR_W - TOKENS_W - spend - 3.0 * theme::SPACE_3).max(40.0);
        let font = if item.mono {
            egui::FontId::new(12.0, theme::edit_body().family)
        } else {
            theme::ui_small()
        };
        ui.add_sized(
            [name_w, ROW_H],
            egui::Label::new(RichText::new(&item.name).font(font).color(p.ink)).truncate(),
        );
        if item.idle {
            ui.add_sized(
                [BAR_W, ROW_H],
                egui::Label::new(
                    RichText::new("No run yet")
                        .font(theme::meta())
                        .color(p.ink_faint),
                )
                .truncate(),
            );
        } else {
            share_bar(
                ui,
                item.share,
                item.hue.map_or(p.accent, |hue| p.label(hue)),
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(spend) = item.spend {
                ui.add_sized(
                    [SPEND_W, ROW_H],
                    egui::Label::new(
                        RichText::new(format!("${spend:.2}"))
                            .font(theme::meta())
                            .color(p.ink),
                    )
                    .halign(Align::RIGHT)
                    .truncate(),
                );
            }
            ui.add_sized(
                [TOKENS_W, ROW_H],
                egui::Label::new(
                    RichText::new(compact_tokens(item.tokens))
                        .font(theme::meta())
                        .color(p.ink_muted),
                )
                .halign(Align::RIGHT)
                .truncate(),
            );
        });
    });
    let detail = detail(item);
    let Some(provider) = item.provider.as_deref() else {
        inner
            .response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), &detail));
        return;
    };
    let id = ui.scope_id().with(("usage-break", provider));
    let response = ui
        .interact(inner.response.rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &detail));
    if ui.is_rect_visible(inner.response.rect) {
        let fade = super::widgets::hover_fade(ui.ctx(), response.id, response.hovered());
        ui.painter().set(
            hover,
            Shape::rect_filled(
                inner.response.rect,
                CornerRadius::same(theme::RADIUS_SM),
                p.surface_hover.gamma_multiply(fade),
            ),
        );
    }
    if response.clicked() {
        *asked = Some(Asked::Provider(provider.to_string()));
    }
}

fn share_bar(ui: &mut egui::Ui, share: f32, color: Color32) {
    let p = theme::palette(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(BAR_W, BAR_H), Sense::hover());
    let label = format!("{:.0}% of the tokens", share * 100.0);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &label));
    let radius = CornerRadius::same(BAR_H as u8 / 2);
    ui.painter().rect_filled(rect, radius, p.surface_sunken);
    let filled = Rect::from_min_size(
        rect.min,
        Vec2::new(rect.width() * share.clamp(0.0, 1.0), BAR_H),
    );
    ui.painter().rect_filled(filled, radius, color);
}

fn detail(item: &Item) -> String {
    if item.idle {
        return format!("{} · No run yet", item.name);
    }
    let mut text = format!(
        "{} · {} tokens · {} {}",
        item.name,
        thousands(item.tokens as usize),
        item.runs,
        if item.runs == 1 { "run" } else { "runs" }
    );
    if let Some(spend) = item.spend {
        text.push_str(&format!(" · ${spend:.2}"));
    }
    text
}

fn skeleton(ui: &mut egui::Ui, grouped: bool, pulse: bool) {
    let rows = |ui: &mut egui::Ui| {
        for _ in 0..4 {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::hover());
            if pulse {
                let fill = theme::skeleton_fill(ui.ctx(), theme::palette(ui));
                let bar = Rect::from_min_size(
                    Pos2::new(rect.left(), rect.center().y - 4.0),
                    Vec2::new((rect.width() * 0.42).max(8.0), 8.0),
                );
                ui.painter()
                    .rect_filled(bar, CornerRadius::same(theme::RADIUS_SM), fill);
            }
        }
    };
    pair(
        ui,
        grouped,
        |ui| {
            blocks::card(ui, |ui| {
                panel_head(ui, "By model", "");
                rows(ui);
            });
        },
        |ui| {
            blocks::card(ui, |ui| {
                panel_head(ui, "By project", "");
                rows(ui);
            });
        },
    );
}
