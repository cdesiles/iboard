//! The Settings dialog (`⌘,`), with three tabs: General (whether deleting
//! asks first), AI review (which agent runs the review, and with which model)
//! and Titles (which engine names new issues). See
//! `design/components/{GeneralSettings,ReviewSettings,TitleSettings}`.
//!
//! What the dialog shows about the tools (where each is installed, its
//! version, its models) comes from running them, so it is gathered in the
//! background by a [`Catalog`] that lives for the session.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Id, Key, Layout, Margin, Modifiers, RichText,
    Stroke, Vec2,
};

use crate::agent::{self, AgentConfig, Detected, Models, Tool};
use crate::labels::{self, Labels};
use crate::theme::{self, Icon};
use crate::titles::{self, Availability, Engine};
use crate::widgets::{self, ButtonKind, MenuOption};

/// Width of the dialog, in points.
const DIALOG_WIDTH: f32 = 440.0;

/// Tallest the model menu's list grows before it scrolls.
const MODEL_LIST_MAX_H: f32 = 240.0;

/// A value computed on another thread.
enum Pending<T> {
    Loading(Receiver<T>),
    Ready(T),
}

impl<T: Send + 'static> Pending<T> {
    fn spawn(work: impl FnOnce() -> T + Send + 'static) -> Self {
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(work());
        });
        Pending::Loading(receiver)
    }

    /// Adopts the result once it lands; `lost` stands in for a thread that
    /// died without answering.
    fn poll(&mut self, lost: impl FnOnce() -> T) {
        if let Pending::Loading(receiver) = self {
            match receiver.try_recv() {
                Ok(value) => *self = Pending::Ready(value),
                Err(TryRecvError::Disconnected) => *self = Pending::Ready(lost()),
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    fn ready(&self) -> Option<&T> {
        match self {
            Pending::Ready(value) => Some(value),
            Pending::Loading(_) => None,
        }
    }
}

/// The installed tools and their models, gathered once per session.
#[derive(Default)]
pub struct Catalog {
    /// Run the tools to learn about them. Off in tests, which then see every
    /// tool as missing with no models.
    probe: bool,
    tools: HashMap<Tool, Pending<Detected>>,
    models: HashMap<Tool, Pending<Models>>,
}

impl Catalog {
    /// A catalog that runs the installed tools.
    pub fn probing() -> Self {
        Self {
            probe: true,
            ..Self::default()
        }
    }

    /// Starts looking for every tool, and listing the models of `tool`, unless
    /// that is already known or under way.
    pub fn load(&mut self, tool: Tool) {
        let probe = self.probe;
        for known in Tool::ALL.into_iter().filter(|t| t.binary().is_some()) {
            self.tools.entry(known).or_insert_with(|| {
                if probe {
                    Pending::spawn(move || agent::detect(known))
                } else {
                    Pending::Ready(Detected::default())
                }
            });
        }
        if tool.binary().is_some() {
            self.models.entry(tool).or_insert_with(|| {
                if probe {
                    Pending::spawn(move || agent::list_models(tool))
                } else {
                    Pending::Ready(Models {
                        list: Ok(Vec::new()),
                        default: None,
                    })
                }
            });
        }
    }

    fn retry_models(&mut self, tool: Tool) {
        self.models.remove(&tool);
        self.load(tool);
    }

    /// Adopts finished work; true while some is still running.
    pub fn poll(&mut self) -> bool {
        for pending in self.tools.values_mut() {
            pending.poll(Detected::default);
        }
        for pending in self.models.values_mut() {
            pending.poll(|| Models {
                list: Err("the model list could not be read".to_string()),
                default: None,
            });
        }
        self.tools
            .values()
            .any(|pending| matches!(pending, Pending::Loading(_)))
            || self
                .models
                .values()
                .any(|pending| matches!(pending, Pending::Loading(_)))
    }

    fn detected(&self, tool: Tool) -> Option<&Detected> {
        self.tools.get(&tool).and_then(Pending::ready)
    }

    fn models(&self, tool: Tool) -> Option<&Models> {
        self.models.get(&tool).and_then(Pending::ready)
    }
}

/// Which select's menu is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Menu {
    Tool,
    Model,
}

/// What the dialog asks of the app after a frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Keep,
    Close,
    Save,
}

/// The dialog's tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    General,
    Review,
    Titles,
    Labels,
}

