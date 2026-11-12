//! What the Reader says when the open issue is no longer at its path: where
//! it went, and the one step that follows it there.

use egui::epaint::MarginF32;

use super::{
    ButtonKind, Frame, IboardApp, Layout, Path, PathBuf, Rect, RichText, Sense, Stroke, Vec2,
    theme, widgets,
};
use crate::files::file_name;

/// What the notice offers.
pub(super) enum Action {
    Open(PathBuf),
    Board,
}

/// Which picture the notice shows, if any.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Art {
    Moved,
    Gone,
}

/// Where an issue the Reader can no longer find went, or that it is gone.
pub(super) struct Notice {
    pub(super) title: String,
    file: String,
    sentence: String,
    note: bool,
    destination: Option<PathBuf>,
    status: Option<theme::Status>,
    art: Option<Art>,
}

impl IboardApp {
    /// What the Reader says of `path`, which is no longer on the board.
    fn notice_of(&self, path: &Path) -> Notice {
        let file = file_name(path);
        let from = folder_name(path.parent().unwrap_or(path));
        let root = self.board.root_of(path);
        let found = self.board.columns.iter().rev().find_map(|column| {
            column
                .issues
                .iter()
                .find(|issue| issue.file_name() == file && issue.path.starts_with(root))
                .map(|issue| (column, issue.path.clone()))
        });
        match found {
            Some((column, destination)) => {
                let done = column.is_done();
                let to = folder_name(destination.parent().unwrap_or(&destination));
                let name = if column.kind.is_none() && column.dir != self.board.root {
                    format!("{to}/")
                } else {
                    column.name.clone()
                };
                Notice {
                    title: format!("Moved to {name}"),
                    file: file.clone(),
                    sentence: format!(" moved from {from}/ to {to}/."),
                    note: false,
                    destination: Some(destination),
                    status: Some(super::super::board::column_status(column)),
                    art: done.then_some(Art::Moved),
                }
            }
            None => Notice {
                title: "Issue not found".to_string(),
                file: file.clone(),
                sentence: " is no longer in any column. It was deleted, or renamed outside iboard."
                    .to_string(),
                note: false,
                destination: None,
                status: None,
                art: Some(Art::Gone),
            },
        }
    }
}

impl IboardApp {
    /// The notice for `path`, which the board no longer holds.
    pub(super) fn gone_notice(&self, path: &Path, log_open: bool) -> Notice {
        let mut notice = self.notice_of(path);
        notice.note = log_open;
        notice
    }

    /// Opens the issue where it went, carrying the panel that was open on it.
    pub(super) fn follow_moved(&mut self, from: &Path, to: PathBuf) {
        let log = self.agent_log_open(from);
        let review = self.session.review.take_if(|review| review.path == from);
        self.open_issue(to.clone());
        if let Some(mut review) = review {
            review.path = to.clone();
            self.session.review = Some(review);
        } else {
            self.session.review = None;
        }
        if log {
            if !self.agent_log_open(&to) {
                self.session.agent_log = None;
                self.toggle_agent_log(&to);
            }
        } else if self.agent_log_open(&to) {
            self.session.agent_log = None;
        }
    }
}

/// Draws the card in the issue pane, and says what its buttons asked.
pub(super) fn notice_ui(
    ui: &mut egui::Ui,
    notice: &Notice,
    focus: bool,
    path: &Path,
) -> Option<Action> {
    let p = theme::palette(ui);
    let gutter = theme::SPACE_5;
    let width = (ui.available_width() - 2.0 * gutter).min(theme::READER_MAX);
    let left = ui.max_rect().min.x + ((ui.available_width() - width) / 2.0).max(gutter);
    let rect = Rect::from_min_size(
        egui::pos2(left, ui.max_rect().min.y + theme::SPACE_6),
        Vec2::new(width, (ui.max_rect().height() - theme::SPACE_6).max(0.0)),
    );
    let mut card = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("issue-gone", path))
            .max_rect(rect)
            .layout(Layout::top_down(egui::Align::Min)),
    );
    let mut action = None;
    let card = Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_soft))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS_MD))
        .show(&mut card, |ui| {
            ui.set_width(width);
            if let Some(art) = notice.art {
                picture(ui, art, width);
            }
            Frame::new()
                .inner_margin(MarginF32 {
                    left: theme::SPACE_7,
                    right: theme::SPACE_7,
                    top: theme::SPACE_6,
                    bottom: theme::SPACE_7,
                })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    title(ui, notice);
                    ui.add_space(theme::SPACE_3);
                    let sentence = sentence(ui, &notice.file, &notice.sentence);
                    ui.label(sentence);
                    if notice.note {
                        ui.add_space(theme::SPACE_3);
                        ui.label(
                            RichText::new("Its agent log is still open beside it.")
                                .font(theme::meta())
                                .color(p.ink_muted),
                        );
                    }
                    ui.add_space(theme::SPACE_5);
                    action = actions(ui, notice, focus);
                });
        })
        .response;
    card.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Section, true, &notice.title));
    action
}

