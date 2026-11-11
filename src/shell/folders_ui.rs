//! FoldersDialog: the four standard column folders a single-project board
//! expects, present or missing, with the board folder's name beside them (see
//! `design/components/FoldersDialog`). It works on a name; the app writes the
//! folders, the config and the ignore entry.

use std::path::{Path, PathBuf};

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{Align, Color32, FontId, Frame, Id, Key, Layout, Modifiers, RichText, Sense, Ui, Vec2};

use crate::domains::onboarding::folders::{self, MAX_NAME_CHARS, Standard};
use crate::shell::theme;
use crate::shell::widgets::{self, ButtonKind};

/// What the dialog asks the app this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Not now, or `esc`: leave the board as it is and remember the project.
    Close,
    /// Write the name, then the ignore entry when asked, then the folders.
    Create { name: String, ignore: bool },
}

/// The dialog's one field.
pub fn name_id() -> Id {
    Id::unique("folders-name")
}

pub struct Dialog {
    pub name: String,
    /// Why the last creation could not finish, above the foot.
    pub error: Option<String>,
    project: PathBuf,
    project_name: String,
    project_path: String,
    /// The name the board opens on now.
    current_name: String,
    current_root: PathBuf,
    /// `iboard.toml` sets the name.
    configured: bool,
    ignore: bool,
    opening: bool,
    not_now_id: Option<Id>,
    primary_id: Option<Id>,
}

impl Dialog {
    pub fn new(
        project: PathBuf,
        project_name: String,
        project_path: String,
        current_name: String,
        current_root: PathBuf,
        configured: bool,
    ) -> Self {
        Self {
            name: current_name.clone(),
            error: None,
            project,
            project_name,
            project_path,
            current_name,
            current_root,
            configured,
            ignore: true,
            opening: true,
            not_now_id: None,
            primary_id: None,
        }
    }

    pub fn project(&self) -> &Path {
        &self.project
    }