/// The dialog's state while it is open.
pub struct Dialog {
    pub tab: Tab,
    /// The AI review tab was shown, so Save writes its settings.
    pub review_seen: bool,
    /// The settings being edited; the app's copy changes only on Save.
    pub form: AgentConfig,
    /// The title engine being chosen.
    pub engine: Engine,
    /// Settings › General: deleting an issue asks first.
    pub confirm_delete: bool,
    /// Settings › Labels: every label and its `#rrggbb` colour (empty: none).
    pub label_rows: Vec<(String, String)>,
    /// The Labels tab was shown, so Save writes `[labels]`.
    pub labels_seen: bool,
    /// Renames made while editing the rows, as (old, new), in order. The app
    /// rewrites the issues that carry them on Save.
    pub renames: Vec<(String, String)>,
    /// The row whose name is being edited, if any.
    renaming: Option<usize>,
    rename_text: String,
    rename_focus: bool,
    new_label: String,
    new_hex: String,
    label_error: Option<String>,
    /// Each engine's availability, checked when the dialog opened.
    availability: Vec<(Engine, Availability)>,
    /// The on-device model's languages, and the Mac's own.
    languages: Vec<String>,
    user_language: Option<(String, bool)>,
    /// The command validation failure, shown under the Command field.
    invalid: Option<String>,
    /// Why saving failed, shown above the footer.
    pub save_error: Option<String>,
    menu: Option<Menu>,
    /// The model menu's filter.
    filter: String,
    /// The keyboard highlight among the open menu's options.
    highlight: usize,
    /// Give the model filter focus on the next frame.
    focus_filter: bool,
}

/// One pickable row of the model menu: its value (empty for Default), what it
/// shows, and its `meta` note.
struct ModelRow {
    value: String,
    favorite: bool,
}

impl Dialog {
    pub fn new(config: &AgentConfig, engine: Engine, confirm_delete: bool, tab: Tab) -> Self {
        let availability = Engine::ALL
            .into_iter()
            .map(|engine| (engine, titles::availability(engine, config)))
            .collect();
        Self {
            tab,
            review_seen: false,
            form: config.clone(),
            engine,
            confirm_delete,
            label_rows: Vec::new(),
            labels_seen: false,
            renames: Vec::new(),
            renaming: None,
            rename_text: String::new(),
            rename_focus: false,
            new_label: String::new(),
            new_hex: String::new(),
            label_error: None,
            availability,
            languages: titles::apple_languages(),
            user_language: titles::user_language(),
            invalid: None,
            save_error: None,
            menu: None,
            filter: String::new(),
            highlight: 0,
            focus_filter: false,
        }
    }

