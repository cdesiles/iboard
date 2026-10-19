//! Settings › AI review and AI build: which agent runs, and with which model;
//! for the review, also how far it reads.

use egui::{Frame, Id, Key, Margin, Modifiers, RichText, Vec2};

use super::{Catalog, Dialog, Menu};
use crate::agent::{Detected, Models, ReviewMode, Tool};
use crate::format::display_path;
use crate::theme::{self, Icon};
use crate::widgets::{
    self, ButtonKind, MenuOption, field_label, help, menu_note, menu_separator, text_field,
};

/// Tallest the model menu's list grows before it scrolls.
const MODEL_LIST_MAX_H: f32 = 240.0;

/// One pickable row of the model menu: its value (empty for Default), what it
/// shows, and its `meta` note.
pub(super) struct ModelRow {
    pub(super) value: String,
    pub(super) favorite: bool,
}

impl Dialog {
    fn open_menu(&mut self, menu: Menu) {
        self.menu = Some(menu);
        self.filter.clear();
        self.focus_filter = menu == Menu::Model;
        self.highlight = 0;
    }

    pub(super) fn choose_tool(&mut self, tool: Tool, catalog: &mut Catalog) {
        if tool != self.form.tool {
            self.form.tool = tool;
            self.form.model.clear();
            self.invalid = None;
        }
        catalog.load(tool);
        self.menu = None;
    }

    pub(super) fn review_body(&mut self, ui: &mut egui::Ui, catalog: &mut Catalog) {
        self.tool_field(ui, catalog);
        if self.form.tool == Tool::Custom {
            ui.add_space(theme::SPACE_5);
            self.command_field(ui);
        }
        ui.add_space(theme::SPACE_5);
        self.model_field(ui, catalog);
        if self.tab == super::Tab::Review {
            ui.add_space(theme::SPACE_5);
            self.scope_field(ui);
        }
        if let Some(error) = &self.save_error {
            ui.add_space(theme::SPACE_5);
            help(ui, error, theme::palette(ui).danger);
        }
    }

    fn tool_field(&mut self, ui: &mut egui::Ui, catalog: &mut Catalog) {
        let p = theme::palette(ui);
        field_label(ui, "Tool");
        let tool = self.form.tool;
        let open = self.menu == Some(Menu::Tool);
        let response =
            widgets::select(ui, Id::new("review-tool"), tool.name(), tool.binary(), open);
        self.toggle_on(&response, Menu::Tool);

        if self.menu == Some(Menu::Tool) {
            let options = Tool::ALL;
            let picked = self.keyboard(ui, options.len());
            let mut picked = picked.map(|index| options[index]);
            let shown = self.popup(ui, &response, Menu::Tool, |dialog, ui| {
                for (index, option) in options.into_iter().enumerate() {
                    let detected = catalog.detected(option);
                    let (meta, missing) = match (option.binary(), detected) {
                        (None, _) => (String::new(), false),
                        (Some(_), None) => ("…".to_string(), false),
                        (Some(_), Some(found)) if found.path.is_none() => {
                            ("not found".to_string(), true)
                        }
                        (Some(_), Some(found)) => {
                            (found.version.clone().unwrap_or_default(), false)
                        }
                    };
                    let name = match option {
                        Tool::Custom => "Custom command…",
                        other => other.name(),
                    };
                    if widgets::menu_option(
                        ui,
                        MenuOption {
                            name,
                            mono: false,
                            muted: missing,
                            meta: &meta,
                            checked: option == tool,
                            active: index == dialog.highlight,
                        },
                    )
                    .clicked()
                    {
                        picked = Some(option);
                    }
                }
            });
            if let Some(option) = picked {
                self.choose_tool(option, catalog);
            } else if !shown {
                self.menu = None;
            }
        }

        let Some(binary) = self.form.tool.binary() else {
            return;
        };
        ui.add_space(theme::SPACE_2);
        match catalog.detected(self.form.tool) {
            None => help(ui, &format!("Looking for {binary}…"), p.ink_muted),
            Some(Detected { path: None, .. }) => help(
                ui,
                &format!("{binary} is not on your PATH. Install it or pick another tool."),
                p.status_doing,
            ),
            Some(Detected {
                path: Some(path),
                version,
            }) => {
                let mut text = format!("Found {}", display_path(path));
                if let Some(version) = version {
                    text.push_str(&format!(" · {version}"));
                }
                help(ui, &text, p.status_done);
            }
        }
    }

