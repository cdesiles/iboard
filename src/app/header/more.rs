//! The Reader's `…` menu: Move to…, Refresh, Agent log, Draft design and
//! Archive, each shown only where it applies.

use super::{ButtonKind, IboardApp, Icon, Path, PathBuf, Vec2, widgets};
use widgets::MenuAction;

/// The `…` menu, and the Move to… submenu in it: column names only.
const MORE_MENU_WIDTH: f32 = 220.0;
const MOVE_MENU_WIDTH: f32 = 180.0;

/// A row of the `…` menu.
enum More {
    /// The columns to send the issue to.
    Move(Vec<(String, PathBuf)>),
    /// Refresh, with Auto off, and its key if it has one here.
    Refresh(&'static str),
    AgentLog,
    DraftDesign,
    Archive,
}

impl More {
    fn look(&self) -> (Option<Icon>, &'static str) {
        match self {
            Self::Move(_) => (Some(Icon::ArrowRight), "Move to…"),
            Self::Refresh(_) => (Some(Icon::Refresh), "Refresh"),
            Self::AgentLog => (Some(Icon::Log), "Agent log"),
            Self::DraftDesign => (None, "Draft design"),
            Self::Archive => (Some(Icon::Archive), "Archive"),
        }
    }
}

/// What a click in the `…` menu asks for.
enum Pick<'a> {
    Move(PathBuf),
    Row(&'a More),
}

impl IboardApp {
    /// The rows the `…` menu offers on `path`, in order.
    fn more_menu(&self, path: &Path, exists: bool) -> Vec<More> {
        let reading = self.session.reading().is_some() && exists;
        let done = self.is_done_issue(path);
        let here = self.board.column_of(path).map(|column| column.dir.clone());
        let columns: Vec<_> = self
            .board
            .columns
            .iter()
            .filter(|column| exists && Some(&column.dir) != here.as_ref())
            .map(|column| (column.name.clone(), column.dir.clone()))
            .collect();
        [
            (!columns.is_empty(), More::Move(columns)),
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
            .on_hover_text("Move to…");
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
            Some(Pick::Move(dir)) => self.send_to(path, &dir),
            Some(Pick::Row(More::Refresh(_))) => self.refresh(),
            Some(Pick::Row(More::AgentLog)) => self.toggle_agent_log(path),
            Some(Pick::Row(More::DraftDesign)) => self.start_enrichment(path),
            Some(Pick::Row(More::Archive)) => self.archive_issue(path),
            Some(Pick::Row(More::Move(_))) | None => {}
        }
    }
}

fn rows<'a>(ui: &mut egui::Ui, menu: &'a [More]) -> Option<Pick<'a>> {
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
            submenu: matches!(row, More::Move(_)),
            ..MenuAction::default()
        };
        let response = widgets::menu_action(ui, action);
        if let More::Move(columns) = row {
            picked = move_menu(ui, &response, columns).map(Pick::Move).or(picked);
        } else if response.clicked() {
            picked = Some(Pick::Row(row));
        }
    }
    picked
}

/// Move to…'s submenu: the other columns.
fn move_menu(
    ui: &egui::Ui,
    row: &egui::Response,
    columns: &[(String, PathBuf)],
) -> Option<PathBuf> {
    let picked = egui::containers::menu::SubMenu::new().show(ui, row, |ui| {
        ui.set_width(MOVE_MENU_WIDTH);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let mut picked = None;
        for (name, dir) in columns {
            let action = MenuAction {
                name,
                ..MenuAction::default()
            };
            if widgets::menu_action(ui, action).clicked() {
                picked = Some(dir.clone());
            }
        }
        picked
    });
    picked.and_then(|inner| inner.inner)
}
