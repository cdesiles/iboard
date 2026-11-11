//! The ProjectMenu under the board header's project name: the groups, then
//! the user's projects in their own order, filtered as soon as a key is
//! typed, and a footer with Stats, Add project…, Edit groups… and Reorder. Reorder
//! is a mode of its own: rows drag, or move with `⌥↑ ⌥↓`, and Done saves.

use std::ops::Range;
use std::path::PathBuf;

use egui::{
    Align, CornerRadius, Event, Frame, Id, Key, Layout, Modifiers, Pos2, Rect, RichText, Stroke,
    Vec2,
};

use crate::shell::theme::{self, Icon};
use crate::shell::widgets::{self, ButtonKind, Drag, MENU_ROW_H, RowLook};

const WIDTH: f32 = 360.0;
const ROW_GAP: f32 = 0.0;
/// The groups scroll after four rows.
const GROUPS_MAX_H: f32 = 4.0 * (MENU_ROW_H + ROW_GAP);
/// The projects scroll after eight rows, or sooner in a short window.
const LIST_MAX_H: f32 = 8.0 * (MENU_ROW_H + ROW_GAP);
/// Under this many projects the filter field hides until a key is typed.
const FILTER_FROM: usize = 9;
/// How close to the list's edge a dragged row scrolls it.
const AUTO_SCROLL_EDGE: f32 = 24.0;

pub struct GroupRow {
    pub name: String,
    pub count: String,
}

pub struct ProjectRow {
    pub path: PathBuf,
    pub name: String,
    pub detail: String,
}

/// What the menu lists this frame, and which board is on screen.
pub struct Entries {
    pub groups: Vec<GroupRow>,
    pub projects: Vec<ProjectRow>,
    pub current_group: Option<String>,
    pub current_project: Option<PathBuf>,
}

pub enum Pick {
    Group(String),
    Project(PathBuf),
}

pub enum Action {
    Open(Pick),
    Stats,
    Add,
    EditGroups,
    SaveOrder(Vec<PathBuf>),
    Close,
}

/// One line of the list the arrows walk through.
#[derive(Clone, PartialEq, Debug)]
enum Item {
    Group(usize),
    /// A project, with the part of its name the filter matched.
    Project(usize, Option<Range<usize>>),
}

struct Reorder {
    order: Vec<PathBuf>,
    selected: Option<usize>,
    dragging: Option<usize>,
}

#[derive(Default)]
pub struct Menu {
    pub filter: String,
    active: Option<usize>,
    reorder: Option<Reorder>,
    /// A key was typed: the field shows even on a short list.
    typed: bool,
    focus_field: bool,
    reveal: bool,
    /// The menu's height minus its project list's, last frame.
    overhead: Option<f32>,
}

fn field_id() -> Id {
    Id::unique("project_menu_filter")
}

impl Menu {
    pub fn new(entries: &Entries) -> Self {
        let mut menu = Self {
            reveal: true,
            ..Self::default()
        };
        menu.active = menu.current_index(entries);
        menu
    }

    fn items(&self, entries: &Entries) -> Vec<Item> {
        if self.filter.is_empty() {
            let groups = (0..entries.groups.len()).map(Item::Group);
            let projects = (0..entries.projects.len()).map(|at| Item::Project(at, None));
            return groups.chain(projects).collect();
        }
        entries
            .projects
            .iter()
            .enumerate()
            .filter_map(|(at, row)| {
                let mark = matches(&row.name, &row.detail, &self.filter)?;
                Some(Item::Project(at, mark))
            })
            .collect()
    }

    fn current_index(&self, entries: &Entries) -> Option<usize> {
        self.items(entries).iter().position(|item| match item {
            Item::Group(at) => entries.current_group.as_ref() == Some(&entries.groups[*at].name),
            Item::Project(at, _) => {
                entries.current_project.as_ref() == Some(&entries.projects[*at].path)
            }
        })
    }