    /// The name is one segment the field owns; a configured path with a slash
    /// stays as it is until it is typed over.
    fn problem(&self) -> Option<&'static str> {
        (self.name != self.current_name)
            .then(|| folders::name_problem(&self.name))
            .flatten()
    }

    fn target(&self) -> PathBuf {
        match self.name == self.current_name || folders::name_problem(&self.name).is_some() {
            true => self.current_root.clone(),
            false => self.project.join(&self.name),
        }
    }

    fn missing(&self) -> Vec<Standard> {
        folders::missing(&self.target())
    }

    fn can_submit(&self) -> bool {
        self.problem().is_none() && (!self.missing().is_empty() || self.name != self.current_name)
    }

    fn primary_label(&self) -> String {
        let missing = self.missing().len();
        match (missing, self.name != self.current_name) {
            (0, false) => "Nothing to create".to_string(),
            (0, true) => format!("Use `{}/`", self.name.trim_end_matches('/')),
            (1, _) => "Create folder".to_string(),
            (n, _) => format!("Create {n} folders"),
        }
    }

    fn create(&self) -> Action {
        Action::Create {
            name: self.name.clone(),
            ignore: self.ignore,
        }
    }

    pub fn show(&mut self, ui: &Ui) -> Option<Action> {
        let ctx = ui.ctx().clone();
        let mut action = self.keys(&ctx);
        let frame = widgets::modal_frame(ui).inner_margin(20.0);
        let response = widgets::modal(ui, "folders-dialog", theme::ModalSize::S, "Board folders")
            .frame(frame)
            .show(&ctx, |ui| {
                let inner = theme::modal_size(theme::ModalSize::S, ctx.content_rect()).x - 40.0;
                ui.set_width(inner);
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                self.head(ui);
                ui.add_space(theme::SPACE_5);
                self.field(ui);
                self.ignore_box(ui);
                ui.add_space(theme::SPACE_4);
                self.body(ui);
                ui.add_space(theme::SPACE_3);
                self.rows(ui);
                if let Some(error) = &self.error {
                    ui.add_space(theme::SPACE_3);
                    widgets::help(ui, error, theme::palette(ui).danger);
                }
                ui.add_space(theme::SPACE_5);
                if let Some(picked) = self.foot(ui) {
                    action = Some(picked);
                }
            });
        if action.is_none() && response.should_close() {
            action = Some(Action::Close);
        }
        action
    }

    fn keys(&mut self, ctx: &egui::Context) -> Option<Action> {
        let take = |modifiers, key| ctx.input_mut(|i| i.consume_key(modifiers, key));
        if take(Modifiers::NONE, Key::Escape) {
            return Some(Action::Close);
        }
        if take(Modifiers::NONE, Key::Enter) && !self.button_focused(ctx) && self.can_submit() {
            return Some(self.create());
        }
        None
    }

    fn button_focused(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| {
            self.not_now_id.is_some_and(|id| m.has_focus(id))
                || self.primary_id.is_some_and(|id| m.has_focus(id))
        })
    }

    fn head(&self, ui: &mut Ui) {
        let p = theme::palette(ui);
        ui.label(
            RichText::new("Board folders")
                .font(theme::ui_title())
                .color(p.ink),
        );
        ui.add_space(theme::SPACE_3);
        Frame::new()
            .fill(p.surface_sunken)
            .corner_radius(egui::CornerRadius::same(theme::RADIUS_SM))
            .inner_margin(Vec2::new(10.0, theme::SPACE_3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                ui.add(
                    egui::Label::new(
                        RichText::new(&self.project_name)
                            .font(theme::ui_strong())
                            .color(p.ink),
                    )
                    .truncate(),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(&self.project_path)
                            .font(theme::meta())
                            .color(p.ink_muted),
                    )
                    .truncate(),
                );
            });
    }

    fn field(&mut self, ui: &mut Ui) {
        let p = theme::palette(ui);
        widgets::field_label(ui, "Folder");
        let response = widgets::text_entry(
            ui,
            widgets::field_edit(ui, &mut self.name, "", FontId::monospace(12.0))
                .id(name_id())
                .char_limit(MAX_NAME_CHARS)
                .desired_width(f32::INFINITY),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Folder"));
        if std::mem::take(&mut self.opening) {
            response.request_focus();
            let end = CCursor::new(self.name.chars().count());
            let mut state =
                egui::text_edit::TextEditState::load(ui.ctx(), name_id()).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(CCursorRange::two(CCursor::new(0), end)));
            state.store(ui.ctx(), name_id());
        }
        if self.configured {
            ui.add_space(theme::SPACE_1);
            inline(
                ui,
                &[("Set in ", false), ("iboard.toml", true), (".", false)],
                theme::meta(),
                p.ink_muted,
            );
        }
        if let Some(problem) = self.problem() {
            ui.add_space(theme::SPACE_1);
            widgets::help(ui, problem, p.danger);
        }
    }

    fn ignore_box(&mut self, ui: &mut Ui) {
        if self.problem().is_some() || self.target().is_dir() {
            return;
        }
        let Some(entry) = folders::ignore_entry(&self.project, &self.target()) else {
            return;
        };
        let p = theme::palette(ui);
        ui.add_space(theme::SPACE_2);
        widgets::checkbox(ui, &mut self.ignore, "Keep this folder out of git");
        if self.ignore {
            ui.add_space(theme::SPACE_1);
            inline(
                ui,
                &[
                    ("Adds ", false),
                    (&entry, true),
                    (" to ", false),
                    (".gitignore", true),
                    (".", false),
                ],
                theme::meta(),
                p.ink_muted,
            );
        }
    }

    fn body(&self, ui: &mut Ui) {
        let p = theme::palette(ui);
        if self.problem().is_some() {
            return;
        }
        if self.name != self.current_name {
            let old = format!("{}/", self.current_name.trim_end_matches('/'));
            let new = format!("{}/", self.name.trim_end_matches('/'));
            inline(
                ui,
                &[
                    ("The board moves to ", false),
                    (&new, true),
                    ("; ", false),
                    (&old, true),
                    (" stays where it is, off the board.", false),
                ],
                theme::ui_small(),
                p.ink_muted,
            );
            return;
        }
        let missing = self.missing().len();
        let parts = match missing {
            0 => vec![("All four columns are present in this board.", false)],
            1 => vec![
                (
                    "One of the four columns is missing from this board. It is created empty, with a ",
                    false,
                ),
                (".gitkeep", true),
                (" so git keeps it.", false),
            ],
            n => vec![
                (word(n), false),
                (
                    " of the four columns are missing from this board. Each is created empty, with a ",
                    false,
                ),
                (".gitkeep", true),
                (" so git keeps it.", false),
            ],
        };
        inline(ui, &parts, theme::ui_small(), p.ink_muted);
    }

    fn rows(&self, ui: &mut Ui) {
        let rows = folders::standard(&self.target());
        for folder in rows {
            folder_row(ui, &folder);
        }
    }

    fn foot(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut picked = None;
        let can_submit = self.can_submit();
        let label = self.primary_label();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            let primary = widgets::button(
                ui,
                ButtonKind::Primary.when(can_submit),
                None,
                &label,
                Some("↩"),
            );
            self.primary_id = Some(primary.id);
            if primary.clicked() && can_submit {
                picked = Some(self.create());
            }
            let not_now = widgets::button(ui, ButtonKind::Secondary, None, "Not now", Some("esc"));
            self.not_now_id = Some(not_now.id);
            if not_now.clicked() {
                picked = Some(Action::Close);
            }
        });
        picked
    }
}

