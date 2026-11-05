//! The WelcomeScreen: what the window shows with no project and no group to
//! open.

use super::{ButtonKind, Frame, IboardApp, Located, Sense, Vec2, theme, widgets, workspace};

/// The Logo's wordmark on this screen, and the height of what is centred.
const WORDMARK: Vec2 = Vec2::new(153.0, 48.0);
const CONTENT_H: f32 = 180.0;

/// The `located` of a window with no project yet.
pub(super) fn nowhere() -> Located {
    Located {
        project_dir: workspace::virtual_root(""),
        root: workspace::virtual_root(""),
        config: None,
    }
}

impl IboardApp {
    pub(super) fn welcoming(&self) -> bool {
        self.group.is_none() && self.located.project_dir == nowhere().project_dir
    }

    pub(super) fn welcome_ui(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| {
                let middle = ui.max_rect().center().y;
                drag_strip(ui);
                if let Some(error) = self.error.clone() {
                    self.error_strip(ui, &error);
                }
                ui.add_space((middle - CONTENT_H / 2.0 - ui.cursor().top()).max(0.0));
                ui.vertical_centered(|ui| self.welcome_content(ui));
            });
        self.config_window(ui);
        widgets::settle_modals(ui.ctx());
        self.flush_pending_front(ui.ctx());
    }

    fn welcome_content(&mut self, ui: &mut egui::Ui) {
        let wordmark = if ui.visuals().dark_mode {
            egui::include_image!("../../design/components/Logo/files/iboard-wordmark-dark.svg")
        } else {
            egui::include_image!("../../design/components/Logo/files/iboard-wordmark-light.svg")
        };
        ui.add(
            egui::Image::new(wordmark)
                .fit_to_exact_size(WORDMARK)
                .alt_text("iboard"),
        );
        ui.add_space(theme::SPACE_6);
        let sentence = "Open a project with an issues/ folder, or create one in any folder.";
        ui.label(
            egui::RichText::new(sentence)
                .font(theme::ui_body())
                .color(theme::palette(ui).ink_muted),
        );
        ui.add_space(theme::SPACE_6);
        if widgets::button(ui, ButtonKind::Primary, None, "Open a folder…", None).clicked() {
            self.add_picked_projects();
        }
        ui.add_space(theme::SPACE_3);
        let create = "Create issues in a folder…";
        if widgets::button(ui, ButtonKind::Secondary, None, create, None)
            .on_hover_text("Creates issues/ with todo, ready, doing and done, then opens the board")
            .clicked()
        {
            for project in (self.folder_picker)().into_iter().rev() {
                self.initialise(project);
            }
        }
    }
}

/// The free strip under the traffic lights, which moves the window as the
/// header bar does.
fn drag_strip(ui: &mut egui::Ui) {
    let (_, strip) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), theme::BAR_H),
        Sense::click_and_drag(),
    );
    if strip.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}
