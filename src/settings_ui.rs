//! The Settings dialog (`⌘,`), with five tabs: General (the theme, and
//! whether deleting asks first), AI review (which agent runs the review, and with which model),
//! AI build (the same, for the agent that implements Ready issues), Titles (which engine names new issues) and Labels (label colours). See
//! `design/components/{GeneralSettings,ReviewSettings,TitleSettings}`.
//!
//! One module per tab. What the dialog shows about the tools (where each is
//! installed, its version, its models) comes from running them, so it is
//! gathered in the background by a [`Catalog`] that lives for the session.

mod catalog;
mod general;
mod labels;
mod review;
mod titles;

use egui::{Color32, Frame, Id, Key, Margin, Modifiers, Vec2};

use crate::agent::AgentConfig;
use crate::general::Appearance;
use crate::job;
use crate::labels::Labels;
use crate::theme;
use crate::titles::{Availability, Engine};
use crate::widgets::{self, FooterClick};
pub use catalog::Catalog;

/// Width of the dialog, in points.
const DIALOG_WIDTH: f32 = 440.0;

/// Height of the tab body. Fixed, so the panel stands still when the tab
/// changes; taller content scrolls inside it.
const DIALOG_BODY_H: f32 = 420.0;

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
    Build,
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
    /// The AI build tab was shown, so Save writes its settings.
    pub build_seen: bool,
    /// The AI build agent being edited. The AI build tab edits it through
    /// the AI review tab's fields, swapped into `form` while it draws.
    pub build_form: AgentConfig,
    /// The title engine being chosen.
    pub engine: Engine,
    /// Settings › General: deleting an issue asks first.
    pub confirm_delete: bool,
    /// Settings › General: the theme.
    pub appearance: Appearance,
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

impl Dialog {
    pub fn new(config: &AgentConfig, engine: Engine, confirm_delete: bool, tab: Tab) -> Self {
        let availability = Engine::ALL
            .into_iter()
            .map(|engine| (engine, crate::titles::availability(engine, config)))
            .collect();
        Self {
            tab,
            review_seen: false,
            form: config.clone(),
            build_seen: false,
            build_form: config.clone(),
            engine,
            confirm_delete,
            appearance: Appearance::default(),
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
            languages: crate::titles::apple_languages(),
            user_language: crate::titles::user_language(),
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

    /// Starts Settings › AI build on the agent that implements issues.
    pub fn with_build(mut self, build: &AgentConfig) -> Self {
        self.build_form = build.clone();
        self
    }

    /// Starts Settings › General on the current theme.
    pub fn with_appearance(mut self, appearance: Appearance) -> Self {
        self.appearance = appearance;
        self
    }

    /// Checks the form before saving; on failure the dialog stays open with
    /// the reason under the field.
    pub fn validate(&mut self) -> bool {
        self.invalid = self.form.validate().err();
        self.invalid.is_none()
    }

    /// [`Dialog::validate`], for the AI build tab.
    pub fn validate_build(&mut self) -> bool {
        self.invalid = self.build_form.validate().err();
        self.invalid.is_none()
    }

    /// Settings › AI build: the AI review tab's fields, on the build agent.
    fn build_body(&mut self, ui: &mut egui::Ui, catalog: &mut Catalog) {
        std::mem::swap(&mut self.form, &mut self.build_form);
        self.review_body(ui, catalog);
        std::mem::swap(&mut self.form, &mut self.build_form);
    }

    /// Draws the dialog over the current screen and handles its keys.
    pub fn show(&mut self, ui: &egui::Ui, catalog: &mut Catalog, saved_to: &str) -> Action {
        let ctx = ui.ctx();
        catalog.load(self.form.tool);
        if catalog.poll() {
            ctx.request_repaint_after(job::POLL_EVERY);
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

        let dark = ui.visuals().dark_mode;
        egui::Modal::new(Id::new("review-settings"))
            .frame(widgets::popover_frame(ui))
            .backdrop_color(Color32::from_black_alpha(if dark { 110 } else { 60 }))
            .show(ctx, |ui| {
                ui.set_width(DIALOG_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                if self.header(ui) {
                    action = Action::Close;
                }
                widgets::rule(ui);
                Frame::new()
                    .inner_margin(Margin::same(theme::SPACE_5 as i8))
                    .show(ui, |ui| {
                        ui.set_width(DIALOG_WIDTH - 2.0 * theme::SPACE_5);
                        egui::ScrollArea::vertical()
                            .id_salt("settings-body")
                            .min_scrolled_height(DIALOG_BODY_H)
                            .max_height(DIALOG_BODY_H)
                            .auto_shrink([false, false])
                            .show(ui, |ui| match self.tab {
                                Tab::General => self.general_body(ui),
                                Tab::Review => {
                                    self.review_seen = true;
                                    self.review_body(ui, catalog);
                                }
                                Tab::Build => {
                                    self.build_seen = true;
                                    self.build_body(ui, catalog);
                                }
                                Tab::Titles => self.titles_body(ui),
                                Tab::Labels => {
                                    self.labels_seen = true;
                                    self.labels_body(ui);
                                }
                            });
                    });
                widgets::rule(ui);
                match widgets::modal_footer(ui, saved_to, "Save") {
                    FooterClick::None => {}
                    FooterClick::Primary => action = Action::Save,
                    FooterClick::Cancel => action = Action::Close,
                }
            });
        action
    }

    /// "Settings", its tabs and the close button; true when close was clicked.
    fn header(&mut self, ui: &mut egui::Ui) -> bool {
        widgets::modal_header(ui, "Settings", |ui| {
            if let Some(tab) = tabs(ui, self.tab) {
                self.tab = tab;
                self.menu = None;
                self.invalid = None;
            }
        })
    }
}

/// The segmented control in the header; returns the tab clicked, if any.
fn tabs(ui: &mut egui::Ui, current: Tab) -> Option<Tab> {
    widgets::segmented(
        ui,
        current,
        &[
            (Tab::General, "General"),
            (Tab::Review, "AI review"),
            (Tab::Build, "AI build"),
            (Tab::Titles, "Titles"),
            (Tab::Labels, "Labels"),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{self, Models, Tool};

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
        for tab in [
            Tab::General,
            Tab::Review,
            Tab::Build,
            Tab::Titles,
            Tab::Labels,
        ] {
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