    fn pick(&self, entries: &Entries, item: &Item) -> Pick {
        match item {
            Item::Group(at) => Pick::Group(entries.groups[*at].name.clone()),
            Item::Project(at, _) => Pick::Project(entries.projects[*at].path.clone()),
        }
    }

    /// The menu's keys, taken before anything else sees them: typing
    /// filters, the arrows move, `↩` opens, `⌘I` is its Stats row, `esc`
    /// clears then closes. In
    /// reorder mode `⌥↑ ⌥↓` move the selected project and `↩` saves.
    pub fn keys(&mut self, ctx: &egui::Context, entries: &Entries) -> Option<Action> {
        let take =
            |modifiers: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(modifiers, key));
        if let Some(reorder) = self.reorder.as_mut() {
            if take(Modifiers::NONE, Key::Escape) {
                self.reorder = None;
                return None;
            }
            if take(Modifiers::NONE, Key::Enter) {
                let order = std::mem::take(&mut reorder.order);
                self.reorder = None;
                return Some(Action::SaveOrder(order));
            }
            let edge = Modifiers::ALT | Modifiers::SHIFT;
            if take(edge, Key::ArrowUp) {
                reorder.shift(Shift::Top);
            } else if take(edge, Key::ArrowDown) {
                reorder.shift(Shift::Bottom);
            } else if take(Modifiers::ALT, Key::ArrowUp) {
                reorder.shift(Shift::Up);
            } else if take(Modifiers::ALT, Key::ArrowDown) {
                reorder.shift(Shift::Down);
            } else if take(Modifiers::NONE, Key::ArrowUp) {
                reorder.selected = Some(step(reorder.selected, reorder.order.len(), false));
            } else if take(Modifiers::NONE, Key::ArrowDown) {
                reorder.selected = Some(step(reorder.selected, reorder.order.len(), true));
            } else {
                return None;
            }
            self.reveal = true;
            return None;
        }
        if take(Modifiers::NONE, Key::Escape) {
            if self.filter.is_empty() {
                return Some(Action::Close);
            }
            self.filter.clear();
            self.active = self.current_index(entries);
            self.reveal = true;
            return None;
        }
        if take(Modifiers::COMMAND, Key::I) {
            return Some(Action::Stats);
        }
        let items = self.items(entries);
        if take(Modifiers::NONE, Key::Enter) {
            let item = self.active.and_then(|at| items.get(at))?;
            return Some(Action::Open(self.pick(entries, item)));
        }
        for (key, down) in [(Key::ArrowDown, true), (Key::ArrowUp, false)] {
            if take(Modifiers::NONE, key) && !items.is_empty() {
                self.active = Some(step(self.active, items.len(), down));
                self.reveal = true;
            }
        }
        if !ctx.memory(|m| m.has_focus(field_id())) {
            let typed = ctx.input_mut(|i| {
                let mut typed = String::new();
                i.events.retain(|event| match event {
                    Event::Text(text) => {
                        typed.push_str(text);
                        false
                    }
                    _ => true,
                });
                typed
            });
            if !typed.is_empty() {
                self.filter.push_str(&typed);
                self.typed = true;
                self.focus_field = true;
                self.active = Some(0);
                self.reveal = true;
            }
        }
        None
    }

    /// Draws the menu under `anchor`. A click outside it closes it.
    pub fn show(&mut self, ctx: &egui::Context, anchor: Rect, entries: &Entries) -> Option<Action> {
        let top = anchor.bottom() + theme::SPACE_2;
        let room = ctx.content_rect().bottom() - top - theme::SPACE_5;
        let overhead = self.overhead.unwrap_or(260.0);
        let list_max = (room - overhead).clamp(2.0 * (MENU_ROW_H + ROW_GAP), LIST_MAX_H);
        let mut action = None;
        let mut list_h = 0.0;
        let area = egui::Area::new(Id::unique("project_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(anchor.left(), top))
            .show(ctx, |ui| {
                widgets::popover_frame(ui).show(ui, |ui| {
                    ui.set_width(WIDTH);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    if self.reorder.is_some() {
                        list_h = self.reorder_body(ui, entries, list_max);
                        self.reorder_footer(ui, &mut action);
                    } else {
                        list_h = self.body(ui, entries, list_max, &mut action);
                        match footer(ui) {
                            Some(Footer::Stats) => action = Some(Action::Stats),
                            Some(Footer::Add) => action = Some(Action::Add),
                            Some(Footer::EditGroups) => action = Some(Action::EditGroups),
                            Some(Footer::Reorder) => self.start_reorder(entries),
                            None => {}
                        }
                    }
                });
            });
        self.overhead = Some(area.response.rect.height() - list_h);
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|at| !area.response.rect.contains(at) && !anchor.contains(at))
        });
        if clicked_outside && action.is_none() {
            return Some(Action::Close);
        }
        action
    }

    fn start_reorder(&mut self, entries: &Entries) {
        let order: Vec<PathBuf> = entries
            .projects
            .iter()
            .map(|row| row.path.clone())
            .collect();
        let selected = entries
            .current_project
            .as_ref()
            .and_then(|current| order.iter().position(|path| path == current));
        self.filter.clear();
        self.reveal = true;
        self.reorder = Some(Reorder {
            order,
            selected,
            dragging: None,
        });
    }

    /// Groups, then the filtered projects. Returns the list's height.
    fn body(
        &mut self,
        ui: &mut egui::Ui,
        entries: &Entries,
        list_max: f32,
        action: &mut Option<Action>,
    ) -> f32 {
        let filtering = !self.filter.is_empty();
        let mut items = self.items(entries);
        let groups_shown = !filtering && !entries.groups.is_empty();
        if groups_shown {
            section(ui, |ui| {
                head(ui, "Groups", &entries.groups.len().to_string());
                egui::ScrollArea::vertical()
                    .id_salt("project_menu_groups")
                    .max_height(GROUPS_MAX_H)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = ROW_GAP;
                        for (at, group) in entries.groups.iter().enumerate() {
                            let look = RowLook {
                                current: entries.current_group.as_ref() == Some(&group.name),
                                ..RowLook::default()
                            };
                            if self.item_row(ui, at, (&group.name, &group.count), look) {
                                *action = Some(Action::Open(Pick::Group(group.name.clone())));
                            }
                        }
                    });
            });
            widgets::rule(ui);
        }
        let first_project = items
            .iter()
            .position(|item| matches!(item, Item::Project(..)))
            .unwrap_or(items.len());
        let mut list_h = 0.0;
        section(ui, |ui| {
            let count = match filtering {
                true => format!("{}/{}", items.len(), entries.projects.len()),
                false => entries.projects.len().to_string(),
            };
            head(ui, "Projects", &count);
            if entries.projects.len() >= FILTER_FROM || self.typed || filtering {
                let field = widgets::text_entry(
                    ui,
                    widgets::field_edit(ui, &mut self.filter, "Filter projects", theme::ui_body())
                        .id(field_id())
                        .desired_width(f32::INFINITY),
                );
                if std::mem::take(&mut self.focus_field) {
                    field.request_focus();
                }
                if field.changed() {
                    items = self.items(entries);
                    self.active = match self.filter.is_empty() {
                        true => self.current_index(entries),
                        false => Some(0),
                    };
                    self.reveal = true;
                }
                ui.add_space(theme::SPACE_2);
            }
            let filtering = !self.filter.is_empty();
            let shown = egui::ScrollArea::vertical()
                .id_salt("project_menu_projects")
                .max_height(list_max)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = ROW_GAP;
                    for (at, item) in items
                        .iter()
                        .enumerate()
                        .skip(first_project.min(items.len()))
                    {
                        let Item::Project(index, mark) = item else {
                            continue;
                        };
                        let project = &entries.projects[*index];
                        let look = RowLook {
                            current: entries.current_project.as_ref() == Some(&project.path),
                            mark: mark.clone(),
                            ..RowLook::default()
                        };
                        if self.item_row(ui, at, (&project.name, &project.detail), look) {
                            *action = Some(Action::Open(Pick::Project(project.path.clone())));
                        }
                    }
                });
            list_h = shown.inner_rect.height();
            if filtering {
                let note = match self.active.and_then(|at| items.get(at)) {
                    Some(Item::Project(index, _)) => format!(
                        "↩ opens {} · esc clears the filter",
                        entries.projects[*index].name
                    ),
                    _ if items.is_empty() => format!(
                        "No project matches {}. Add project… to add one.",
                        self.filter
                    ),
                    _ => "esc clears the filter".to_string(),
                };
                ui.add_space(theme::SPACE_2);
                note_line(ui, &note);
            }
        });
        list_h
    }

    /// The row of the `at`th item, the active one scrolled into view when the
    /// menu asks. True when clicked.
    fn item_row(
        &mut self,
        ui: &mut egui::Ui,
        at: usize,
        (title, detail): (&str, &str),
        look: RowLook,
    ) -> bool {
        let active = self.active == Some(at);
        let width = ui.available_width();
        let row = widgets::menu_row(ui, width, title, detail, RowLook { active, ..look });
        if active && std::mem::take(&mut self.reveal) {
            row.scroll_to_me(Some(Align::Center));
        }
        row.clicked()
    }

    /// Reorder mode: every project with its grip, dragged or moved with the
    /// keys. Returns the list's height.
    fn reorder_body(&mut self, ui: &mut egui::Ui, entries: &Entries, list_max: f32) -> f32 {
        let p = theme::palette(ui);
        let Some(reorder) = self.reorder.as_mut() else {
            return 0.0;
        };
        let names: Vec<(&str, &str)> = reorder
            .order
            .iter()
            .map(|path| {
                entries
                    .projects
                    .iter()
                    .find(|row| row.path == *path)
                    .map_or(("", ""), |row| (row.name.as_str(), row.detail.as_str()))
            })
            .collect();
        let mut list_h = 0.0;
        let mut dropped = None;
        let reveal = std::mem::take(&mut self.reveal);
        section(ui, |ui| {
            head(ui, "Reorder projects", &reorder.order.len().to_string());
            note_line(
                ui,
                "Drag a project, or select it and press ⌥↑ ⌥↓. The board does not change.",
            );
            ui.add_space(theme::SPACE_2);
            let shown = egui::ScrollArea::vertical()
                .id_salt("project_menu_reorder")
                .max_height(list_max)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = ROW_GAP;
                    let width = ui.available_width();
                    let mut rects = Vec::with_capacity(names.len());
                    for (at, (name, detail)) in names.iter().enumerate() {
                        let active = reorder.selected == Some(at);
                        let drag = match reorder.dragging == Some(at) {
                            true => Drag::Placeholder,
                            false => Drag::Grip,
                        };
                        let look = RowLook {
                            active,
                            drag: Some(drag),
                            ..RowLook::default()
                        };
                        let row = widgets::menu_row(ui, width, name, detail, look);
                        rects.push(row.rect);
                        if active && reveal {
                            row.scroll_to_me(Some(Align::Center));
                        }
                        if row.clicked() || row.drag_started() {
                            reorder.selected = Some(at);
                        }
                        if row.drag_started() {
                            reorder.dragging = Some(at);
                        }
                    }
                    let Some(from) = reorder.dragging else {
                        return;
                    };
                    let Some(pointer) = ui.ctx().pointer_interact_pos() else {
                        return;
                    };
                    let slot = rects
                        .iter()
                        .filter(|rect| rect.center().y < pointer.y)
                        .count();
                    let line_y = match rects.get(slot) {
                        Some(rect) => rect.top() - ROW_GAP / 2.0,
                        None => rects.last().map_or(pointer.y, |rect| rect.bottom() + 1.0),
                    };
                    let x = ui.max_rect().x_range();
                    ui.painter().hline(x, line_y, Stroke::new(2.0, p.accent));
                    widgets::menu_row_lifted(ui, names[from], pointer);
                    let view = ui.clip_rect();
                    let scroll = if pointer.y < view.top() + AUTO_SCROLL_EDGE {
                        6.0
                    } else if pointer.y > view.bottom() - AUTO_SCROLL_EDGE {
                        -6.0
                    } else {
                        0.0
                    };
                    if scroll != 0.0 {
                        ui.scroll_with_delta(Vec2::new(0.0, scroll));
                        ui.ctx().request_repaint();
                    }
                    if ui.input(|i| i.pointer.any_released()) {
                        dropped = Some((from, slot));
                    }
                });
            list_h = shown.inner_rect.height();
        });
        if let Some((from, slot)) = dropped {
            reorder.dragging = None;
            let to = if slot > from { slot - 1 } else { slot };
            reorder.selected = Some(move_item(&mut reorder.order, from, to));
        }
        list_h
    }

    fn reorder_footer(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        let p = theme::palette(ui);
        let mut done = false;
        let mut cancel = false;
        footer_frame(ui).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                ui.label(
                    RichText::new("Saved as your project order")
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    done =
                        widgets::button(ui, ButtonKind::Primary, None, "Done", Some("↩")).clicked();
                    cancel =
                        widgets::button(ui, ButtonKind::Secondary, None, "Cancel", Some("esc"))
                            .clicked();
                });
            });
        });
        if cancel {
            self.reorder = None;
        } else if done && let Some(reorder) = self.reorder.take() {
            *action = Some(Action::SaveOrder(reorder.order));
        }
    }
}