/// The picture at the top of the card, 7:2 and cropped.
fn picture(ui: &mut egui::Ui, art: Art, width: f32) {
    let p = theme::palette(ui);
    let height = width * 2.0 / 7.0;
    let dark = ui.visuals().dark_mode;
    let source = match (art, dark) {
        (Art::Moved, false) => {
            egui::include_image!(
                "../../../../design/components/IssueGone/files/issue-moved-light.jpg"
            )
        }
        (Art::Moved, true) => {
            egui::include_image!(
                "../../../../design/components/IssueGone/files/issue-moved-dark.jpg"
            )
        }
        (Art::Gone, false) => {
            egui::include_image!(
                "../../../../design/components/IssueGone/files/issue-gone-light.jpg"
            )
        }
        (Art::Gone, true) => {
            egui::include_image!(
                "../../../../design/components/IssueGone/files/issue-gone-dark.jpg"
            )
        }
    };
    let image = ui.add(
        egui::Image::new(source)
            .uv(egui::Rect::from_min_max(
                egui::pos2(0.0, 1.0 / 6.0),
                egui::pos2(1.0, 5.0 / 6.0),
            ))
            .fit_to_exact_size(Vec2::new(width, height))
            .maintain_aspect_ratio(false),
    );
    ui.painter().hline(
        image.rect.x_range(),
        image.rect.max.y,
        Stroke::new(1.0, p.line_soft),
    );
}

/// The title, with the destination column's icon beside it. It is painted, not
/// a label, so the card's section is the one node carrying its title.
fn title(ui: &mut egui::Ui, notice: &Notice) {
    let p = theme::palette(ui);
    let text = egui::WidgetText::from(
        RichText::new(&notice.title)
            .font(theme::read_h2())
            .color(p.ink),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    );
    let icon_width = if notice.status.is_some() {
        theme::STATUS_ICON + theme::SPACE_3
    } else {
        0.0
    };
    let height = text.size().y.max(theme::STATUS_ICON);
    let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let mut x = row.min.x;
    if let Some(status) = notice.status {
        let icon = Rect::from_min_size(
            egui::pos2(x, row.center().y - theme::STATUS_ICON / 2.0),
            Vec2::splat(theme::STATUS_ICON),
        );
        theme::status_icon(ui.painter(), status, icon, p);
        x += icon_width;
    }
    ui.painter().galley(
        egui::pos2(x, row.center().y - text.size().y / 2.0),
        text,
        p.ink,
    );
}

/// The sentence, the file name in `ink` and the rest in `ink-muted`.
fn sentence(ui: &mut egui::Ui, file: &str, rest: &str) -> egui::text::LayoutJob {
    let p = theme::palette(ui);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width().min(theme::READ_MEASURE);
    job.wrap.break_anywhere = file.chars().count() > 32;
    job.append(
        file,
        0.0,
        egui::TextFormat::simple(theme::ui_strong(), p.ink),
    );
    job.append(
        rest,
        0.0,
        egui::TextFormat::simple(theme::ui_body(), p.ink_muted),
    );
    job
}

/// The card's buttons: the way to follow the issue first, then the way out.
fn actions(ui: &mut egui::Ui, notice: &Notice, focus: bool) -> Option<Action> {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_3);
        if let Some(to) = &notice.destination {
            let label = notice
                .title
                .strip_prefix("Moved to ")
                .map_or_else(|| "Open".to_string(), |name| format!("Open in {name}"));
            let open = widgets::button(ui, ButtonKind::Primary, None, &label, Some("↩"));
            if focus {
                open.request_focus();
            }
            if open.clicked() {
                action = Some(Action::Open(to.clone()));
            }
            if widgets::button(
                ui,
                ButtonKind::Secondary,
                None,
                "Back to board",
                Some("esc"),
            )
            .clicked()
            {
                action = Some(Action::Board);
            }
        } else {
            let back = widgets::button(ui, ButtonKind::Primary, None, "Back to board", Some("esc"));
            if focus {
                back.request_focus();
            }
            if back.clicked() {
                action = Some(Action::Board);
            }
        }
    });
    action
}

/// The folder's name as the sentence names it, without its slash.
fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || "issues".to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}
