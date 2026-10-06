//! The Design panel: browse the project's design system and scaffold a
//! component.
//!
//! Opened from the board header. It reads the system the shared `design` module
//! loaded and renders a component's preview through the out-of-process helper,
//! so the board itself never loads WebKit: startup stays instant and a broken
//! preview cannot take the window down. When no helper is available the panel
//! simply says so.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use iboard::design::{self, DesignSystem};

use crate::theme;
use crate::widgets::{self, ButtonKind};

/// What the panel asks the app to do after a frame.
pub enum Action {
    Keep,
    Close,
    /// Scaffold a new component (the deterministic part is the library's).
    Create {
        name: String,
        group: String,
        intent: String,
    },
    /// Render a component's preview to a PNG.
    Render(String),
}

/// The Design panel's session state.
pub struct Panel {
    selected: Option<String>,
    form_open: bool,
    name: String,
    group: String,
    intent: String,
    /// The last outcome ("Created …"), shown under the form.
    pub status: Option<String>,
    render: Render,
}

enum Render {
    Idle,
    Running(Receiver<Result<PathBuf, String>>),
    Ready(egui::TextureHandle),
    Failed(String),
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            selected: None,
            form_open: false,
            name: String::new(),
            group: "Components".to_string(),
            intent: String::new(),
            status: None,
            render: Render::Idle,
        }
    }
}

