//! GroupsDialog: creates and edits the project groups, a friendlier way to
//! edit the `[[workspace.group]]` tables of the user configuration (see
//! `design/components/GroupsDialog`). It works on a [`Draft`]; the app
//! writes the groups [`Action::Save`] hands back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Align, CornerRadius, Frame, Id, Key, Layout, Modifiers, Rect, RichText, Stroke, Vec2};

use crate::shell::project_menu::{matches, step};
use crate::shell::theme::{self, Icon};
use crate::shell::widgets::{self, ButtonKind, FooterClick, RowLook, Tone};
use crate::workspace::config::{self, has_issues_dir};
use crate::workspace::draft::{Added, Draft};
use crate::workspace::{Group, Workspace};

/// The body scrolls past this height.
/// The Add project menu filters once it lists more projects than this.
const ADD_FILTER_FROM: usize = 8;
/// The Add project menu's list scrolls after eight two-line rows.
const ADD_LIST_MAX_H: f32 = 8.0 * 40.0;

/// How long a row already in the group flashes.
const FLASH_SECONDS: f64 = 1.2;

/// How strong the flash is with `left` of it to go, from 1 to 0: with reduced
/// motion it holds steady, then goes at once.
fn flash_strength(left: f64, reduced: bool) -> f32 {
    if reduced { 1.0 } else { left as f32 }
}

const HELP: &str = "Each group is a board of its own projects. Removing a project here never touches its issues/ folder.";

/// What the dialog asks of the app this frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Keep,
    Close,
    /// Write these groups, expecting the file as it was read.
    Save(Vec<Group>),
    /// Write these groups over a file that changed meanwhile.
    Overwrite(Vec<Group>),
    /// Drop the draft and read the groups again.
    Reload,
}

/// The name field of the `at`th group.
pub fn name_id(at: usize) -> Id {
    Id::unique(("group-name", at))
}

/// The `index`th project row of the `at`th group.
pub fn row_id(at: usize, index: usize) -> Id {
    Id::unique(("group-project", at, index))
}

/// A project the Add project menu lists, with what its filter matched.
struct AddRow {
    project: PathBuf,
    name: String,
    path: String,
    mark: Option<std::ops::Range<usize>>,
}

pub struct Dialog {
    pub draft: Draft,
    /// The configuration file as it was read, to refuse a stale save.
    pub fingerprint: u64,
    /// Why the last save failed, at the end of the body.
    pub save_error: Option<String>,
    /// The last save found the file changed on disk.
    pub conflict: bool,
    /// The group whose Add project menu is open.
    adding: Option<usize>,
    /// What the Add project menu's filter holds.
    add_filter: String,
    /// The Add project menu's keyboard row, among the projects it lists.
    add_active: Option<usize>,
    /// The keyboard row moved: it scrolls into view.
    add_reveal: bool,
    /// The group about to be deleted, asking first.
    deleting: Option<usize>,
    /// `esc` with unsaved changes: asking whether to drop them.
    discarding: bool,
    /// The group whose name takes the focus next frame.
    focus_name: Option<usize>,
    /// The group and project row that take the focus next frame.
    focus_row: Option<(usize, usize)>,
    /// A folder picked again, in its group, and when: its row flashes.
    pub flash: Option<(usize, PathBuf, f64)>,
    /// Where each group's block was drawn, to tell which one has the focus.
    blocks: Vec<Rect>,
    /// A save went out: an error it brings back is scrolled into view.
    reveal_error: bool,
    /// The user's projects, as they were when the dialog opened.
    projects: Vec<PathBuf>,
    /// Whether each project shown has its `issues/` folder, looked up once.
    has_issues: HashMap<PathBuf, bool>,
    home: Option<PathBuf>,
    pick_folders: fn() -> Vec<PathBuf>,
}

impl Dialog {
    /// The dialog on `workspace`'s groups; with `new_group`, straight into a
    /// new group, its name focused.
    pub fn new(workspace: Option<&Workspace>, fingerprint: u64, new_group: bool) -> Self {
        let mut dialog = Self {
            draft: Draft::new(workspace),
            fingerprint,
            save_error: None,
            conflict: false,
            adding: None,
            add_filter: String::new(),
            add_active: None,
            add_reveal: false,
            deleting: None,
            discarding: false,
            focus_name: None,
            focus_row: None,
            flash: None,
            blocks: Vec::new(),
            reveal_error: false,
            projects: Vec::new(),
            has_issues: HashMap::new(),
            home: crate::settings::home(),
            pick_folders: crate::shell::dialog::pick_folders,
        };
        if new_group {
            dialog.new_group();
        }
        dialog
    }

