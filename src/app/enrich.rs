//! Draft design: an agent's mockup of an issue, rendered beside it.

use super::*;

impl IboardApp {
    /// Start a UX enrichment: an agent drafts a static mockup of the issue,
    /// binding to the project's design system when there is one.
    pub(super) fn start_enrichment(&mut self, path: &Path) {
        if self.board.column_of(path).is_some_and(Column::is_done) {
            return;
        }
        if !self.agent.configured() {
            self.open_config();
            return;
        }
        let Some(issue) = self.board.find(path).map(|issue| issue.markdown.clone()) else {
            return;
        };
        let system = self
            .design_system
            .clone()
            .or_else(|| DesignSystem::discover(&self.located.project_dir).ok());
        let prompt = prompts::enrichment_prompt(&issue, system.as_ref(), None, "light");
        self.enrichment = Some(Enrichment {
            path: path.to_path_buf(),
            html: None,
            png: None,
            status: EnrichStatus::Running(agent::start(&self.agent, &prompt, path)),
            refine: String::new(),
            texture: None,
            note: None,
        });
    }

    /// Re-draft the artifact with the typed refinement attached.
    pub(super) fn refine_enrichment(&mut self) {
        let Some(enrichment) = self.enrichment.as_ref() else {
            return;
        };
        let instruction = enrichment.refine.trim().to_string();
        let Some(html) = enrichment.html.clone() else {
            return;
        };
        if instruction.is_empty() {
            return;
        }
        let path = enrichment.path.clone();
        let Some(issue) = self.board.find(&path).map(|issue| issue.markdown.clone()) else {
            return;
        };
        let system = self
            .design_system
            .clone()
            .or_else(|| DesignSystem::discover(&self.located.project_dir).ok());
        let prompt = prompts::enrichment_prompt(
            &issue,
            system.as_ref(),
            Some((&html, &instruction)),
            "light",
        );
        let answer = agent::start(&self.agent, &prompt, &path);
        if let Some(enrichment) = self.enrichment.as_mut() {
            enrichment.status = EnrichStatus::Running(answer);
            enrichment.refine.clear();
            enrichment.note = None;
        }
    }

    /// Adopt the agent's mockup, then its render, as they land.
    pub(super) fn poll_enrichment(&mut self, ctx: &egui::Context) {
        let Some(enrichment) = self.enrichment.as_mut() else {
            return;
        };
        let outcome = match &enrichment.status {
            EnrichStatus::Running(drafting) => drafting.answer().map(EnrichOutcome::Agent),
            EnrichStatus::Rendering(rendering) => rendering.answer().map(EnrichOutcome::Png),
            EnrichStatus::Done | EnrichStatus::Failed(_) => return,
        };
        let Some(outcome) = outcome else {
            ctx.request_repaint_after(job::POLL_EVERY);
            return;
        };

        match outcome {
            EnrichOutcome::Agent(Ok(reply)) => {
                let html = prompts::clean_html_reply(&reply);
                if !html.contains('<') {
                    enrichment.status =
                        EnrichStatus::Failed("the agent returned no HTML".to_string());
                    return;
                }
                match write_enrichment_artifact(&enrichment.path, &html) {
                    Ok(file) => {
                        let png = enrichment_dir(&enrichment.path).join("ui.png");
                        enrichment.html = Some(html);
                        enrichment.note = Some(format!("Saved {}", file.display()));
                        enrichment.status = if design::render::available() {
                            EnrichStatus::Rendering(design::render::start(&file, &png, 1100, 800))
                        } else {
                            EnrichStatus::Done
                        };
                    }
                    Err(err) => enrichment.status = EnrichStatus::Failed(err),
                }
            }
            EnrichOutcome::Agent(Err(err)) => enrichment.status = EnrichStatus::Failed(err),
            EnrichOutcome::Png(Ok(png)) => {
                enrichment.png = Some(png);
                // The file was rewritten; decode it again.
                enrichment.texture = None;
                enrichment.status = EnrichStatus::Done;
            }
            EnrichOutcome::Png(Err(err)) => {
                enrichment.note = Some(format!("preview: {err}"));
                enrichment.status = EnrichStatus::Done;
            }
        }
    }

