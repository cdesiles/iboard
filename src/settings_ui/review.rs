//! Settings › AI review and AI build: which agent runs, and with which model;
//! for the review, also how far it reads, and for the build how long it works.

use egui::{Frame, Id, Vec2};

use super::select_menu::Menu;
use super::{Catalog, Dialog};
use crate::agent::{BuildTimeout, Detected, Models, ReviewMode, Tool};
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

/// The model menu's options: Default, then the favorites and every model
/// under their group heads, the highlight scrolled into view. Returns the
/// value clicked.
fn model_list(
    ui: &mut egui::Ui,
    rows: &[ModelRow],
    highlight: usize,
    current: &str,
    default: Option<&str>,
) -> Option<String> {
    let favorites = rows.iter().filter(|row| row.favorite).count();
    let listed = rows.len().saturating_sub(1 + favorites);
    let mut picked = None;
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
            ("Default", default.unwrap_or_default())
        } else if row.favorite {
            (row.value.as_str(), "★")
        } else {
            (row.value.as_str(), "")
        };
        let option = widgets::menu_option(
            ui,
            MenuOption {
                meta_is_key: false,
                name,
                mono: !row.value.is_empty(),
                muted: false,
                meta,
                checked: row.value == current,
                active: index == highlight,
            },
        );
        if index == highlight {
            option.scroll_to_me(None);
        }
        if option.clicked() {
            picked = Some(row.value.clone());
        }
    }
    picked
}

/// What the model menu says under its options: the models are loading, why
/// listing them failed (with Retry), or, when `unmatched`, that no model
/// matches the filter. True when Retry was clicked.
fn list_status(ui: &mut egui::Ui, tool: Tool, models: Option<&Models>, unmatched: bool) -> bool {
    let p = theme::palette(ui);
    let mut retry = false;
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
                .inner_margin(Vec2::new(theme::SPACE_3, 0.0))
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
        Some(Ok(_)) if unmatched => menu_note(ui, "No model matches.", p.ink_muted),
        Some(Ok(_)) => {}
    }
    retry
}

/// The help under the model select: what Default runs.
fn model_help(tool: Tool, models: Option<&Models>) -> &'static str {
    match (tool, models) {
        (Tool::OpenCode, Some(Models { default: None, .. })) => {
            "OpenCode has no favorite model, so Default lets it pick."
        }
        (Tool::Claude, Some(Models { default: None, .. })) => {
            "Claude Code has no model in its settings, so Default lets it pick."
        }
        (Tool::Claude, _) => "Default uses the model in Claude Code's settings.",
        _ => "Default uses the first favorite in OpenCode.",
    }
}

impl Dialog {
    pub(super) fn choose_tool(&mut self, tool: Tool, catalog: &mut Catalog) {
        if tool != self.form.tool {
            self.form.tool = tool;
            self.form.model.clear();
            self.invalid = None;
        }
        catalog.load(tool);
        self.menu.close();
    }

    pub(super) fn review_body(&mut self, ui: &mut egui::Ui, catalog: &mut Catalog) {
        self.tool_field(ui, catalog);
        if self.form.tool == Tool::Custom {
            ui.add_space(theme::SPACE_5);
            self.command_field(ui);
        }
        ui.add_space(theme::SPACE_5);
        self.model_field(ui, catalog);
        ui.add_space(theme::SPACE_5);
        if self.tab == super::Tab::Review {
            self.scope_field(ui);
        } else {
            self.timeout_field(ui);
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
        let open = self.menu.is_open(Menu::Tool);
        let response = widgets::select(
            ui,
            Id::unique("review-tool"),
            tool.name(),
            tool.binary(),
            open,
        );
        self.menu.toggle_on(&response, Menu::Tool);

        if self.menu.is_open(Menu::Tool) {
            let options = Tool::ALL;
            let picked = self.menu.keyboard(ui, options.len());
            let mut picked = picked.map(|index| options[index]);
            self.menu.popup(ui, &response, Menu::Tool, |menu, ui| {
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
                            meta_is_key: false,
                            name,
                            mono: false,
                            muted: missing,
                            meta: &meta,
                            checked: option == tool,
                            active: index == menu.highlight,
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

    fn timeout_field(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        field_label(ui, "Timeout");
        let current = self.build_timeout;
        let open = self.menu.is_open(Menu::Timeout);
        let response = widgets::select(
            ui,
            Id::unique("build-timeout"),
            &current.name(),
            current.hint(),
            open,
        );
        self.menu.toggle_on(&response, Menu::Timeout);

        if self.menu.is_open(Menu::Timeout) {
            let options = BuildTimeout::ALL;
            let picked = self.menu.keyboard(ui, options.len());
            let mut picked = picked.map(|index| options[index]);
            self.menu.popup(ui, &response, Menu::Timeout, |menu, ui| {
                for (index, option) in options.into_iter().enumerate() {
                    if widgets::menu_option(
                        ui,
                        MenuOption {
                            meta_is_key: false,
                            name: &option.name(),
                            mono: false,
                            muted: false,
                            meta: option.hint().unwrap_or_default(),
                            checked: option == current,
                            active: index == menu.highlight,
                        },
                    )
                    .clicked()
                    {
                        picked = Some(option);
                    }
                }
            });
            if let Some(option) = picked {
                self.build_timeout = option;
                self.menu.close();
            }
        }

        ui.add_space(theme::SPACE_2);
        help(
            ui,
            "A build or an Ask is stopped after this long.",
            p.ink_muted,
        );
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
        let open = self.menu.is_open(Menu::Model);
        let response = widgets::select(
            ui,
            Id::unique("review-model"),
            &value,
            hint.as_deref(),
            open,
        );
        self.menu.toggle_on(&response, Menu::Model);

        if self.menu.is_open(Menu::Model) {
            let rows = self.model_rows(models);
            let picked = self.menu.keyboard(ui, rows.len());
            let mut picked = picked.map(|index| rows[index].value.clone());
            let mut retry = false;
            self.menu.popup(ui, &response, Menu::Model, |menu, ui| {
                menu.filter_field(ui, Id::unique("review-model-filter"), "Filter models");
                ui.add_space(theme::SPACE_2);
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .content_margin(theme::scroll_margin(0.0, 0.0))
                        .max_height(MODEL_LIST_MAX_H),
                    |ui| {
                        if let Some(row) =
                            model_list(ui, &rows, menu.highlight, &current, default.as_deref())
                        {
                            picked = Some(row);
                        }
                        let unmatched = rows.len() == 1 && !menu.filter.is_empty();
                        retry = list_status(ui, tool, models, unmatched);
                    },
                );
            });
            if retry {
                catalog.retry_models(tool);
            }
            if let Some(model) = picked {
                self.form.model = model;
                self.menu.close();
            }
        }

        ui.add_space(theme::SPACE_2);
        help(ui, model_help(tool, models), p.ink_muted);
    }

    /// The model menu's pickable rows: Default, the favorites, then every
    /// model, narrowed by the filter.
    pub(super) fn model_rows(&self, models: Option<&Models>) -> Vec<ModelRow> {
        let filter = self.menu.filter.trim().to_lowercase();
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
}