    /// Fills Settings › Labels: the coloured labels, then the ones issues use
    /// without a colour yet.
    pub fn with_labels(mut self, labels: &Labels, used: impl IntoIterator<Item = String>) -> Self {
        let mut rows = labels.entries();
        for name in used {
            if !rows.iter().any(|(known, _)| *known == name) {
                rows.push((name, String::new()));
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        self.label_rows = rows;
        self
    }

    /// Checks the form before saving; on failure the dialog stays open with
    /// the reason under the field.
    pub fn validate(&mut self) -> bool {
        self.invalid = self.form.validate().err();
        self.invalid.is_none()
    }

    fn open_menu(&mut self, menu: Menu) {
        self.menu = Some(menu);
        self.filter.clear();
        self.focus_filter = menu == Menu::Model;
        self.highlight = 0;
    }

    fn choose_tool(&mut self, tool: Tool, catalog: &mut Catalog) {
        if tool != self.form.tool {
            self.form.tool = tool;
            self.form.model.clear();
            self.invalid = None;
        }
        catalog.load(tool);
        self.menu = None;
    }

    /// Draws the dialog over the current screen and handles its keys.
    pub fn show(&mut self, ui: &egui::Ui, catalog: &mut Catalog, saved_to: &str) -> Action {
        let ctx = ui.ctx();
        catalog.load(self.form.tool);
        if catalog.poll() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // An open menu owns the arrows, `↩` and `esc`; otherwise `esc` closes
        // and `⌘↩` saves.
        let mut action = Action::Keep;
        if self.menu.is_none() {
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter)) {
                action = Action::Save;
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                action = Action::Close;
            }
        }

        let p = theme::palette(ui);
        let dark = ui.visuals().dark_mode;
        let frame = Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .shadow(ui.visuals().popup_shadow);
        egui::Modal::new(Id::new("review-settings"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(if dark { 110 } else { 60 }))
            .show(ctx, |ui| {
                ui.set_width(DIALOG_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                if self.header(ui) {
                    action = Action::Close;
                }
                rule(ui);
                Frame::new()
                    .inner_margin(Margin::same(theme::SPACE_5 as i8))
                    .show(ui, |ui| {
                        ui.set_width(DIALOG_WIDTH - 2.0 * theme::SPACE_5);
                        match self.tab {
                            Tab::General => self.general_body(ui),
                            Tab::Review => {
                                self.review_seen = true;
                                self.body(ui, catalog);
                            }
                            Tab::Titles => self.titles_body(ui),
                            Tab::Labels => {
                                self.labels_seen = true;
                                self.labels_body(ui);
                            }
                        }
                    });
                rule(ui);
                match self.footer(ui, saved_to) {
                    Action::Keep => {}
                    other => action = other,
                }
            });
        action
    }

    /// "Settings", its tabs and the close button; true when close was clicked.
    fn header(&mut self, ui: &mut egui::Ui) -> bool {
        let p = theme::palette(ui);
        let mut close = false;
        Frame::new()
            .inner_margin(Margin {
                left: theme::SPACE_5 as i8,
                right: theme::SPACE_3 as i8,
                top: 0,
                bottom: 0,
            })
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), theme::BAR_H),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.label(
                            RichText::new("Settings")
                                .font(theme::ui_title())
                                .color(p.ink),
                        );
                        ui.add_space(theme::SPACE_4);
                        if let Some(tab) = tabs(ui, self.tab) {
                            self.tab = tab;
                            self.menu = None;
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            close =
                                widgets::button(ui, ButtonKind::Ghost, Some(Icon::Close), "", None)
                                    .on_hover_text("Close (esc)")
                                    .clicked();
                        });
                    },
                );
            });
        close
    }

    fn body(&mut self, ui: &mut egui::Ui, catalog: &mut Catalog) {
        self.tool_field(ui, catalog);
        if self.form.tool == Tool::Custom {
            ui.add_space(theme::SPACE_5);
            self.command_field(ui);
        }
        ui.add_space(theme::SPACE_5);
        self.model_field(ui, catalog);
        if let Some(error) = &self.save_error {
            ui.add_space(theme::SPACE_5);
            help(ui, error, theme::palette(ui).danger);
        }
    }

    fn tool_field(&mut self, ui: &mut egui::Ui, catalog: &mut Catalog) {
        let p = theme::palette(ui);
        label(ui, "Tool");
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

    fn command_field(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        label(ui, "Command");
        let response = text_field(ui, &mut self.form.command, "llm -m {model} < {prompt}");
        if response.changed() {
            self.invalid = None;
        }
        ui.add_space(theme::SPACE_2);
        help(
            ui,
            "iboard fills in {prompt} (a temp file with the issue and instructions), {model} and \
             {issue} (the issue path). The review is read from stdout.",
            p.ink_muted,
        );
        if let Some(invalid) = &self.invalid {
            ui.add_space(theme::SPACE_2);
            help(ui, invalid, p.danger);
        }
    }

    fn model_field(&mut self, ui: &mut egui::Ui, catalog: &mut Catalog) {
        let p = theme::palette(ui);
        label(ui, "Model");
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
    fn model_rows(&self, models: Option<&Models>) -> Vec<ModelRow> {
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
        let p = theme::palette(ui);
        let mut open = true;
        egui::Popup::from_response(anchor)
            .id(Id::new(("review-menu", menu as u8)))
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .gap(theme::SPACE_2)
            .width(anchor.rect.width())
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.line_strong))
                    .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                    .shadow(ui.style().visuals.popup_shadow)
                    .inner_margin(Margin::same(theme::SPACE_2 as i8)),
            )
            .show(|ui| {
                ui.set_width(anchor.rect.width() - 2.0 * theme::SPACE_2);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                content(self, ui);
            });
        // A click on the select itself is handled by `toggle_on`.
        open || anchor.clicked()
    }

    /// The config path on the left, Cancel and Save on the right.
    fn footer(&mut self, ui: &mut egui::Ui, saved_to: &str) -> Action {
        let p = theme::palette(ui);
        let mut action = Action::Keep;
        Frame::new()
            .inner_margin(Margin::symmetric(
                theme::SPACE_5 as i8,
                theme::SPACE_3 as i8,
            ))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), theme::CONTROL_H),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                        if widgets::button(ui, ButtonKind::Primary, None, "Save", Some("⌘↩"))
                            .clicked()
                        {
                            action = Action::Save;
                        }
                        if widgets::button(ui, ButtonKind::Secondary, None, "Cancel", None)
                            .clicked()
                        {
                            action = Action::Close;
                        }
                        ui.add(
                            egui::Label::new(
                                RichText::new(saved_to)
                                    .font(theme::meta())
                                    .color(p.ink_muted),
                            )
                            .truncate(),
                        );
                    },
                );
            });
        action
    }
}