enum Shift {
    Up,
    Down,
    Top,
    Bottom,
}

impl Reorder {
    fn shift(&mut self, shift: Shift) {
        let Some(from) = self.selected else {
            return;
        };
        let last = self.order.len().saturating_sub(1);
        let to = match shift {
            Shift::Up => from.saturating_sub(1),
            Shift::Down => (from + 1).min(last),
            Shift::Top => 0,
            Shift::Bottom => last,
        };
        self.selected = Some(move_item(&mut self.order, from, to));
    }
}

/// Moves `list[from]` to `to`, the others closing up. Returns where it is.
fn move_item<T>(list: &mut Vec<T>, from: usize, to: usize) -> usize {
    if from >= list.len() {
        return from;
    }
    let item = list.remove(from);
    let to = to.min(list.len());
    list.insert(to, item);
    to
}

/// The next (or previous) line from `at`, round the end.
pub fn step(at: Option<usize>, len: usize, down: bool) -> usize {
    match (at, down) {
        (None, true) => 0,
        (None, false) => len.saturating_sub(1),
        (Some(at), true) => (at + 1) % len.max(1),
        (Some(at), false) => (at + len.max(1) - 1) % len.max(1),
    }
}

/// Whether a project named `name` at `path` matches `query`, ignoring case:
/// `None` when it does not, else the matched bytes of the name, if any.
pub fn matches(name: &str, path: &str, query: &str) -> Option<Option<Range<usize>>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(None);
    }
    let lower = name.to_lowercase();
    if lower.len() == name.len()
        && let Some(start) = lower.find(&query)
    {
        return Some(Some(start..start + query.len()));
    }
    if lower.contains(&query) || path.to_lowercase().contains(&query) {
        return Some(None);
    }
    None
}

