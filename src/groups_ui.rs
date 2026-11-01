//! GroupsDialog: creates and edits the project groups, a friendlier way to
//! edit the `[[workspace.group]]` tables of the user configuration (see
//! `design/components/GroupsDialog`). It works on a [`Draft`]; the app
//! writes the groups [`Action::Save`] hands back.

use std::path::{Path, PathBuf};

use egui::{
    Align, CornerRadius, Frame, Id, Key, Layout, Margin, Modifiers, RichText, Stroke, Vec2,
};

use crate::theme::{self, Icon};
use crate::widgets::{self, ButtonKind, FooterClick, Tone};
use crate::workspace::config::{self, has_issues_dir};
use crate::workspace::draft::{Added, Draft};
use crate::workspace::{Group, Workspace};

/// The body scrolls past this height.
const BODY_MAX_H: f32 = 520.0;
/// The Add project menu filters once it lists more projects than this.
const ADD_FILTER_FROM: usize = 8;
/// The Add project menu's list scrolls after eight two-line rows.
const ADD_LIST_MAX_H: f32 = 8.0 * 40.0;

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
    /// The group about to be deleted, asking first.
    deleting: Option<usize>,
    /// `esc` with unsaved changes: asking whether to drop them.
    discarding: bool,
    /// The group whose name takes the focus next frame.
    focus_name: Option<usize>,
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
            deleting: None,
            discarding: false,
            focus_name: None,
            home: crate::settings::home(),
            pick_folders: crate::dialog::pick_folders,
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

    fn new_group(&mut self) {
        let at = self.draft.add_group();
        self.focus_name = Some(at);
        self.adding = None;
    }

    fn save(&mut self, overwrite: bool) -> Action {
        self.discarding = false;
        match self.draft.validate() {
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

    /// The keys the dialog owns: the confirmation's first, then an open
    /// menu's, then its own.
    fn keys(&mut self, ctx: &egui::Context) -> Action {
        let take = |modifiers, key| ctx.input_mut(|i| i.consume_key(modifiers, key));
        if let Some(at) = self.deleting {
            if take(Modifiers::NONE, Key::Enter) {
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
        if take(Modifiers::COMMAND, Key::Enter) {
            return self.save(self.conflict);
        }
        if take(Modifiers::COMMAND, Key::N) {
            self.new_group();
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

    pub fn show(&mut self, ui: &egui::Ui, projects: &[PathBuf], config_file: &str) -> Action {
        let ctx = ui.ctx().clone();
        let mut action = self.keys(&ctx);
        widgets::modal(ui, "groups-dialog", theme::ModalSize::M).show(&ctx, |ui| {
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
                    .max_height(BODY_MAX_H)
                    .auto_shrink([false, true]),
                true,
                |ui| self.body(ui, projects),
            );
            widgets::rule(ui);
            if let Some(picked) = self.error_bar(ui) {
                action = picked;
            }
            if self.discarding {
                self.discard_bar(ui, &mut action);
            }
            let note = match self.draft.is_dirty() {
                true => format!("{config_file} · Unsaved"),
                false => config_file.to_string(),
            };
            match widgets::modal_footer(ui, &note, "Save") {
                FooterClick::None => {}
                FooterClick::Primary => action = self.save(self.conflict),
                FooterClick::Cancel => action = self.cancel(),
            }
        });
        if let Some(at) = self.deleting {
            self.confirm_delete(ui, at);
        }
        action
    }

    fn body(&mut self, ui: &mut egui::Ui, projects: &[PathBuf]) {
        let p = theme::palette(ui);
        ui.spacing_mut().item_spacing.y = theme::SPACE_4;
        if self.draft.groups.is_empty() {
            self.no_groups(ui);
        } else {
            widgets::help(ui, HELP, p.ink_muted);
            for at in 0..self.draft.groups.len() {
                if at < self.draft.groups.len() {
                    self.group_block(ui, at, projects);
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

    /// Why the last save failed, above the footer so it is always in view:
    /// a file changed on disk offers Reload and Keep your version.
    fn error_bar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let p = theme::palette(ui);
        let mut action = None;
        if !self.conflict && self.save_error.is_none() {
            return None;
        }
        Frame::new()
            .inner_margin(Margin::symmetric(
                theme::SPACE_5 as i8,
                theme::SPACE_3 as i8,
            ))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                if self.conflict {
                    widgets::help(
                        ui,
                        "The file changed on disk. Reload or keep your version.",
                        p.danger,
                    );
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                        if widgets::button(ui, ButtonKind::Secondary, None, "Reload", None)
                            .clicked()
                        {
                            action = Some(Action::Reload);
                        }
                        if widgets::button(
                            ui,
                            ButtonKind::Secondary,
                            None,
                            "Keep your version",
                            None,
                        )
                        .clicked()
                        {
                            action = Some(self.save(true));
                        }
                    });
                } else if let Some(error) = &self.save_error {
                    widgets::help(ui, error, p.danger);
                }
            });
        action
    }

    fn no_groups(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let response = Frame::new()
            .fill(p.surface_sunken)
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(Margin::same(theme::SPACE_5 as i8))
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
        widgets::dashed_rect(ui.painter(), response.rect, theme::RADIUS_MD, p.line_strong);
    }

    fn group_block(&mut self, ui: &mut egui::Ui, at: usize, projects: &[PathBuf]) {
        let p = theme::palette(ui);
        let edge = match self.draft.groups[at].new {
            true => p.accent,
            false => p.line_strong,
        };
        Frame::new()
            .stroke(Stroke::new(1.0, edge))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(Margin::same(theme::SPACE_4 as i8))
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
                    self.adding = match self.adding {
                        Some(open) if open == at => None,
                        _ => Some(at),
                    };
                }
                if self.adding == Some(at) {
                    self.add_menu(ui, at, projects);
                }
            });
    }

    fn group_head(&mut self, ui: &mut egui::Ui, at: usize) {
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            let count = match self.draft.groups[at].repos.len() {
                1 => "1 project".to_string(),
                n => format!("{n} projects"),
            };
            let delete = widgets::icon_button(
                ui,
                Icon::Trash,
                Id::new(("delete-group", at)),
                "Delete group (asks first)",
                Tone::Danger,
            );
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let width = ui.available_width() - 90.0;
                let name = widgets::text_entry(
                    ui,
                    egui::TextEdit::singleline(&mut self.draft.groups[at].name)
                        .hint_text(RichText::new("Group name").color(p.ink_faint))
                        .font(theme::ui_strong())
                        .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
                        .min_size(Vec2::new(0.0, theme::CONTROL_H))
                        .desired_width(width),
                );
                if self.focus_name == Some(at) {
                    name.request_focus();
                    self.focus_name = None;
                }
                if name.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.add_filter.clear();
                    self.adding = Some(at);
                }
                if name.has_focus()
                    && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Backspace))
                {
                    self.deleting = Some(at);
                }
                ui.label(RichText::new(count).font(theme::meta()).color(p.ink_muted));
            });
            if delete.clicked() {
                self.deleting = Some(at);
            }
        });
    }

    fn empty_group(&mut self, ui: &mut egui::Ui, at: usize) {
        let p = theme::palette(ui);
        let response = Frame::new()
            .fill(p.surface_sunken)
            .corner_radius(CornerRadius::same(theme::RADIUS_SM))
            .inner_margin(Margin::same(theme::SPACE_3 as i8))
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
                        self.add_filter.clear();
                        self.adding = Some(at);
                    }
                });
            })
            .response;
        widgets::dashed_rect(ui.painter(), response.rect, theme::RADIUS_SM, p.line_strong);
    }

    fn project_rows(&mut self, ui: &mut egui::Ui, at: usize) {
        let p = theme::palette(ui);
        let mut removed = None;
        for (index, repo) in self.draft.groups[at].repos.iter().enumerate() {
            if index > 0 {
                widgets::rule(ui);
            }
            let row = ui.horizontal(|ui| {
                let remove = widgets::icon_button(
                    ui,
                    Icon::Close,
                    Id::new(("remove-project", at, index)),
                    "Remove from this group. The folder is not touched.",
                    Tone::Plain,
                );
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                    let name = repo.file_name().map_or_else(
                        || repo.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    );
                    ui.label(RichText::new(name).font(theme::ui_strong()).color(p.ink));
                    let path = config::contract(repo, self.home.as_deref());
                    let (path, color) = match has_issues_dir(repo) {
                        true => (path, p.ink_muted),
                        false => (format!("{path} · no issues/ folder"), p.status_doing),
                    };
                    ui.add(
                        egui::Label::new(RichText::new(path).font(theme::meta()).color(color))
                            .truncate(),
                    );
                });
                remove.clicked()
            });
            if row.inner {
                removed = Some(index);
            }
        }
        if let Some(index) = removed {
            self.draft.remove_project(at, index);
        }
    }

    /// The Add project menu under the group's button: the user's projects
    /// not in the group yet, in their order, with a filter once there are
    /// more than 8; then the OS picker. It stays open to add more.
    fn add_menu(&mut self, ui: &mut egui::Ui, at: usize, projects: &[PathBuf]) {
        let p = theme::palette(ui);
        let width = ui.available_width() - 2.0 * theme::SPACE_1;
        let fresh: Vec<&PathBuf> = projects
            .iter()
            .filter(|project| !self.draft.groups[at].repos.contains(project))
            .collect();
        let filterable = fresh.len() > ADD_FILTER_FROM;
        let query = self.add_filter.trim().to_lowercase();
        let mut picked: Vec<PathBuf> = Vec::new();
        widgets::popover_frame(ui)
            .inner_margin(Margin::same(theme::SPACE_1 as i8))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                if !fresh.is_empty() {
                    widgets::menu_group(ui, "Your projects");
                    if filterable {
                        let field = widgets::text_entry(
                            ui,
                            egui::TextEdit::singleline(&mut self.add_filter)
                                .hint_text(RichText::new("Filter projects").color(p.ink_faint))
                                .font(theme::ui_body())
                                .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
                                .min_size(Vec2::new(0.0, theme::CONTROL_H))
                                .desired_width(f32::INFINITY),
                        );
                        if !field.has_focus() && ui.memory(|m| m.focused().is_none()) {
                            field.request_focus();
                        }
                    }
                    egui::ScrollArea::vertical()
                        .max_height(ADD_LIST_MAX_H)
                        .show(ui, |ui| {
                            for project in &fresh {
                                let name = project
                                    .file_name()
                                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                                let path = config::contract(project, self.home.as_deref());
                                let shown = query.is_empty()
                                    || name.to_lowercase().contains(&query)
                                    || path.to_lowercase().contains(&query);
                                if shown
                                    && widgets::menu_row(ui, width, &name, &path, false).clicked()
                                {
                                    picked.push((*project).clone());
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
                    false,
                );
                if choose.clicked() {
                    picked.extend((self.pick_folders)());
                }
            });
        for dir in picked {
            self.add(at, &dir);
        }
    }

    fn add(&mut self, at: usize, dir: &Path) {
        if self.draft.add_project(at, dir, has_issues_dir) == Added::AlreadyThere {
            self.draft.groups[at].notice = None;
        }
    }

    fn discard_bar(&mut self, ui: &mut egui::Ui, action: &mut Action) {
        let p = theme::palette(ui);
        Frame::new()
            .inner_margin(Margin::symmetric(
                theme::SPACE_5 as i8,
                theme::SPACE_3 as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    ui.label(
                        RichText::new("Discard changes to groups?")
                            .font(theme::ui_strong())
                            .color(p.ink),
                    );
                    if widgets::button(ui, ButtonKind::Secondary, None, "Discard", None).clicked() {
                        *action = Action::Close;
                    }
                    if widgets::button(ui, ButtonKind::Secondary, None, "Keep editing", Some("esc"))
                        .clicked()
                    {
                        self.discarding = false;
                    }
                });
            });
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