impl Dialog {
    /// Settings › Labels: each label's chip, its `#rrggbb` colour, a way back
    /// to the preset and a trash to remove it; then a row to add one.
    fn labels_body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        label(ui, "Label colours");
        let mut remove = None;
        let mut rows = std::mem::take(&mut self.label_rows);
        let renaming = self.renaming;
        let mut start_rename: Option<(usize, String)> = None;
        let mut commit_rename = false;
        let rows_height = (rows.len() as f32 * 32.0).min(LABEL_LIST_MAX_H);
        Frame::new()
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(Margin::symmetric(
                theme::SPACE_4 as i8,
                theme::SPACE_2 as i8,
            ))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("label-rows")
                    .max_height(rows_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                        for (index, (name, hex)) in rows.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.set_min_height(28.0);
                                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                                if renaming == Some(index) {
                                    let field = ui.add(
                                        egui::TextEdit::singleline(&mut self.rename_text)
                                            .hint_text("name")
                                            .font(FontId::monospace(12.0))
                                            .margin(Margin::symmetric(theme::SPACE_2 as i8, 4))
                                            .char_limit(labels::MAX_LABEL_CHARS)
                                            .desired_width(140.0),
                                    );
                                    if std::mem::take(&mut self.rename_focus) {
                                        field.request_focus();
                                    }
                                    if field.lost_focus() {
                                        commit_rename = true;
                                    }
                                } else {
                                    let pill = widgets::label_pill(
                                        ui,
                                        name.as_str(),
                                        labels::parse_hex(hex),
                                    )
                                    .on_hover_text("Double-click to rename");
                                    if pill.double_clicked() {
                                        start_rename = Some((index, name.clone()));
                                    }
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    // Every label can go, the presets too.
                                    if widgets::icon_button(
                                        ui,
                                        Icon::Trash,
                                        egui::Id::new(("label-remove", index)),
                                        "Remove this label",
                                        widgets::Tone::Danger,
                                    )
                                    .clicked()
                                    {
                                        remove = Some(index);
                                    }
                                    if let Some(preset) = labels::preset_hex(name)
                                        && !hex.eq_ignore_ascii_case(preset)
                                        && widgets::button(
                                            ui,
                                            ButtonKind::Ghost,
                                            None,
                                            "Reset",
                                            None,
                                        )
                                        .on_hover_text(format!("Back to {preset}"))
                                        .clicked()
                                    {
                                        *hex = preset.to_string();
                                    }
                                    hex_field(ui, hex, "no colour");
                                });
                            });
                        }
                    });
            });
        self.label_rows = rows;
        if let Some(index) = remove {
            self.label_rows.remove(index);
        }
        if let Some((index, text)) = start_rename {
            self.renaming = Some(index);
            self.rename_text = text;
            self.rename_focus = true;
        }
        if commit_rename {
            self.commit_rename();
        }

        ui.add_space(theme::SPACE_5);
        label(ui, "Add a label");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            ui.add(
                egui::TextEdit::singleline(&mut self.new_label)
                    .hint_text(RichText::new("name").color(p.ink_faint))
                    .font(FontId::monospace(12.0))
                    .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
                    .min_size(Vec2::new(0.0, theme::CONTROL_H))
                    .desired_width(140.0),
            );
            hex_field(ui, &mut self.new_hex, "#rrggbb");
            let name = self
                .new_label
                .trim()
                .trim_matches(['[', ']'])
                .to_lowercase();
            if !name.is_empty() {
                widgets::label_pill(ui, &name, labels::parse_hex(&self.new_hex));
            }
            if widgets::button(ui, ButtonKind::Secondary, Some(Icon::Plus), "Add", None).clicked() {
                self.label_error = if name.is_empty() {
                    Some("Name the label first.".to_string())
                } else if labels::parse_hex(&self.new_hex).is_none() {
                    Some("The colour must be #rrggbb, for example #1a6e42.".to_string())
                } else {
                    match self.label_rows.iter_mut().find(|(known, _)| *known == name) {
                        Some(row) => row.1 = self.new_hex.trim().to_string(),
                        None => {
                            self.label_rows
                                .push((name, self.new_hex.trim().to_string()));
                            self.label_rows.sort_by(|a, b| a.0.cmp(&b.0));
                        }
                    }
                    self.new_label.clear();
                    self.new_hex.clear();
                    None
                };
            }
        });
        ui.add_space(theme::SPACE_2);
        match &self.label_error {
            Some(error) => help(ui, error, p.danger),
            None => help(
                ui,
                "A label is the [word] at the start of an issue's title. Only changes to iboard's presets are written to [labels].",
                p.ink_muted,
            ),
        }
        if let Some(error) = &self.save_error {
            ui.add_space(theme::SPACE_5);
            help(ui, error, p.danger);
        }
    }

    /// Commits a double-click name edit: normalizes the typed name, updates
    /// the row and remembers the rename so Save can rewrite the issues.
    fn commit_rename(&mut self) {
        let Some(index) = self.renaming.take() else {
            return;
        };
        let Some(old) = self.label_rows.get(index).map(|(name, _)| name.clone()) else {
            self.rename_text.clear();
            return;
        };
        let Some(new) = labels::normalize(&self.rename_text) else {
            self.rename_text.clear();
            return;
        };
        self.rename_text.clear();
        if new == old {
            return;
        }
        if let Some(row) = self.label_rows.get_mut(index) {
            row.0 = new.clone();
        }
        if let Some(rename) = self.renames.iter_mut().find(|(from, _)| *from == old) {
            rename.1 = new;
        } else {
            self.renames.push((old, new));
        }
    }

    /// Settings › General: whether deleting an issue asks first.
    fn general_body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        label(ui, "Deleting issues");
        widgets::checkbox(ui, &mut self.confirm_delete, "Ask before deleting an issue");
        ui.add_space(theme::SPACE_2);
        let help_line = if self.confirm_delete {
            "Delete opens a confirmation first: Del then ↩."
        } else {
            "Delete moves the file to the Trash at once. ⌘Z puts it back, or restore it from the Trash."
        };
        help(ui, help_line, p.ink_muted);
        if let Some(error) = &self.save_error {
            ui.add_space(theme::SPACE_5);
            help(ui, error, p.danger);
        }
    }

    /// Settings › Titles: which engine names new issues, and whether it can
    /// right now.
    fn titles_body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        label(ui, "Name new issues with");
        let mut picked = None;
        let mut fix: Option<Engine> = None;
        Frame::new()
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (index, (engine, availability)) in self.availability.iter().enumerate() {
                    if index > 0 {
                        rule(ui);
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
                    let (clicked, fixed) = choice_row(ui, &row);
                    if clicked {
                        picked = Some(*engine);
                    }
                    if fixed {
                        fix = Some(*engine);
                    }
                }
            });
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

