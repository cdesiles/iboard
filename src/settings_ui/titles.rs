//! Settings › Titles: which engine names new issues.

use egui::{Align, Color32, CornerRadius, Frame, Layout, Rect, RichText, Stroke, UiBuilder, Vec2};

use super::{Dialog, Tab};
use crate::theme;
use crate::titles::{Availability, Engine};
use crate::widgets::{self, ButtonKind, field_label, help};

/// Width of a row's radio.
const RADIO_W: f32 = 14.0;

/// One engine in the Titles radio list.
struct ChoiceRow<'a> {
    engine: Engine,
    selected: bool,
    /// The word on the right, and its colour.
    state: Option<(String, Color32)>,
    availability: &'a Availability,
    /// The languages line and its tooltip.
    languages: Option<(String, String)>,
}

/// Draws a radio row; returns its response, and whether its fix button was
/// pressed. The row senses before its content, so the button stays on top.
fn choice_row(ui: &mut egui::Ui, row: &ChoiceRow) -> (egui::Response, bool) {
    let p = theme::palette(ui);
    let mut fixed = false;
    let builder = UiBuilder::new()
        .id_salt(("title-engine", row.engine.key()))
        .sense(egui::Sense::click());
    let inner = ui.scope_builder(builder, |ui| {
        Frame::new()
            .fill(if row.selected {
                p.accent_soft
            } else {
                Color32::TRANSPARENT
            })
            .inner_margin(theme::ROW_PAD)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = Vec2::new(theme::SPACE_3, theme::SPACE_1);
                row_title(ui, row);
                ui.horizontal(|ui| {
                    ui.add_space(RADIO_W + theme::SPACE_3);
                    ui.vertical(|ui| fixed = row_details(ui, row));
                });
            });
    });
    let response = inner
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            ui.is_enabled(),
            row.selected,
            row.engine.name(),
        )
    });
    (response, fixed)
}

/// A row's radio, its name and its state.
fn row_title(ui: &mut egui::Ui, row: &ChoiceRow) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(RADIO_W), egui::Sense::hover());
        let edge = if row.selected {
            p.accent
        } else {
            p.line_strong
        };
        ui.painter()
            .circle(dot.center(), 6.5, p.surface, Stroke::new(1.0, edge));
        if row.selected {
            ui.painter().circle_filled(dot.center(), 3.5, p.accent);
        }
        ui.label(
            RichText::new(row.engine.name())
                .font(theme::ui_strong())
                .color(p.ink),
        );
        if let Some((state, color)) = &row.state {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(state).font(theme::meta()).color(*color));
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(6.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.0, *color);
            });
        }
    });
}

/// A row's lines under its name, and its fix; true when the fix was pressed.
fn row_details(ui: &mut egui::Ui, row: &ChoiceRow) -> bool {
    let p = theme::palette(ui);
    help(ui, row.engine.help(), p.ink_muted);
    if let Some((line, all)) = &row.languages {
        ui.label(RichText::new(line).font(theme::meta()).color(p.ink_muted))
            .on_hover_text(all);
    }
    let Availability::Unavailable { why, fixable, .. } = row.availability else {
        return false;
    };
    help(ui, why, p.ink);
    let fix = match (row.engine, fixable) {
        (Engine::Apple, true) => "Open System Settings",
        (Engine::Agent, true) => "Set up AI review",
        _ => return false,
    };
    ui.add_space(theme::SPACE_2);
    widgets::button(ui, ButtonKind::Secondary, None, fix, None).clicked()
}

/// Opens the Apple Intelligence pane of System Settings.
fn open_apple_intelligence_settings() {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.Siri-Settings.extension")
        .spawn();
}

impl Dialog {
    /// Settings › Titles: which engine names new issues, and whether it can
    /// right now.
    pub(super) fn titles_body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        field_label(ui, "Name new issues with");
        let mut picked = None;
        let mut fix: Option<Engine> = None;
        let mut focused: Option<Rect> = None;
        Frame::new()
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (index, (engine, availability)) in self.availability.iter().enumerate() {
                    if index > 0 {
                        widgets::rule(ui);
                    }
                    let state = match (engine, availability) {
                        (Engine::Apple, Availability::Ready) => {
                            Some(("Ready".to_string(), p.status_done))
                        }
                        (Engine::Agent, Availability::Ready) => {
                            Some((self.form.label(), p.status_done))
                        }
                        (Engine::Manual, _) => None,
                        (_, Availability::Unavailable { state, fixable, .. }) => Some((
                            state.to_string(),
                            if *fixable {
                                p.status_doing
                            } else {
                                p.ink_muted
                            },
                        )),
                    };
                    let languages = (*engine == Engine::Apple && availability.is_ready())
                        .then(|| self.languages_line())
                        .flatten();
                    let row = ChoiceRow {
                        engine: *engine,
                        selected: self.engine == *engine,
                        state,
                        availability,
                        languages,
                    };
                    let (response, fixed) = choice_row(ui, &row);
                    if response.clicked() {
                        picked = Some(*engine);
                    }
                    if response.has_focus() {
                        focused = Some(response.rect);
                    }
                    if fixed {
                        fix = Some(*engine);
                    }
                }
            });
        if let Some(rect) = focused {
            widgets::paint_focus_ring(ui, rect, theme::RADIUS_SM);
        }
        if let Some(engine) = picked {
            self.engine = engine;
        }
        match fix {
            Some(Engine::Apple) => open_apple_intelligence_settings(),
            Some(Engine::Agent) => self.tab = Tab::Review,
            _ => {}
        }
        let chosen_ready = self
            .availability
            .iter()
            .find(|(engine, _)| *engine == self.engine)
            .is_some_and(|(_, availability)| availability.is_ready());
        ui.add_space(theme::SPACE_3);
        if chosen_ready {
            help(
                ui,
                "A description in a language the model doesn't support shows the title line instead, with your text kept.",
                p.ink_muted,
            );
        } else {
            help(
                ui,
                "Until it's ready, the composer asks you for a title. iboard never switches to the review agent by itself, because that sends text over the network.",
                p.status_doing,
            );
        }
        if let Some(error) = &self.save_error {
            ui.add_space(theme::SPACE_5);
            help(ui, error, p.danger);
        }
    }

    /// "English, Français, Deutsch, Español +11 · yours: Français", with the
    /// full list in a tooltip.
    fn languages_line(&self) -> Option<(String, String)> {
        if self.languages.is_empty() {
            return None;
        }
        let shown = 4;
        let mut line = self
            .languages
            .iter()
            .take(shown)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if self.languages.len() > shown {
            line.push_str(&format!(" +{}", self.languages.len() - shown));
        }
        match &self.user_language {
            Some((name, true)) => line.push_str(&format!(" · yours: {name}")),
            Some((name, false)) => line.push_str(&format!(" · not yours ({name})")),
            None => {}
        }
        Some((line, self.languages.join(", ")))
    }
}