impl Panel {
    /// Adopt a render that is running on a background thread.
    pub fn start_render(&mut self, receiver: Receiver<Result<PathBuf, String>>) {
        self.render = Render::Running(receiver);
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        system: Option<&DesignSystem>,
        error: Option<&str>,
        render_available: bool,
    ) -> Action {
        self.poll(ctx);
        let mut action = Action::Keep;

        egui::Window::new("Design system")
            .collapsible(false)
            .resizable(true)
            .default_width(760.0)
            .default_height(560.0)
            .show(ctx, |ui| {
                let p = theme::palette(ui);
                ui.horizontal(|ui| {
                    let title = system
                        .map(|system| system.meta.title.clone())
                        .unwrap_or_else(|| "No design system".to_string());
                    ui.label(egui::RichText::new(title).color(p.ink).strong());
                    if let Some(system) = system {
                        ui.label(
                            egui::RichText::new(format!("· {}", system.meta.namespace))
                                .color(p.ink_muted),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::button(ui, ButtonKind::Secondary, None, "Close", None).clicked()
                        {
                            action = Action::Close;
                        }
                    });
                });

                let Some(system) = system else {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(error.unwrap_or("No design system in this project."))
                            .color(p.ink_muted),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Add design/design-system.json, or `idesign import <package>`.",
                        )
                        .color(p.ink_faint)
                        .size(11.0),
                    );
                    return;
                };

                ui.label(
                    egui::RichText::new(format!(
                        "{} · {} components · {} color tokens",
                        system.tokens.themes.len().max(1),
                        system.components.len(),
                        system.tokens.color.len(),
                    ))
                    .color(p.ink_muted)
                    .size(11.0),
                );
                ui.separator();

                if let Some(status) = &self.status {
                    ui.label(egui::RichText::new(status).color(p.ink_muted).size(11.0));
                }

                self.new_component_ui(ui, &mut action);
                ui.separator();

                let mut selected = self.selected.clone();
                egui::ScrollArea::vertical()
                    .id_salt("design-components")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        if system.components.is_empty() {
                            ui.label(
                                egui::RichText::new("No components yet.")
                                    .color(p.ink_faint)
                                    .size(11.0),
                            );
                        }
                        for component in &system.components {
                            let is_selected = selected.as_deref() == Some(&component.name);
                            if ui.selectable_label(is_selected, &component.name).clicked() {
                                selected = Some(component.name.clone());
                            }
                        }
                    });
                if selected != self.selected {
                    self.selected = selected;
                    self.render = Render::Idle;
                }

                ui.separator();
                self.selected_ui(ui, system, render_available, &mut action);
            });

        action
    }

    fn new_component_ui(&mut self, ui: &mut egui::Ui, action: &mut Action) {
        let p = theme::palette(ui);
        let label = if self.form_open {
            "Cancel new component"
        } else {
            "New component…"
        };
        if widgets::button(ui, ButtonKind::Secondary, None, label, None).clicked() {
            self.form_open = !self.form_open;
        }
        if !self.form_open {
            return;
        }
        egui::Grid::new("design-new-component")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Name").color(p.ink_muted).size(11.0));
                ui.add(
                    egui::TextEdit::singleline(&mut self.name)
                        .hint_text("Toast")
                        .desired_width(240.0),
                );
                ui.end_row();

                ui.label(egui::RichText::new("Group").color(p.ink_muted).size(11.0));
                ui.add(egui::TextEdit::singleline(&mut self.group).desired_width(240.0));
                ui.end_row();

                ui.label(egui::RichText::new("Intent").color(p.ink_muted).size(11.0));
                ui.add(
                    egui::TextEdit::singleline(&mut self.intent)
                        .hint_text("Transient confirmation")
                        .desired_width(240.0),
                );
                ui.end_row();
            });
        let ready = !self.name.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new("Create component"))
            .on_disabled_hover_text("Name the component first")
            .clicked()
        {
            *action = Action::Create {
                name: self.name.trim().to_string(),
                group: self.group.trim().to_string(),
                intent: self.intent.trim().to_string(),
            };
            self.name.clear();
            self.intent.clear();
            self.form_open = false;
        }
    }

    fn selected_ui(
        &mut self,
        ui: &mut egui::Ui,
        system: &DesignSystem,
        render_available: bool,
        action: &mut Action,
    ) {
        let p = theme::palette(ui);
        let Some(name) = self.selected.clone() else {
            ui.label(
                egui::RichText::new("Pick a component to see its notes and preview.")
                    .color(p.ink_faint)
                    .size(11.0),
            );
            return;
        };
        let Some(component) = system.components.iter().find(|c| c.name == name) else {
            return;
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&component.name).color(p.ink).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = render_available && component.has_preview;
                let hint = if !component.has_preview {
                    "This component ships no preview.html"
                } else if !render_available {
                    "No HTML renderer on this machine"
                } else {
                    "Render preview"
                };
                if ui
                    .add_enabled(enabled, egui::Button::new("Render preview"))
                    .on_hover_text(hint)
                    .on_disabled_hover_text(hint)
                    .clicked()
                {
                    *action = Action::Render(name.clone());
                }
            });
        });

        if let Some(summary) = design::css::component_summary(component) {
            ui.label(egui::RichText::new(summary).color(p.ink_muted).size(12.0));
        }

        match &self.render {
            Render::Idle => {}
            Render::Running(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        egui::RichText::new("Rendering…")
                            .color(p.ink_muted)
                            .size(11.0),
                    );
                });
            }
            Render::Failed(err) => {
                ui.label(egui::RichText::new(err).color(p.danger).size(11.0));
            }
            Render::Ready(texture) => {
                let size = texture.size_vec2();
                let max = ui.available_width().min(620.0);
                let scale = if size.x > max { max / size.x } else { 1.0 };
                ui.add_space(4.0);
                ui.image((texture.id(), size * scale));
            }
        }
    }

    /// Poll a running render and, when it lands, decode it into a texture.
    fn poll(&mut self, ctx: &egui::Context) {
        let received = match &self.render {
            Render::Running(receiver) => Some(receiver.try_recv()),
            _ => None,
        };
        match received {
            Some(Ok(Ok(path))) => match load_texture(ctx, &path) {
                Ok(texture) => self.render = Render::Ready(texture),
                Err(err) => self.render = Render::Failed(err),
            },
            Some(Ok(Err(err))) => self.render = Render::Failed(err),
            Some(Err(TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(120));
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.render = Render::Failed("the renderer stopped".to_string());
            }
            None => {}
        }
    }
}

/// Decode a PNG into an egui texture.
fn load_texture(ctx: &egui::Context, path: &Path) -> Result<egui::TextureHandle, String> {
    let bytes =
        std::fs::read(path).map_err(|err| format!("Couldn't read {}: {err}", path.display()))?;
    let image = image::load_from_memory(&bytes)
        .map_err(|err| format!("Couldn't decode the preview: {err}"))?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Ok(ctx.load_texture(path.to_string_lossy(), color, egui::TextureOptions::LINEAR))
}