/// Draws a radio row; returns whether it was picked, and whether its fix
/// button was pressed.
fn choice_row(ui: &mut egui::Ui, row: &ChoiceRow) -> (bool, bool) {
    let p = theme::palette(ui);
    let mut fixed = false;
    let inner = Frame::new()
        .fill(if row.selected {
            p.accent_soft
        } else {
            Color32::TRANSPARENT
        })
        .inner_margin(Margin::symmetric(
            theme::SPACE_4 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::new(theme::SPACE_3, theme::SPACE_1);
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
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
                        let (dot, _) =
                            ui.allocate_exact_size(Vec2::splat(6.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot.center(), 3.0, *color);
                    });
                }
            });
            ui.indent("help", |ui| {
                help(ui, row.engine.help(), p.ink_muted);
                if let Some((line, all)) = &row.languages {
                    ui.label(RichText::new(line).font(theme::meta()).color(p.ink_muted))
                        .on_hover_text(all);
                }
                if let Availability::Unavailable { why, fixable, .. } = row.availability {
                    help(ui, why, p.ink);
                    let fix = match (row.engine, fixable) {
                        (Engine::Apple, true) => Some("Open System Settings"),
                        (Engine::Agent, true) => Some("Set up AI review"),
                        _ => None,
                    };
                    if let Some(fix) = fix {
                        ui.add_space(theme::SPACE_2);
                        fixed =
                            widgets::button(ui, ButtonKind::Secondary, None, fix, None).clicked();
                    }
                }
            });
        });
    let clicked = ui
        .interact(
            inner.response.rect,
            Id::new(("title-engine", row.engine.key())),
            egui::Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked();
    (clicked && !fixed, fixed)
}

