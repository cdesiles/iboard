//! The ProjectMenu under the board header's project name: the groups, then
//! the user's projects in their own order, filtered as soon as a key is
//! typed, and a footer with Add project…, Edit groups… and Reorder. Reorder
//! is a mode of its own: rows drag, or move with `⌥↑ ⌥↓`, and Done saves.

use std::ops::Range;
use std::path::PathBuf;

use egui::text::LayoutJob;
use egui::{
    Align, Color32, CornerRadius, Event, Frame, Id, Key, Layout, Margin, Modifiers, Pos2, Rect,
    RichText, Sense, Stroke, StrokeKind, TextFormat, Vec2,
};

use iboard::theme::{self, Icon};
use iboard::widgets::{self, ButtonKind};

const WIDTH: f32 = 360.0;
const ROW_H: f32 = 38.0;
const ROW_GAP: f32 = 2.0;
/// The groups scroll after four rows.
const GROUPS_MAX_H: f32 = 4.0 * (ROW_H + ROW_GAP);
/// The projects scroll after eight rows, or sooner in a short window.
const LIST_MAX_H: f32 = 8.0 * (ROW_H + ROW_GAP);
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
    Id::new("project_menu_filter")
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
    /// filters, the arrows move, `↩` opens, `esc` clears then closes. In
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
        let list_max = (room - overhead).clamp(2.0 * (ROW_H + ROW_GAP), LIST_MAX_H);
        let mut action = None;
        let mut list_h = 0.0;
        let area = egui::Area::new(Id::new("project_menu"))
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
                            let look = Look {
                                current: entries.current_group.as_ref() == Some(&group.name),
                                active: self.active == Some(at),
                                ..Look::default()
                            };
                            let row = row(ui, &group.name, &group.count, None, look);
                            if look.active && std::mem::take(&mut self.reveal) {
                                row.scroll_to_me(Some(Align::Center));
                            }
                            if row.clicked() {
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
                        let look = Look {
                            current: entries.current_project.as_ref() == Some(&project.path),
                            active: self.active == Some(at),
                            ..Look::default()
                        };
                        let row = row(ui, &project.name, &project.detail, mark.clone(), look);
                        if look.active && std::mem::take(&mut self.reveal) {
                            row.scroll_to_me(Some(Align::Center));
                        }
                        if row.clicked() {
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
                    let mut rects = Vec::with_capacity(names.len());
                    for (at, (name, detail)) in names.iter().enumerate() {
                        let look = Look {
                            active: reorder.selected == Some(at),
                            grip: true,
                            placeholder: reorder.dragging == Some(at),
                            ..Look::default()
                        };
                        let row = row(ui, name, detail, None, look);
                        rects.push(row.rect);
                        if look.active && reveal {
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
                    lifted(ui, names[from], pointer);
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
fn step(at: Option<usize>, len: usize, down: bool) -> usize {
    match (at, down) {
        (None, true) => 0,
        (None, false) => len.saturating_sub(1),
        (Some(at), true) => (at + 1) % len.max(1),
        (Some(at), false) => (at + len.max(1) - 1) % len.max(1),
    }
}

/// Whether a project named `name` at `path` matches `query`, ignoring case:
/// `None` when it does not, else the matched bytes of the name, if any.
fn matches(name: &str, path: &str, query: &str) -> Option<Option<Range<usize>>> {
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
        .inner_margin(Margin::same(theme::SPACE_2 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A section's [`widgets::menu_head`], "GROUPS 2", in line with its rows.
fn head(ui: &mut egui::Ui, title: &str, count: &str) {
    Frame::new()
        .inner_margin(Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_2 as i8,
        ))
        .show(ui, |ui| widgets::menu_head(ui, title, count));
}

fn note_line(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    Frame::new()
        .inner_margin(Margin::symmetric(theme::SPACE_3 as i8, 0))
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
        .inner_margin(Margin::same(theme::SPACE_3 as i8))
        .corner_radius(CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius,
            se: radius,
        })
}

enum Footer {
    Add,
    EditGroups,
    Reorder,
}

/// The three actions, on the window ground.
fn footer(ui: &mut egui::Ui) -> Option<Footer> {
    let mut picked = None;
    widgets::rule(ui);
    footer_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
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

#[derive(Clone, Copy, Default)]
struct Look {
    current: bool,
    active: bool,
    grip: bool,
    placeholder: bool,
}

impl Look {
    /// The slot a dragged row left behind is faded.
    fn fade(self, color: Color32) -> Color32 {
        match self.placeholder {
            true => color.gamma_multiply(0.35),
            false => color,
        }
    }
}

/// A two-line row: the name (its matched part on `accent-soft`), the path
/// or count under it. The whole width is the hit area.
fn row(
    ui: &mut egui::Ui,
    name: &str,
    detail: &str,
    mark: Option<Range<usize>>,
    look: Look,
) -> egui::Response {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let sense = match look.grip {
        true => Sense::click_and_drag(),
        false => Sense::click(),
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_H), sense);
    let cursor = match look.grip {
        true => egui::CursorIcon::Grab,
        false => egui::CursorIcon::PointingHand,
    };
    let response = response.on_hover_cursor(cursor);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let fill = if look.active {
        p.accent_soft
    } else if response.hovered() {
        p.surface_hover
    } else {
        Color32::TRANSPARENT
    };
    let fade = |color: Color32| look.fade(color);
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), fade(fill));
    let mut inner = rect.shrink2(Vec2::new(theme::SPACE_3, theme::SPACE_2));
    if look.grip {
        let grip = Rect::from_min_size(
            Pos2::new(inner.left() - 2.0, rect.center().y - theme::ICON_SIZE / 2.0),
            Vec2::splat(theme::ICON_SIZE),
        );
        let color = if response.hovered() {
            p.ink_muted
        } else {
            p.ink_faint
        };
        theme::paint_icon(ui.painter(), Icon::Grip, grip, fade(color));
        inner.min.x = grip.right() + theme::SPACE_2;
    }
    paint_lines(ui, ui.painter(), inner, (name, detail), mark, look);
    response
}

fn paint_lines(
    ui: &egui::Ui,
    painter: &egui::Painter,
    inner: Rect,
    (name, detail): (&str, &str),
    mark: Option<Range<usize>>,
    look: Look,
) {
    let p = theme::palette(ui);
    let fade = |color: Color32| look.fade(color);
    let color = fade(if look.current { p.accent } else { p.ink });
    let plain = TextFormat::simple(theme::ui_strong(), color);
    let mut job = LayoutJob::default();
    match mark.filter(|range| name.get(range.clone()).is_some()) {
        Some(range) => {
            job.append(&name[..range.start], 0.0, plain.clone());
            let marked = TextFormat {
                background: p.accent_soft,
                ..plain.clone()
            };
            job.append(&name[range.clone()], 0.0, marked);
            job.append(&name[range.end..], 0.0, plain);
        }
        None => job.append(name, 0.0, plain),
    }
    job.wrap.max_width = inner.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let title = ui.fonts_mut(|f| f.layout_job(job));
    let detail_color = fade(p.ink_faint);
    let detail = ui.fonts_mut(|f| {
        let mut job = LayoutJob::simple_singleline(detail.to_string(), theme::meta(), detail_color);
        job.wrap.max_width = inner.width();
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        f.layout_job(job)
    });
    let gap = theme::SPACE_1;
    let top = inner.center().y - (title.size().y + detail.size().y + gap) / 2.0;
    painter.galley(Pos2::new(inner.min.x, top), title.clone(), color);
    painter.galley(
        Pos2::new(inner.min.x, top + title.size().y + gap),
        detail,
        detail_color,
    );
}

/// The row being dragged, raised under the pointer like a dragged card.
fn lifted(ui: &egui::Ui, (name, detail): (&str, &str), pointer: Pos2) {
    let p = theme::palette(ui);
    let width = ui.max_rect().width();
    let rect = Rect::from_min_size(
        Pos2::new(ui.max_rect().left(), pointer.y - ROW_H / 2.0),
        Vec2::new(width, ROW_H),
    );
    let layer = egui::LayerId::new(egui::Order::Tooltip, Id::new("project_menu_lifted"));
    let painter = ui.ctx().layer_painter(layer);
    let shadow = ui.visuals().popup_shadow;
    painter.add(shadow.as_shape(rect, CornerRadius::same(theme::RADIUS_SM)));
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS_SM),
        p.surface,
        Stroke::new(1.0, p.line_strong),
        StrokeKind::Inside,
    );
    let mut inner = rect.shrink2(Vec2::new(theme::SPACE_3, theme::SPACE_2));
    let grip = Rect::from_min_size(
        Pos2::new(inner.left() - 2.0, rect.center().y - theme::ICON_SIZE / 2.0),
        Vec2::splat(theme::ICON_SIZE),
    );
    theme::paint_icon(&painter, Icon::Grip, grip, p.ink_muted);
    inner.min.x = grip.right() + theme::SPACE_2;
    paint_lines(ui, &painter, inner, (name, detail), None, Look::default());
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
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
    fn the_arrows_go_round_the_list() {
        assert_eq!(step(None, 3, true), 0);
        assert_eq!(step(None, 3, false), 2);
        assert_eq!(step(Some(2), 3, true), 0);
        assert_eq!(step(Some(0), 3, false), 2);
    }
}