    /// Asks `pick` for folders instead of the OS picker.
    pub fn with_picker(mut self, pick: fn() -> Vec<PathBuf>) -> Self {
        self.pick_folders = pick;
        self
    }

    /// Lists `projects` in the Add project menus.
    pub fn with_projects(mut self, projects: Vec<PathBuf>) -> Self {
        self.projects = projects;
        self
    }

    fn has_issues(&mut self, repo: &Path) -> bool {
        *self
            .has_issues
            .entry(repo.to_path_buf())
            .or_insert_with(|| has_issues_dir(repo))
    }

    fn new_group(&mut self) {
        let at = self.draft.add_group();
        self.focus_name = Some(at);
        self.adding = None;
    }

    fn open_add(&mut self, at: usize) {
        self.add_filter.clear();
        self.add_active = None;
        self.adding = Some(at);
    }

    fn save(&mut self, overwrite: bool) -> Action {
        self.discarding = false;
        let groups = self.draft.validate();
        self.reveal_error = groups.is_some();
        match groups {
            Some(groups) if overwrite => Action::Overwrite(groups),
            Some(groups) => Action::Save(groups),
            None => {
                self.focus_name = self.draft.groups.iter().position(|g| g.error.is_some());
                Action::Keep
            }
        }
    }

    fn cancel(&mut self) -> Action {
        if self.draft.is_dirty() && !self.discarding {
            self.discarding = true;
            Action::Keep
        } else {
            Action::Close
        }
    }

    /// The group the focus is in, as drawn last frame.
    pub fn focused_group(&self, ctx: &egui::Context) -> Option<usize> {
        let focused = ctx.read_response(ctx.memory(|m| m.focused())?)?.rect;
        self.blocks
            .iter()
            .position(|block| block.contains_rect(focused))
    }

    /// The keys the dialog owns: the confirmation's first, then an open
    /// menu's, then its own. Taken before the fields draw, so `⌘⌫` in a name
    /// doesn't also clear it.
    fn keys(&mut self, ctx: &egui::Context) -> Action {
        let take = |modifiers, key| ctx.input_mut(|i| i.consume_key(modifiers, key));
        if let Some(at) = self.deleting {
            if widgets::enter_confirms(ctx) && take(Modifiers::NONE, Key::Enter) {
                self.draft.delete_group(at);
                self.deleting = None;
            } else if take(Modifiers::NONE, Key::Escape) {
                self.deleting = None;
            }
            return Action::Keep;
        }
        if self.adding.is_some() && take(Modifiers::NONE, Key::Escape) {
            self.adding = None;
            return Action::Keep;
        }
        if let Some(at) = self.adding {
            self.add_keys(ctx, at);
        }
        if take(Modifiers::COMMAND, Key::Enter) {
            return self.save(self.conflict);
        }
        if take(Modifiers::COMMAND, Key::N) {
            self.new_group();
            return Action::Keep;
        }
        if let Some(at) = self.focused_group(ctx)
            && take(Modifiers::COMMAND, Key::Backspace)
        {
            self.deleting = Some(at);
            return Action::Keep;
        }
        if self.discarding && take(Modifiers::NONE, Key::Escape) {
            self.discarding = false;
            return Action::Keep;
        }
        if take(Modifiers::NONE, Key::Escape) {
            return self.cancel();
        }
        Action::Keep
    }

