//! The Design panel, and agents drafting a component's preview.

use super::*;

impl IboardApp {
    /// Opens the Design panel on a fresh load of the project's design system.
    /// The header's Design button: the live catalog, as `idesign` opens it,
    /// where its window helper exists; the Design panel otherwise, or when the
    /// project has no design system to show.
    pub(super) fn design_button(&mut self, dark: bool) {
        self.show_design(design::catalog::available(), dark);
    }

    pub(super) fn show_design(&mut self, live: bool, dark: bool) {
        if !self.idesign {
            return;
        }
        if live && let Ok(system) = DesignSystem::discover(&self.located.project_dir) {
            let theme = if dark { "dark" } else { "light" };
            match design::catalog::show(&system, theme) {
                Ok(_) => {
                    self.session.notice =
                        Some(("Opened the design catalog".to_string(), Instant::now()));
                }
                Err(err) => self.error = Some(err),
            }
            return;
        }
        self.open_design();
    }

    pub(super) fn open_design(&mut self) {
        if !self.idesign {
            return;
        }
        match DesignSystem::discover(&self.located.project_dir) {
            Ok(system) => {
                self.session.design_system = Some(system);
                self.session.design_error = None;
            }
            Err(err) => {
                self.session.design_system = None;
                self.session.design_error = Some(err);
            }
        }
        self.session.design = Some(design_ui::Panel::default());
    }

    /// The Design panel, and the actions it asks for: scaffold a component, or
    /// render a preview on a background thread. Taking the panel out for the
    /// frame keeps the borrows simple and lets a `Close` drop it.
    pub(super) fn design_window(&mut self, ui: &mut egui::Ui) {
        let Some(mut panel) = self.session.design.take() else {
            return;
        };
        if !self.idesign {
            return;
        }
        let action = panel.show(
            ui.ctx(),
            self.session.design_system.as_ref(),
            self.session.design_error.as_deref(),
            design::render::available(),
            self.agent.configured(),
        );
        let keep = self.handle_design_action(&mut panel, action);
        if keep {
            self.session.design = Some(panel);
        }
    }

    /// Applies what the panel asked for. Returns whether the panel stays open.
    /// Factored out of `design_window` so a test can drive it without a click.
    pub(super) fn handle_design_action(
        &mut self,
        panel: &mut design_ui::Panel,
        action: design_ui::Action,
    ) -> bool {
        if !self.idesign {
            return false;
        }
        match action {
            design_ui::Action::Keep => true,
            design_ui::Action::Close => false,
            design_ui::Action::Create {
                name,
                group,
                intent,
                draft,
            } => {
                let mut created = false;
                if let Some(system) = self.session.design_system.as_mut() {
                    match system.add_component(&name, &group, Some(intent.as_str())) {
                        Ok(dir) => {
                            panel.status = Some(format!("Created {}", dir.display()));
                            created = true;
                        }
                        Err(err) => panel.status = Some(err),
                    }
                }
                if created && draft && self.agent.configured() {
                    self.start_design_draft(&name, &group, &intent);
                    panel.status = Some(format!("Drafting {name}…"));
                }
                true
            }
            design_ui::Action::Render(name) => {
                if let Some(system) = self.session.design_system.as_ref() {
                    match design::render::component_preview(system, &name) {
                        Ok(receiver) => panel.start_render(name, receiver),
                        Err(err) => panel.fail_render(&name, err),
                    }
                }
                true
            }
            design_ui::Action::OpenCatalog => {
                if let Some(system) = self.session.design_system.as_ref() {
                    match design::catalog::show(system, "light") {
                        Ok(path) => panel.status = Some(format!("Opened {}", path.display())),
                        Err(err) => panel.status = Some(err),
                    }
                }
                true
            }
        }
    }

    /// Ask the configured agent to draft a component's preview fragment. The
    /// prompt is built from the just-scaffolded component and the system.
    pub(super) fn start_design_draft(&mut self, name: &str, group: &str, intent: &str) {
        if !self.idesign {
            return;
        }
        let Some(system) = self.session.design_system.as_ref() else {
            return;
        };
        let prompt = prompts::component_prompt(system, name, group, intent, "light");
        let preview = system
            .root
            .join("components")
            .join(name)
            .join("preview.html");
        self.session.design_draft = Some(DraftJob {
            name: name.to_string(),
            job: agent::start(&self.agent, &prompt, &preview),
        });
    }

    /// Adopt a drafted component as soon as the agent answers. Runs every frame,
    /// so a draft started before the panel closed still lands.
    pub(super) fn poll_design_draft(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.session.design_draft.as_ref() else {
            return;
        };
        let Some(result) = draft.job.answer() else {
            ctx.request_repaint_after(job::POLL_EVERY);
            return;
        };
        let name = draft.name.clone();
        self.session.design_draft = None;

        let message = match result {
            Ok(reply) => {
                let html = prompts::clean_html_reply(&reply);
                if html.contains('<') {
                    self.write_drafted_component(&name, &html)
                } else {
                    format!("{name}: the agent returned no HTML")
                }
            }
            Err(err) => format!("{name}: {err}"),
        };
        if let Some(panel) = self.session.design.as_mut() {
            panel.forget_preview(&name);
            panel.status = Some(message);
        }
    }

    /// Write a drafted fragment into the component and reload the system.
    pub(super) fn write_drafted_component(&mut self, name: &str, html: &str) -> String {
        let Some(system) = self.session.design_system.as_ref() else {
            return format!("{name}: no design system");
        };
        let root = system.root.clone();
        let path = root.join("components").join(name).join("preview.html");
        if let Err(err) = std::fs::write(&path, html) {
            return format!("Couldn't write {}: {err}", path.display());
        }
        if let Ok(reloaded) = DesignSystem::load(&root) {
            self.session.design_system = Some(reloaded);
        }
        format!("Drafted {name}.")
    }
}