    /// The enrichment window: the loader, the rendered mockup, and the refine
    /// field. Taking the job out for the frame keeps the borrows simple.
    pub(super) fn enrichment_window(&mut self, ui: &mut egui::Ui) {
        let Some(mut enrichment) = self.enrichment.take() else {
            return;
        };
        let ctx = ui.ctx().clone();

        if matches!(enrichment.status, EnrichStatus::Done)
            && enrichment.texture.is_none()
            && let Some(png) = enrichment.png.clone()
        {
            enrichment.texture = design_ui::load_texture(&ctx, &png).ok();
        }

        let name = enrichment
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("issue")
            .to_string();
        let drafting = matches!(enrichment.status, EnrichStatus::Running(_));
        let mut close = false;
        let mut refine = false;

        egui::Window::new(format!("Design draft — {name}"))
            .collapsible(false)
            .resizable(true)
            .default_width(760.0)
            .default_height(620.0)
            .show(&ctx, |ui| {
                let p = theme::palette(ui);
                ui.horizontal(|ui| {
                    match &enrichment.status {
                        EnrichStatus::Running(_) => {
                            ui.spinner();
                            ui.label(RichText::new("Drafting the mockup…").color(p.ink_muted));
                        }
                        EnrichStatus::Rendering(_) => {
                            ui.spinner();
                            ui.label(RichText::new("Rendering the preview…").color(p.ink_muted));
                        }
                        EnrichStatus::Failed(error) => {
                            ui.label(RichText::new(error).color(p.danger));
                        }
                        EnrichStatus::Done => {
                            ui.label(RichText::new("Mockup").color(p.ink).strong());
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::button(ui, ButtonKind::Secondary, None, "Close", None).clicked()
                        {
                            close = true;
                        }
                    });
                });

                if let Some(texture) = &enrichment.texture {
                    let size = texture.size_vec2();
                    let max = ui.available_width().min(680.0);
                    let scale = if size.x > max { max / size.x } else { 1.0 };
                    ui.add_space(4.0);
                    ui.image((texture.id(), size * scale));
                } else if let Some(note) = &enrichment.note {
                    ui.label(RichText::new(note).color(p.ink_faint).size(11.0));
                }

                if enrichment.html.is_some() && !drafting {
                    ui.separator();
                    ui.label(RichText::new("Refine").color(p.ink_muted).size(11.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut enrichment.refine)
                            .hint_text("Make the header taller, use the accent for the CTA…")
                            .desired_width(f32::INFINITY),
                    );
                    let ready = !enrichment.refine.trim().is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new("Refine"))
                        .on_disabled_hover_text("Describe what to change")
                        .clicked()
                    {
                        refine = true;
                    }
                }
            });

        if refine {
            self.enrichment = Some(enrichment);
            self.refine_enrichment();
        } else if !close {
            self.enrichment = Some(enrichment);
        }
    }
}

/// Where a ticket's drafted artifacts live: `assets/<stem>/` beside the issue
/// (the layout issue 004 settled on).
pub(super) fn enrichment_dir(issue: &Path) -> PathBuf {
    let parent = issue.parent().unwrap_or(Path::new("."));
    let stem = issue
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("issue");
    parent.join("assets").join(stem)
}

/// Write a drafted mockup to `assets/<stem>/ui.html`.
pub(super) fn write_enrichment_artifact(issue: &Path, html: &str) -> Result<PathBuf, String> {
    let dir = enrichment_dir(issue);
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("Couldn't create {}: {err}", dir.display()))?;
    let file = dir.join("ui.html");
    std::fs::write(&file, html)
        .map_err(|err| format!("Couldn't write {}: {err}", file.display()))?;
    Ok(file)
}