    pub fn show(&mut self, ui: &egui::Ui, config_file: &str) -> Action {
        let ctx = ui.ctx().clone();
        let mut action = self.keys(&ctx);
        widgets::modal(ui, "groups-dialog", theme::ModalSize::M, "Project groups").show(
            &ctx,
            |ui| {
                ui.set_width(theme::modal_size(theme::ModalSize::M, ctx.content_rect()).x);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                if widgets::modal_header(ui, "Project groups", |_| {}) {
                    action = self.cancel();
                }
                widgets::rule(ui);
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .id_salt("groups-body")
                        .content_margin(theme::scroll_margin(theme::SPACE_5, theme::SPACE_5))
                        .max_height(widgets::modal_body_max(ctx.content_rect()))
                        .auto_shrink([false, true]),
                    |ui| {
                        widgets::focus_here(ui);
                        self.body(ui);
                        if let Some(picked) = self.save_problem(ui, config_file) {
                            action = picked;
                        }
                    },
                );
                widgets::rule(ui);
                if self.discarding {
                    self.discard_bar(ui, &mut action);
                }
                let p = theme::palette(ui);
                let dirty = self.draft.is_dirty();
                let footer = widgets::modal_footer_with(ui, config_file, "Save", |ui| {
                    if dirty {
                        widgets::pill(ui, "Unsaved", p.status_doing);
                    }
                });
                match footer {
                    FooterClick::None => {}
                    FooterClick::Primary => action = self.save(self.conflict),
                    FooterClick::Cancel => action = self.cancel(),
                }
            },
        );
        if let Some(at) = self.deleting {
            self.confirm_delete(ui, at);
        }
        action
    }

    fn body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.spacing_mut().item_spacing.y = theme::SPACE_4;
        if self.draft.groups.is_empty() {
            self.no_groups(ui);
        } else {
            widgets::help(ui, HELP, p.ink_muted);
            self.blocks.clear();
            for at in 0..self.draft.groups.len() {
                if at < self.draft.groups.len() {
                    self.group_block(ui, at);
                }
            }
            if widgets::button(
                ui,
                ButtonKind::Secondary,
                Some(Icon::Plus),
                "New group",
                Some("⌘N"),
            )
            .clicked()
            {
                self.new_group();
            }
        }
    }

    /// Why the last save failed, at the end of the body: a file changed on
    /// disk offers Reload and Keep your version. Scrolled into view as it
    /// comes.
    fn save_problem(&mut self, ui: &mut egui::Ui, config_file: &str) -> Option<Action> {
        let p = theme::palette(ui);
        let mut action = None;
        if self.conflict {
            let text = format!(
                "Couldn't save {config_file}: the file changed on disk. Reload or keep your version."
            );
            widgets::help(ui, &text, p.danger);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                if widgets::button(ui, ButtonKind::Secondary, None, "Reload", None).clicked() {
                    action = Some(Action::Reload);
                }
                if widgets::button(ui, ButtonKind::Secondary, None, "Keep your version", None)
                    .clicked()
                {
                    action = Some(self.save(true));
                }
            });
        } else if let Some(error) = &self.save_error {
            widgets::help(ui, error, p.danger);
        } else {
            return None;
        }
        if std::mem::take(&mut self.reveal_error) {
            ui.scroll_to_cursor(Some(Align::Max));
        }
        action
    }

    fn no_groups(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let response = Frame::new()
            .fill(p.surface_sunken)
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(theme::SPACE_5)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                    ui.label(RichText::new("No groups yet").font(theme::ui_strong()).color(p.ink));
                    widgets::help(
                        ui,
                        "Each group is a board of its own projects: columns merge by kind, and each issue carries its repo: label.",
                        p.ink_muted,
                    );
                    if widgets::button(ui, ButtonKind::Primary, Some(Icon::Plus), "New group", Some("⌘N"))
                        .clicked()
                    {
                        self.new_group();
                    }
                });
            })
            .response;
        widgets::dashed_rect(ui.painter(), response.rect, theme::RADIUS_MD, p.line);
    }

    fn group_block(&mut self, ui: &mut egui::Ui, at: usize) {
        let p = theme::palette(ui);
        let edge = match self.draft.groups[at].new {
            true => p.accent,
            false => p.line_soft,
        };
        let block = Frame::new()
            .stroke(Stroke::new(1.0, edge))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(theme::SPACE_4)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                self.group_head(ui, at);
                let group = &self.draft.groups[at];
                if let Some(error) = &group.error {
                    widgets::help(ui, error, p.danger);
                }
                if let Some(notice) = &group.notice {
                    widgets::help(ui, notice, p.status_doing);
                }
                if group.repos.is_empty() {
                    self.empty_group(ui, at);
                } else {
                    self.project_rows(ui, at);
                }
                let add = widgets::button(
                    ui,
                    ButtonKind::Secondary,
                    Some(Icon::Plus),
                    "Add project",
                    None,
                );
                if add.clicked() {
                    match self.adding {
                        Some(open) if open == at => self.adding = None,
                        _ => self.open_add(at),
                    }
                }
                if self.adding == Some(at) {
                    self.add_menu(ui, at);
                }
            });
        self.blocks.push(block.response.rect);
    }

    /// The name, the project count, and the trash at the far end.
    fn group_head(&mut self, ui: &mut egui::Ui, at: usize) {
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            let count = match self.draft.groups[at].repos.len() {
                1 => "1 project".to_string(),
                n => format!("{n} projects"),
            };
            let width = ui.available_width() - 90.0 - widgets::CARD_ACTION - theme::SPACE_3;
            let name = widgets::text_entry(
                ui,
                widgets::field_edit(
                    ui,
                    &mut self.draft.groups[at].name,
                    "Group name",
                    theme::ui_strong(),
                )
                .id(name_id(at))
                .desired_width(width),
            );
            if self.focus_name == Some(at) && !ui.is_sizing_pass() {
                name.request_focus();
                self.focus_name = None;
            }
            if name.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.open_add(at);
            }
            ui.label(RichText::new(count).font(theme::meta()).color(p.ink_muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let delete = widgets::icon_button(
                    ui,
                    Icon::Trash,
                    Id::unique(("delete-group", at)),
                    "Delete group (asks first)",
                    Tone::Danger,
                );
                if delete.clicked() {
                    self.deleting = Some(at);
                }
            });
        });
    }

    fn empty_group(&mut self, ui: &mut egui::Ui, at: usize) {
        let p = theme::palette(ui);
        let response = Frame::new()
            .fill(p.surface_sunken)
            .corner_radius(CornerRadius::same(theme::RADIUS_SM))
            .inner_margin(theme::SPACE_3)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                    ui.label(
                        RichText::new("No projects in this group yet —")
                            .font(theme::ui_small())
                            .color(p.ink_faint),
                    );
                    if ui
                        .link(RichText::new("Add a project").font(theme::ui_small()))
                        .clicked()
                    {
                        self.open_add(at);
                    }
                });
            })
            .response;
        widgets::dashed_rect(ui.painter(), response.rect, theme::RADIUS_SM, p.line);
    }

    fn project_rows(&mut self, ui: &mut egui::Ui, at: usize) {
        let mut removed = None;
        for index in 0..self.draft.groups[at].repos.len() {
            if index > 0 {
                widgets::rule(ui);
            }
            let repo = self.draft.groups[at].repos[index].clone();
            if let Some(by_key) = self.project_row(ui, at, index, &repo) {
                removed = Some((index, by_key));
            }
        }
        if let Some((index, by_key)) = removed {
            self.draft.remove_project(at, index);
            let left = self.draft.groups[at].repos.len();
            if by_key && left > 0 {
                self.focus_row = Some((at, index.min(left - 1)));
            }
        }
    }

    /// One project: its name and path, and × on hover or focus. `tab` stops
    /// on the row, where `Del` removes it. `Some` when it asks to go, saying
    /// whether by key.
    fn project_row(
        &mut self,
        ui: &mut egui::Ui,
        at: usize,
        index: usize,
        repo: &Path,
    ) -> Option<bool> {
        let p = theme::palette(ui);
        let name = repo.file_name().map_or_else(
            || repo.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let path = config::contract(repo, self.home.as_deref());
        let size = Vec2::new(ui.available_width(), widgets::MENU_ROW_H);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let row = ui.interact(rect, row_id(at, index), egui::Sense::click());
        row.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{name}, {path}"))
        });
        if row.clicked() {
            row.request_focus();
        }
        if self.focus_row == Some((at, index)) {
            self.focus_row = None;
            row.request_focus();
        }
        let remove_id = Id::unique(("remove-project", at, index));
        let focused = row.has_focus() || ui.memory(|m| m.has_focus(remove_id));
        let hovered = ui.rect_contains_pointer(rect);
        if hovered || focused {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
        }
        self.paint_flash(ui, at, repo, rect);
        widgets::focus_ring(ui, &row, theme::RADIUS_SM);
        let inner = rect.shrink2(Vec2::new(theme::SPACE_2, 0.0));
        let mut line = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::right_to_left(Align::Center)),
        );
        line.style_mut().interaction.selectable_labels = false;
        let remove = (hovered || focused).then(|| {
            widgets::icon_button(
                &mut line,
                Icon::Close,
                remove_id,
                "Remove from this group. The folder is not touched. (Del)",
                Tone::Plain,
            )
        });
        let found = self.has_issues(repo);
        line.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
            ui.add_space(theme::SPACE_1);
            ui.add(
                egui::Label::new(RichText::new(&name).font(theme::ui_strong()).color(p.ink))
                    .truncate(),
            );
            let (path, color) = match found {
                true => (path.clone(), p.ink_muted),
                false => (format!("{path} · no issues/ folder"), p.status_doing),
            };
            ui.add(
                egui::Label::new(RichText::new(path).font(theme::meta()).color(color)).truncate(),
            );
        });
        let by_key = focused
            && ui.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Delete)
                    || i.consume_key(Modifiers::NONE, Key::Backspace)
            });
        if by_key {
            Some(true)
        } else {
            remove
                .is_some_and(|remove| remove.clicked())
                .then_some(false)
        }
    }

    /// The `accent` flash of a row whose folder was picked again.
    fn paint_flash(&mut self, ui: &egui::Ui, at: usize, repo: &Path, rect: Rect) {
        let Some((group, folder, since)) = &self.flash else {
            return;
        };
        if *group != at || folder != repo {
            return;
        }
        let left = 1.0 - (ui.input(|i| i.time) - since) / FLASH_SECONDS;
        if left <= 0.0 {
            self.flash = None;
            return;
        }
        let p = theme::palette(ui);
        let reduced = theme::reduced_motion(ui.ctx());
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            Stroke::new(1.5, p.accent.gamma_multiply(flash_strength(left, reduced))),
            egui::StrokeKind::Inside,
        );
        if reduced {
            let rest = std::time::Duration::from_secs_f64(left * FLASH_SECONDS);
            ui.ctx().request_repaint_after(rest);
        } else {
            ui.ctx().request_repaint();
        }
    }

    /// The Add project menu under the group's button: the user's projects
    /// not in the group yet, in their order, with a filter once there are
    /// more than 8, the keyboard's row filled; then the OS picker. It stays
    /// open to add more.
    fn add_menu(&mut self, ui: &mut egui::Ui, at: usize) {
        let width = ui.available_width() - 2.0 * theme::SPACE_1;
        let fresh = self.fresh(at);
        let filterable = fresh.len() > ADD_FILTER_FROM;
        let mut listed = self.listed(&fresh);
        self.add_active = self.add_active.and_then(|row| match listed.len() {
            0 => None,
            len => Some(row.min(len - 1)),
        });
        let mut picked: Vec<PathBuf> = Vec::new();
        widgets::popover_frame(ui)
            .inner_margin(theme::SPACE_1)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                if !fresh.is_empty() {
                    widgets::menu_group(ui, "Your projects");
                    if filterable {
                        let field = widgets::text_entry(
                            ui,
                            widgets::field_edit(
                                ui,
                                &mut self.add_filter,
                                "Filter projects",
                                theme::ui_body(),
                            )
                            .desired_width(f32::INFINITY),
                        );
                        if !field.has_focus() && ui.memory(|m| m.focused().is_none()) {
                            field.request_focus();
                        }
                        if field.changed() {
                            listed = self.listed(&fresh);
                            self.add_active = (!self.add_filter.is_empty()).then_some(0);
                            self.add_reveal = true;
                        }
                    }
                    egui::ScrollArea::vertical()
                        .max_height(ADD_LIST_MAX_H)
                        .show(ui, |ui| {
                            for (row, item) in listed.into_iter().enumerate() {
                                let active = self.add_active == Some(row);
                                let look = RowLook {
                                    active,
                                    mark: item.mark,
                                    ..RowLook::default()
                                };
                                let response =
                                    widgets::menu_row(ui, width, &item.name, &item.path, look);
                                if active && std::mem::take(&mut self.add_reveal) {
                                    response.scroll_to_me(Some(Align::Center));
                                }
                                if response.clicked() {
                                    picked.push(item.project);
                                }
                            }
                        });
                    widgets::menu_separator(ui);
                }
                let choose = widgets::menu_row(
                    ui,
                    width,
                    "Choose folders…",
                    "Choose a project containing an issues/ folder",
                    RowLook::default(),
                );
                if choose.clicked() {
                    picked.extend((self.pick_folders)());
                }
            });
        let now = ui.input(|i| i.time);
        for dir in picked {
            self.add_picked(at, &dir, now);
        }
    }

    /// The user's projects not in the `at`th group yet, in their order.
    fn fresh(&self, at: usize) -> Vec<PathBuf> {
        self.projects
            .iter()
            .filter(|project| !self.draft.groups[at].repos.contains(project))
            .cloned()
            .collect()
    }

    /// The rows of `fresh` the Add project menu's filter keeps.
    fn listed(&self, fresh: &[PathBuf]) -> Vec<AddRow> {
        fresh
            .iter()
            .filter_map(|project| {
                let name = project
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let path = config::contract(project, self.home.as_deref());
                let mark = matches(&name, &path, &self.add_filter)?;
                Some(AddRow {
                    project: project.clone(),
                    name,
                    path,
                    mark,
                })
            })
            .collect()
    }

    /// The Add project menu's type-to-filter keys: the arrows move round its
    /// projects, `↩` adds the active one and the menu stays open.
    fn add_keys(&mut self, ctx: &egui::Context, at: usize) {
        let take = |key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key));
        let listed = self.listed(&self.fresh(at));
        for (key, down) in [(Key::ArrowDown, true), (Key::ArrowUp, false)] {
            if !listed.is_empty() && take(key) {
                self.add_active = Some(step(self.add_active, listed.len(), down));
                self.add_reveal = true;
            }
        }
        if let Some(row) = self.add_active.and_then(|row| listed.get(row))
            && take(Key::Enter)
        {
            let now = ctx.input(|i| i.time);
            self.add_picked(at, &row.project, now);
        }
    }

    /// Adds a folder picked at `now`: one already in the group flashes its row.
    pub fn add_picked(&mut self, at: usize, dir: &Path, now: f64) {
        let found = has_issues_dir(dir);
        self.has_issues.insert(dir.to_path_buf(), found);
        if self.draft.add_project(at, dir, |_| found) == Added::AlreadyThere {
            self.draft.groups[at].notice = None;
            self.flash = Some((at, dir.to_path_buf(), now));
        }
    }

    fn discard_bar(&mut self, ui: &mut egui::Ui, action: &mut Action) {
        match widgets::discard_bar(ui, "Discard changes to groups?") {
            Some(true) => *action = Action::Close,
            Some(false) => self.discarding = false,
            None => {}
        }
    }

    /// ConfirmDialog over this one: deleting a group always asks.
    fn confirm_delete(&mut self, ui: &egui::Ui, at: usize) {
        let Some(group) = self.draft.groups.get(at) else {
            self.deleting = None;
            return;
        };
        let name = match group.name.trim() {
            "" => "Unnamed group".to_string(),
            name => name.to_string(),
        };
        let paths: Vec<String> = group
            .repos
            .iter()
            .map(|repo| config::contract(repo, self.home.as_deref()))
            .collect();
        let count = match paths.len() {
            1 => "1 project".to_string(),
            n => format!("{n} projects"),
        };
        let detail = match paths.is_empty() {
            true => count,
            false => format!("{count} · {}", paths.join(", ")),
        };
        let ask = widgets::Ask {
            id: "confirm-delete-group",
            question: "Delete this group?",
            title: &name,
            detail: &detail,
            consequence: "Only the group is removed. The projects and their issues/ folders stay where they are. Nothing is written until you save.",
            confirm: "Delete group",
        };
        match widgets::confirm(ui, &ask) {
            widgets::Answer::Confirmed => {
                self.draft.delete_group(at);
                self.deleting = None;
            }
            widgets::Answer::Cancelled => self.deleting = None,
            widgets::Answer::Pending => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flash_holds_steady_under_reduced_motion() {
        assert_eq!(flash_strength(0.5, false), 0.5);
        assert_eq!(flash_strength(0.5, true), 1.0);
    }
}
