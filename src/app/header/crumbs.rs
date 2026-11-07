//! The header's breadcrumb: the project menu on the board, an issue's trail
//! back to it elsewhere.

use super::{
    ButtonKind, IboardApp, Icon, Path, Pos2, RichText, Screen, Sense, Stroke, Vec2, column_status,
    file_name, theme, widgets,
};

impl IboardApp {
    /// Board: the project name. Reader and editor: a `‹ Board` button, then
    /// `project / Column`, each a link, and the issue's title once the page's
    /// own has scrolled away (always in the editor).
    pub(super) fn breadcrumb(&mut self, ui: &mut egui::Ui) {
        if self.session.stats.is_some() {
            self.stats_crumbs(ui);
            return;
        }
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let project = self.board.name();
        match self.session.screen.clone() {
            Screen::Board => {
                let open = self.project_menu.is_some();
                let response = widgets::project_trigger(ui, &project, open)
                    .on_hover_text("Switch project  ⌃⇥");
                if response.clicked() {
                    self.toggle_project_menu(ui.ctx());
                }
                if self.project_menu.is_some() {
                    self.show_project_menu(ui.ctx(), response.rect);
                }
            }
            Screen::Detail(path, _) => {
                let titled = self.session.editing().is_some()
                    || self.reader.title_away.as_deref() == Some(path.as_path());
                self.detail_breadcrumb(ui, &path, &project, titled);
            }
            Screen::Diff => {
                if let Some(path) = self.session.diff.as_ref().map(|view| view.issue.clone()) {
                    self.detail_breadcrumb(ui, &path, &project, true);
                    ui.add_space(widgets::CRUMB_PAD);
                    crumb_tail(ui, "Diff");
                }
            }
        }
    }

    /// An issue's trail: Board, then `project / ○ Column`, each crumb leading
    /// back to the board, then the title when `titled`.
    fn detail_breadcrumb(&mut self, ui: &mut egui::Ui, path: &Path, project: &str, titled: bool) {
        let p = theme::palette(ui);
        // The visible way out, for whoever does not know the shortcuts.
        let hint = if self.session.editing().is_some() {
            "Back to the board, asking if unsaved  esc"
        } else {
            "Back to the board  esc"
        };
        let mut back =
            widgets::button(ui, ButtonKind::Back, Some(Icon::Back), "Board", Some("esc"))
                .on_hover_text(hint)
                .clicked();
        // A 16pt `line` divider between the button and the trail.
        let (divider, _) = ui.allocate_exact_size(Vec2::new(1.0, theme::CONTROL_H), Sense::hover());
        ui.painter().line_segment(
            [
                Pos2::new(divider.center().x, divider.center().y - 8.0),
                Pos2::new(divider.center().x, divider.center().y + 8.0),
            ],
            Stroke::new(1.0, p.line),
        );
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        let column = self
            .board
            .column_of(path)
            .map(|column| (column.name.clone(), column_status(column)));
        let title = self
            .board
            .find(path)
            .map(|t| t.title.clone())
            .unwrap_or_else(|| file_name(path));
        back |= widgets::crumb(ui, project, theme::ui_body()).clicked();
        if let Some((column, mark)) = column {
            separator(ui);
            back |= widgets::crumb_marked(ui, &column, theme::ui_body(), Some(mark)).clicked();
        }
        if titled {
            crumb_tail(ui, &title);
        }
        if back {
            self.back_to_board();
        }
    }
}

fn separator(ui: &mut egui::Ui) {
    ui.label(RichText::new("/").color(theme::palette(ui).ink_faint));
}

/// The trail's last crumb, past a `/`: `ui-strong` in `ink`, truncated.
fn crumb_tail(ui: &mut egui::Ui, text: &str) {
    separator(ui);
    ui.add_space(widgets::CRUMB_PAD);
    let text = RichText::new(text)
        .font(theme::ui_strong())
        .color(theme::palette(ui).ink);
    ui.add(egui::Label::new(text).truncate());
}