/// The segmented control in the header; returns the tab clicked, if any.
fn tabs(ui: &mut egui::Ui, current: Tab) -> Option<Tab> {
    let p = theme::palette(ui);
    let mut picked = None;
    Frame::new()
        .fill(p.surface_sunken)
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (tab, name) in [
                (Tab::General, "General"),
                (Tab::Review, "AI review"),
                (Tab::Titles, "Titles"),
                (Tab::Labels, "Labels"),
            ] {
                let active = tab == current;
                let text = RichText::new(name)
                    .font(theme::ui_button())
                    .color(if active { p.ink } else { p.ink_muted });
                let button = egui::Button::new(text)
                    .fill(if active {
                        p.surface
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if active {
                            p.line_strong
                        } else {
                            Color32::TRANSPARENT
                        },
                    ))
                    .corner_radius(CornerRadius::same(theme::RADIUS_SM - 1))
                    .min_size(Vec2::new(0.0, theme::CONTROL_H - 4.0));
                if ui.add(button).clicked() {
                    picked = Some(tab);
                }
            }
        });
    picked
}

/// Opens the Apple Intelligence pane of System Settings.
fn open_apple_intelligence_settings() {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.Siri-Settings.extension")
        .spawn();
}

/// Tallest the label list grows before it scrolls.
const LABEL_LIST_MAX_H: f32 = 240.0;

/// A small mono `#rrggbb` field, edged in `danger` while the value is not a
/// colour.
fn hex_field(ui: &mut egui::Ui, hex: &mut String, hint: &str) -> egui::Response {
    let p = theme::palette(ui);
    let invalid = !hex.trim().is_empty() && labels::parse_hex(hex).is_none();
    let response = ui.add(
        egui::TextEdit::singleline(hex)
            .hint_text(RichText::new(hint).color(p.ink_faint))
            .font(FontId::monospace(12.0))
            .char_limit(7)
            .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
            .min_size(Vec2::new(0.0, theme::CONTROL_H))
            .desired_width(80.0),
    );
    if invalid {
        ui.painter().rect_stroke(
            response.rect,
            CornerRadius::same(theme::RADIUS_SM),
            Stroke::new(1.0, p.danger),
            egui::StrokeKind::Inside,
        );
    }
    response
}

/// A field's label, above its control.
fn label(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(RichText::new(text).font(theme::ui_button()).color(p.ink));
    ui.add_space(theme::SPACE_2);
}

/// The `ui-small` line under a field.
fn help(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.add(egui::Label::new(RichText::new(text).font(theme::ui_small()).color(color)).wrap());
}

/// A full-width mono text field.
fn text_field(ui: &mut egui::Ui, text: &mut String, hint: &str) -> egui::Response {
    let p = theme::palette(ui);
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(RichText::new(hint).color(p.ink_faint))
            .font(FontId::monospace(12.0))
            .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
            .min_size(Vec2::new(0.0, theme::CONTROL_H))
            .desired_width(f32::INFINITY),
    )
}

/// The `line` rule between the header, body and footer.
fn rule(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, p.line);
}

fn menu_separator(ui: &mut egui::Ui) {
    ui.add_space(theme::SPACE_2);
    rule(ui);
    ui.add_space(theme::SPACE_2);
}

/// A line of text inside a menu: loading, or why the list failed.
fn menu_note(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .inner_margin(Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_2 as i8,
        ))
        .show(ui, |ui| help(ui, text, color));
}