    fn scope_field(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        field_label(ui, "Review scope");
        let options = ReviewMode::ALL.map(|scope| (scope, scope.scope_name()));
        if let Some(scope) = widgets::segmented(ui, self.form.scope, &options) {
            self.form.scope = scope;
        }
        ui.add_space(theme::SPACE_2);
        let text = match self.form.scope {
            ReviewMode::Quick => "Quick: the agent reads the issue alone, without opening files.",
            ReviewMode::Deep => {
                "The agent may also read the repository and its design system to check the issue."
            }
        };
        help(ui, text, p.ink_muted);
    }

    fn command_field(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        field_label(ui, "Command");
        let response = text_field(ui, &mut self.form.command, "llm -m {model} < {prompt}");
        if response.changed() {
            self.invalid = None;
        }
        ui.add_space(theme::SPACE_2);
        help(
            ui,
            if self.tab == super::Tab::Build {
                "iboard fills in {prompt} (a temp file with the issue and instructions), {model} \
                 and {issue} (the issue path). It runs from the project folder and may edit it."
            } else {
                "iboard fills in {prompt} (a temp file with the issue and instructions), {model} \
                 and {issue} (the issue path). The review is read from stdout."
            },
            p.ink_muted,
        );
        if let Some(invalid) = &self.invalid {
            ui.add_space(theme::SPACE_2);
            help(ui, invalid, p.danger);
        }
    }

    fn model_field(&mut self, ui: &mut egui::Ui, catalog: &mut Catalog) {
        let p = theme::palette(ui);
        field_label(ui, "Model");
        let tool = self.form.tool;
        if tool == Tool::Custom {
            text_field(ui, &mut self.form.model, "default");
            ui.add_space(theme::SPACE_2);
            help(
                ui,
                "Leave empty to let the command pick. iboard cannot list models for a custom \
                 command.",
                p.ink_muted,
            );
            return;
        }

        let models = catalog.models(tool).cloned();
        let models = models.as_ref();
        let default = models.and_then(|models| models.default.clone());
        let current = self.form.model.trim().to_string();
        let (value, hint) = if current.is_empty() {
            ("Default".to_string(), default.clone())
        } else {
            (current.clone(), None)
        };
        let open = self.menu == Some(Menu::Model);
        let response = widgets::select(ui, Id::new("review-model"), &value, hint.as_deref(), open);
        self.toggle_on(&response, Menu::Model);

        if self.menu == Some(Menu::Model) {
            let rows = self.model_rows(models);
            let picked = self.keyboard(ui, rows.len());
            let mut picked = picked.map(|index| rows[index].value.clone());
            let mut retry = false;
            let favorites = rows.iter().filter(|row| row.favorite).count();
            let listed = rows.len().saturating_sub(1 + favorites);
            let shown = self.popup(ui, &response, Menu::Model, |dialog, ui| {
                let filter = ui.add(
                    egui::TextEdit::singleline(&mut dialog.filter)
                        .id(Id::new("review-model-filter"))
                        .hint_text(RichText::new("Filter models").color(p.ink_faint))
                        .font(theme::ui_body())
                        .margin(Margin::symmetric(theme::SPACE_3 as i8, 3))
                        .min_size(Vec2::new(0.0, theme::CONTROL_H))
                        .desired_width(f32::INFINITY),
                );
                if std::mem::take(&mut dialog.focus_filter) {
                    filter.request_focus();
                }
                if filter.changed() {
                    dialog.highlight = 0;
                }
                ui.add_space(theme::SPACE_2);
                egui::ScrollArea::vertical()
                    .max_height(MODEL_LIST_MAX_H)
                    .show(ui, |ui| {
                        for (index, row) in rows.iter().enumerate() {
                            if index == 1 {
                                menu_separator(ui);
                                if favorites > 0 {
                                    widgets::menu_group(ui, "Favorites");
                                }
                            }
                            if index == 1 + favorites && listed > 0 {
                                widgets::menu_group(ui, &format!("All models · {listed}"));
                            }
                            let (name, meta) = if row.value.is_empty() {
                                ("Default", default.clone().unwrap_or_default())
                            } else if row.favorite {
                                (row.value.as_str(), "★".to_string())
                            } else {
                                (row.value.as_str(), String::new())
                            };
                            let option = widgets::menu_option(
                                ui,
                                MenuOption {
                                    name,
                                    mono: !row.value.is_empty(),
                                    muted: false,
                                    meta: &meta,
                                    checked: row.value == current,
                                    active: index == dialog.highlight,
                                },
                            );
                            if index == dialog.highlight {
                                option.scroll_to_me(None);
                            }
                            if option.clicked() {
                                picked = Some(row.value.clone());
                            }
                        }
                        match models.map(|models| &models.list) {
                            None => {
                                menu_separator(ui);
                                menu_note(
                                    ui,
                                    &format!("Loading models from {}…", tool.key()),
                                    p.ink_muted,
                                );
                            }
                            Some(Err(error)) => {
                                menu_separator(ui);
                                menu_note(ui, error, p.danger);
                                Frame::new()
                                    .inner_margin(Margin::symmetric(theme::SPACE_3 as i8, 0))
                                    .show(ui, |ui| {
                                        retry = widgets::button(
                                            ui,
                                            ButtonKind::Secondary,
                                            Some(Icon::Refresh),
                                            "Retry",
                                            None,
                                        )
                                        .clicked();
                                    });
                            }
                            Some(Ok(_)) if rows.len() == 1 && !dialog.filter.is_empty() => {
                                menu_note(ui, "No model matches.", p.ink_muted);
                            }
                            Some(Ok(_)) => {}
                        }
                    });
            });
            if retry {
                catalog.retry_models(tool);
            }
            if let Some(model) = picked {
                self.form.model = model;
                self.menu = None;
            } else if !shown {
                self.menu = None;
            }
        }

        ui.add_space(theme::SPACE_2);
        let text = match (tool, models) {
            (Tool::OpenCode, Some(Models { default: None, .. })) => {
                "OpenCode has no favorite model, so Default lets it pick."
            }
            (Tool::Claude, Some(Models { default: None, .. })) => {
                "Claude Code has no model in its settings, so Default lets it pick."
            }
            (Tool::Claude, _) => "Default uses the model in Claude Code's settings.",
            _ => "Default uses the first favorite in OpenCode.",
        };
        help(ui, text, p.ink_muted);
    }

