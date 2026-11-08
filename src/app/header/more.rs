//! The Reader's `…` menu: Refresh, Agent log, Draft design and Archive, each
//! shown only where it applies.

use super::{ButtonKind, IboardApp, Icon, Path, Vec2, widgets};
use widgets::MenuAction;

/// The `…` menu: its width.
const MORE_MENU_WIDTH: f32 = 220.0;

/// A row of the `…` menu.
enum More {
    /// Refresh, with Auto off, and its key if it has one here.
    Refresh(&'static str),
    AgentLog,
    DraftDesign,
    Archive,
}

impl More {
    fn look(&self) -> (Option<Icon>, &'static str) {
        match self {
            Self::Refresh(_) => (Some(Icon::Refresh), "Refresh"),
            Self::AgentLog => (Some(Icon::Log), "Agent log"),
            Self::DraftDesign => (None, "Draft design"),
            Self::Archive => (Some(Icon::Archive), "Archive"),
        }
    }
}

impl IboardApp {
    /// The rows the `…` menu offers on `path`, in order.
    fn more_menu(&self, path: &Path, exists: bool) -> Vec<More> {
        let reading = self.session.reading().is_some() && exists;
        let done = self.is_done_issue(path);
        [
            (!self.prefs.auto, More::Refresh(self.refresh_key(path))),
            (reading && self.has_agent_log(path), More::AgentLog),
            (
                reading && !done && self.design_reachable(),
                More::DraftDesign,
            ),
            (reading && done, More::Archive),
        ]
        .into_iter()
        .filter_map(|(shown, row)| shown.then_some(row))
        .collect()
    }

    /// The `…` button and its menu.
    pub(super) fn more_button(&mut self, ui: &mut egui::Ui, path: &Path, exists: bool) {
        let menu = self.more_menu(path, exists);
        if menu.is_empty() {
            return;
        }
        let name = "More actions";
        let button = widgets::button(ui, ButtonKind::Ghost, Some(Icon::More), "", None)
            .on_hover_text("More actions");
        button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), name));
        let picked = egui::Popup::menu(&button)
            .width(MORE_MENU_WIDTH)
            .align(egui::RectAlign::BOTTOM_END)
            .show(|ui| {
                ui.set_min_width(MORE_MENU_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                rows(ui, &menu)
            })
            .and_then(|inner| inner.inner);
        match picked {
            Some(More::Refresh(_)) => self.refresh(),
            Some(More::AgentLog) => self.toggle_agent_log(path),
            Some(More::DraftDesign) => self.start_enrichment(path),
            Some(More::Archive) => self.archive_issue(path),
            None => {}
        }
    }
}

fn rows<'a>(ui: &mut egui::Ui, menu: &'a [More]) -> Option<&'a More> {
    let mut picked = None;
    for row in menu {
        let (icon, name) = row.look();
        let action = MenuAction {
            icon,
            name,
            key: match row {
                More::Refresh(key) => key,
                _ => "",
            },
            ..MenuAction::default()
        };
        if widgets::menu_action(ui, action).clicked() {
            picked = Some(row);
        }
    }
    picked
}