/// `path` with the home folder written `~`.
fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        if let Ok(relative) = path.strip_prefix(&home) {
            return format!("~/{}", relative.display());
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models(ids: &[(&str, bool)]) -> Models {
        Models {
            list: Ok(ids
                .iter()
                .map(|(id, favorite)| agent::Model {
                    id: id.to_string(),
                    favorite: *favorite,
                })
                .collect()),
            default: None,
        }
    }

    #[test]
    fn the_model_menu_lists_default_then_favorites_then_all() {
        let dialog = Dialog::new(&AgentConfig::default(), Engine::Manual, true, Tab::Review);
        let models = models(&[("a/one", false), ("b/two", true)]);
        let rows: Vec<(String, bool)> = dialog
            .model_rows(Some(&models))
            .into_iter()
            .map(|row| (row.value, row.favorite))
            .collect();
        assert_eq!(
            rows,
            [
                (String::new(), false),
                ("b/two".to_string(), true),
                ("a/one".to_string(), false),
                ("b/two".to_string(), false),
            ]
        );
    }

    #[test]
    fn the_filter_narrows_models_but_keeps_default() {
        let mut dialog = Dialog::new(&AgentConfig::default(), Engine::Manual, true, Tab::Review);
        dialog.filter = "ONE".into();
        let models = models(&[("a/one", false), ("b/two", true)]);
        let values: Vec<String> = dialog
            .model_rows(Some(&models))
            .into_iter()
            .map(|row| row.value)
            .collect();
        assert_eq!(values, ["", "a/one"]);
    }

    #[test]
    fn changing_the_tool_resets_the_model_to_default() {
        let mut dialog = Dialog::new(
            &AgentConfig {
                tool: Tool::OpenCode,
                model: "a/one".into(),
                ..AgentConfig::default()
            },
            Engine::Manual,
            true,
            Tab::Review,
        );
        let mut catalog = Catalog::default();
        dialog.choose_tool(Tool::Claude, &mut catalog);
        assert_eq!(dialog.form.tool, Tool::Claude);
        assert!(dialog.form.model.is_empty());

        dialog.form.model = "opus".into();
        dialog.choose_tool(Tool::Claude, &mut catalog);
        assert_eq!(dialog.form.model, "opus", "picking the same tool keeps it");
    }

    #[test]
    fn a_custom_command_without_prompt_keeps_the_dialog_open() {
        let mut dialog = Dialog::new(
            &AgentConfig {
                tool: Tool::Custom,
                command: "llm -m {model}".into(),
                ..AgentConfig::default()
            },
            Engine::Manual,
            true,
            Tab::Review,
        );
        assert!(!dialog.validate());
        assert!(dialog.invalid.is_some());
        dialog.form.command.push_str(" < {prompt}");
        assert!(dialog.validate());
    }

    /// Every state goes through a real egui pass, so a panic or a layout
    /// mistake surfaces without a display.
    #[test]
    fn the_dialog_renders_with_each_tool_and_menu() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1280.0, 800.0),
            )),
            ..Default::default()
        };
        let mut catalog = Catalog::default();
        for tool in Tool::ALL {
            for menu in [None, Some(Menu::Tool), Some(Menu::Model)] {
                let mut dialog = Dialog::new(
                    &AgentConfig {
                        tool,
                        ..AgentConfig::default()
                    },
                    Engine::Manual,
                    true,
                    Tab::Review,
                );
                dialog.menu = menu;
                for _ in 0..2 {
                    let mut action = Action::Keep;
                    ctx.run_ui(input.clone(), |ui| {
                        action = dialog.show(ui, &mut catalog, "Saved to ~/x.toml");
                    })
                    .textures_delta
                    .clear();
                    assert_eq!(action, Action::Keep);
                }
            }
        }
    }

    #[test]
    fn every_tab_renders_and_general_toggles_asking() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1280.0, 800.0),
            )),
            ..Default::default()
        };
        let mut catalog = Catalog::default();
        for tab in [Tab::General, Tab::Review, Tab::Titles, Tab::Labels] {
            for confirm in [true, false] {
                let mut dialog = Dialog::new(&AgentConfig::default(), Engine::Apple, confirm, tab)
                    .with_labels(&Labels::presets(), ["ops".to_string()]);
                for _ in 0..2 {
                    let mut action = Action::Keep;
                    ctx.run_ui(input.clone(), |ui| {
                        action = dialog.show(ui, &mut catalog, "Saved to ~/x.toml");
                    })
                    .textures_delta
                    .clear();
                    assert_eq!(action, Action::Keep);
                }
                assert_eq!(dialog.confirm_delete, confirm, "rendering never flips it");
            }
        }
    }
}