fn section<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::new()
        .inner_margin(theme::SPACE_2)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A section's [`widgets::menu_head`], "GROUPS 2", in line with its rows.
fn head(ui: &mut egui::Ui, title: &str, count: &str) {
    Frame::new()
        .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
        .show(ui, |ui| widgets::menu_head(ui, title, count));
}

fn note_line(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    Frame::new()
        .inner_margin(Vec2::new(theme::SPACE_3, 0.0))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(text).font(theme::meta()).color(p.ink_faint)).wrap(),
            );
        });
}

fn footer_frame(ui: &egui::Ui) -> Frame {
    let p = theme::palette(ui);
    let radius = theme::RADIUS_MD;
    Frame::new()
        .fill(p.bg)
        .inner_margin(theme::SPACE_3)
        .corner_radius(CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius,
            se: radius,
        })
}

/// Between the footer's Stats row and its buttons.
const FOOTER_GAP: f32 = 6.0;

enum Footer {
    Stats,
    Add,
    EditGroups,
    Reorder,
}

/// The Stats row over the three actions, on the window ground.
fn footer(ui: &mut egui::Ui) -> Option<Footer> {
    let mut picked = None;
    widgets::rule(ui);
    footer_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let stats = widgets::wide_button(
            ui,
            ButtonKind::Secondary,
            Some(Icon::Bars),
            "Stats",
            Some("⌘I"),
        )
        .on_hover_text("Project stats: what the commits say, and the Agenda  ⌘I");
        if stats.clicked() {
            picked = Some(Footer::Stats);
        }
        ui.add_space(FOOTER_GAP);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            let add = widgets::button(
                ui,
                ButtonKind::Secondary,
                Some(Icon::Plus),
                "Add project…",
                None,
            )
            .on_hover_text("Choose a project containing an issues/ folder");
            if add.clicked() {
                picked = Some(Footer::Add);
            }
            let groups = widgets::button(
                ui,
                ButtonKind::Secondary,
                Some(Icon::Layers),
                "Edit groups…",
                None,
            );
            if groups.clicked() {
                picked = Some(Footer::EditGroups);
            }
            let reorder =
                widgets::button(ui, ButtonKind::Secondary, Some(Icon::Grip), "Reorder", None)
                    .on_hover_text("Put your projects in your own order");
            if reorder.clicked() {
                picked = Some(Footer::Reorder);
            }
        });
    });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_matches_the_name_or_the_path_ignoring_case() {
        assert_eq!(
            matches("moon-cull", "~/perso/moon-cull", "MOON"),
            Some(Some(0..4))
        );
        assert_eq!(matches("moonshot-cli", "~/oss/x", "shot"), Some(Some(4..8)));
        assert_eq!(matches("site", "~/code/client/site", "client"), Some(None));
        assert_eq!(matches("site", "~/code/site", "moon"), None);
        assert_eq!(matches("site", "~/code/site", "  "), Some(None));
    }

    #[test]
    fn a_project_moves_one_place_or_to_an_edge() {
        let mut reorder = Reorder {
            order: ["a", "b", "c", "d"].map(PathBuf::from).to_vec(),
            selected: Some(1),
            dragging: None,
        };
        let names = |reorder: &Reorder| -> String {
            reorder
                .order
                .iter()
                .map(|path| path.display().to_string())
                .collect()
        };
        reorder.shift(Shift::Up);
        assert_eq!(
            (names(&reorder), reorder.selected),
            ("bacd".into(), Some(0))
        );
        reorder.shift(Shift::Up);
        assert_eq!(names(&reorder), "bacd", "already on top");
        reorder.shift(Shift::Bottom);
        assert_eq!(
            (names(&reorder), reorder.selected),
            ("acdb".into(), Some(3))
        );
        reorder.shift(Shift::Down);
        assert_eq!(names(&reorder), "acdb", "already at the bottom");
        reorder.selected = Some(2);
        reorder.shift(Shift::Top);
        assert_eq!(names(&reorder), "dacb");
    }

    #[test]
    fn the_lists_scroll_at_their_specs_heights() {
        assert_eq!(GROUPS_MAX_H, 168.0);
        assert_eq!(LIST_MAX_H, 336.0);
    }

    #[test]
    fn the_arrows_go_round_the_list() {
        assert_eq!(step(None, 3, true), 0);
        assert_eq!(step(None, 3, false), 2);
        assert_eq!(step(Some(2), 3, true), 0);
        assert_eq!(step(Some(0), 3, false), 2);
    }
}