    /// The model menu's pickable rows: Default, the favorites, then every
    /// model, narrowed by the filter.
    pub(super) fn model_rows(&self, models: Option<&Models>) -> Vec<ModelRow> {
        let filter = self.filter.trim().to_lowercase();
        let matches = |text: &str| filter.is_empty() || text.to_lowercase().contains(&filter);
        let mut rows = vec![ModelRow {
            value: String::new(),
            favorite: false,
        }];
        if let Some(Ok(list)) = models.map(|models| &models.list) {
            for model in list
                .iter()
                .filter(|model| model.favorite && matches(&model.id))
            {
                rows.push(ModelRow {
                    value: model.id.clone(),
                    favorite: true,
                });
            }
            for model in list.iter().filter(|model| matches(&model.id)) {
                rows.push(ModelRow {
                    value: model.id.clone(),
                    favorite: false,
                });
            }
        }
        rows
    }

    /// Opens or closes `menu` from its select: a click, or `space` / `↓` while
    /// the select has focus.
    fn toggle_on(&mut self, response: &egui::Response, menu: Menu) {
        let keyed = response.has_focus()
            && self.menu.is_none()
            && response.ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Space)
                    || i.consume_key(Modifiers::NONE, Key::ArrowDown)
            });
        if response.clicked() || keyed {
            if self.menu == Some(menu) {
                self.menu = None;
            } else {
                self.open_menu(menu);
            }
        }
    }

    /// Moves the highlight among `count` options with `↑ ↓`; returns the one
    /// `↩` picked. `esc` closes the menu and gives focus back to the dialog.
    fn keyboard(&mut self, ui: &egui::Ui, count: usize) -> Option<usize> {
        let (up, down, enter, escape) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            self.menu = None;
            return None;
        }
        if count == 0 {
            return None;
        }
        if down {
            self.highlight = (self.highlight + 1).min(count - 1);
        }
        if up {
            self.highlight = self.highlight.saturating_sub(1);
        }
        self.highlight = self.highlight.min(count - 1);
        enter.then_some(self.highlight)
    }

    /// Shows `menu` under its select, as wide as it. False when it closed
    /// because of a click elsewhere.
    fn popup(
        &mut self,
        ui: &egui::Ui,
        anchor: &egui::Response,
        menu: Menu,
        content: impl FnOnce(&mut Self, &mut egui::Ui),
    ) -> bool {
        if self.menu != Some(menu) {
            return false;
        }
        let mut open = true;
        egui::Popup::from_response(anchor)
            .id(Id::new(("review-menu", menu as u8)))
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .gap(theme::SPACE_2)
            .width(anchor.rect.width())
            .frame(widgets::popover_frame(ui).inner_margin(Margin::same(theme::SPACE_2 as i8)))
            .show(|ui| {
                ui.set_width(anchor.rect.width() - 2.0 * theme::SPACE_2);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                content(self, ui);
            });
        // A click on the select itself is handled by `toggle_on`.
        open || anchor.clicked()
    }
}