/// A 26pt row of the four: name, backing folder, state.
fn folder_row(ui: &mut Ui, folder: &Standard) {
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
    let name_ink = match folder.present {
        true => p.ink_muted,
        false => p.ink,
    };
    let state_ink = match folder.present {
        true => p.ink_faint,
        false => p.ink,
    };
    let name = ui
        .painter()
        .layout_no_wrap(folder.name.to_string(), theme::ui_strong(), name_ink);
    let dir =
        ui.painter()
            .layout_no_wrap(format!("{}/", folder.folder), theme::meta(), p.ink_faint);
    let state = ui.painter().layout_no_wrap(
        match folder.present {
            true => "present".to_string(),
            false => "missing".to_string(),
        },
        theme::meta(),
        state_ink,
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let y = rect.center().y - name.size().y / 2.0;
    ui.painter()
        .galley(egui::pos2(rect.left(), y), name.clone(), name_ink);
    let left = rect.left() + name.size().x + theme::SPACE_2;
    ui.painter().galley(egui::pos2(left, y), dir, p.ink_faint);
    let right = rect.right() - state.size().x;
    ui.painter().galley(egui::pos2(right, y), state, state_ink);
    let state = match folder.present {
        true => "present",
        false => "missing",
    };
    let label = format!("{}, {}/, {state}", folder.name, folder.folder);
    response.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Label, true, label.clone()));
}

fn word(n: usize) -> &'static str {
    match n {
        2 => "Two",
        3 => "Three",
        _ => "Four",
    }
}

/// A sentence with its inline mono parts, wrapped to the dialog's width.
fn inline(ui: &mut Ui, parts: &[(&str, bool)], font: FontId, color: Color32) {
    let p = theme::palette(ui);
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    for (text, code) in parts {
        let font = match code {
            true => FontId::monospace(12.0),
            false => font.clone(),
        };
        job.append(
            text,
            0.0,
            TextFormat {
                font_id: font,
                color: match code {
                    true => p.ink,
                    false => color,
                },
                ..TextFormat::default()
            },
        );
    }
    ui.add(egui::Label::new(job).wrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog(project: &Path, name: &str) -> Dialog {
        let mut dialog = Dialog::new(
            project.to_path_buf(),
            "shop".to_string(),
            "~/code/shop".to_string(),
            "issues".to_string(),
            project.join("issues"),
            false,
        );
        dialog.name = name.to_string();
        dialog
    }

    #[test]
    fn the_primary_counts_what_it_creates() {
        let scratch = tempfile::tempdir().unwrap();
        let missing = dialog(scratch.path(), "tickets");
        assert_eq!(missing.primary_label(), "Create 4 folders");
        assert!(missing.can_submit());

        let same = dialog(scratch.path(), "issues");
        assert_eq!(
            same.primary_label(),
            "Create 4 folders",
            "a missing folder still counts"
        );
    }

    #[test]
    fn an_invalid_name_disables_the_primary() {
        let scratch = tempfile::tempdir().unwrap();
        let invalid = dialog(scratch.path(), "with space");
        assert_eq!(
            invalid.problem(),
            Some("A folder name holds letters, digits, `-`, `_` and inner dots.")
        );
        assert!(!invalid.can_submit());
    }

    #[test]
    fn a_configured_name_with_a_slash_stays_as_it_is() {
        let scratch = tempfile::tempdir().unwrap();
        let mut nested = dialog(scratch.path(), "notes/tickets");
        nested.current_name = "notes/tickets".to_string();
        assert_eq!(nested.problem(), None);
        nested.name = "notes/other".to_string();
        assert_eq!(nested.problem(), Some("A folder name has no slash."));
    }
}
