//! The bell: what needs the author, and the list under it. Questions come
//! from the build service; the runs that finished while the board was away
//! are kept beside it, until they are read.

use std::path::PathBuf;
use std::time::SystemTime;

use super::{
    Align, ButtonKind, Frame, IboardApp, Icon, Layout, Path as AppPath, RichText, Sense, Vec2,
    file_name, theme, widgets,
};
use crate::format::relative_time;
use iboard::notifications::{Kind, Notification};

/// What a row says happened.
#[derive(Clone, Copy)]
enum RowKind {
    Question,
    Notice(Kind),
}

/// One row of the list, live or kept.
struct Row {
    path: PathBuf,
    title: String,
    detail: String,
    at: Option<SystemTime>,
    kind: RowKind,
    read: bool,
}

impl IboardApp {
    /// How many things need the author: unanswered questions, then the
    /// notices not read yet.
    pub(super) fn notification_count(&self) -> usize {
        self.pending_questions().len()
            + self
                .session
                .notifications
                .iter()
                .filter(|notice| !notice.read)
                .count()
    }

    /// The bell in the top bar, and the list under it. A click on a row opens
    /// its issue, where a question is answered and a log is read.
    pub(super) fn notifications_button(&mut self, ui: &mut egui::Ui) {
        let count = self.notification_count();
        let button = widgets::button(ui, ButtonKind::Ghost, Some(Icon::Bell), "", None)
            .on_hover_text("Notifications");
        let name = if count == 0 {
            "Notifications".to_string()
        } else {
            format!("Notifications, {count} unread")
        };
        button
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &name));
        if count > 0 {
            let p = theme::palette(ui);
            let dot = egui::pos2(button.rect.right() - 4.0, button.rect.top() + 4.0);
            ui.painter().circle_filled(dot, 3.8, p.bg);
            ui.painter().circle_filled(dot, 2.4, p.accent);
        }

        let mut picked = None;
        let mut mark_all = false;
        egui::Popup::from_toggle_button_response(&button)
            .width(420.0)
            .align(egui::RectAlign::BOTTOM_END)
            .show(|ui| {
                ui.set_min_width(420.0);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                (picked, mark_all) = self.notifications_body(ui);
            });
        if let Some(path) = picked {
            self.open_issue(path);
        }
        if mark_all {
            self.mark_all_notifications_read();
        }
    }

    /// The list: waiting questions first, then the recent notices.
    fn notifications_body(&self, ui: &mut egui::Ui) -> (Option<PathBuf>, bool) {
        let p = theme::palette(ui);
        let width = ui.available_width();
        let mut picked = None;
        let mut mark_all = false;
        Frame::new()
            .inner_margin(Vec2::new(theme::SPACE_4, theme::SPACE_3))
            .show(ui, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Notifications")
                            .font(theme::ui_strong())
                            .color(p.ink),
                    );
                    let waiting = self.pending_questions().len()
                        + self
                            .session
                            .notifications
                            .iter()
                            .filter(|notice| !notice.read)
                            .count();
                    if waiting > 0 {
                        ui.label(
                            RichText::new(waiting.to_string())
                                .font(theme::meta())
                                .color(p.ink_muted),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let unread = self.session.notifications.iter().any(|notice| !notice.read);
                        if unread
                            && widgets::button(
                                ui,
                                ButtonKind::Ghost,
                                None,
                                "Mark recent read",
                                None,
                            )
                            .clicked()
                        {
                            mark_all = true;
                        }
                    });
                });
            });
        widgets::rule(ui);
        let rows = self.notification_rows();
        if rows.is_empty() {
            Frame::new()
                .inner_margin(Vec2::new(theme::SPACE_4, theme::SPACE_5))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Nothing needs you.")
                            .font(theme::ui_small())
                            .color(p.ink_muted),
                    );
                });
            return (None, false);
        }
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let questions: Vec<&Row> = rows
                    .iter()
                    .filter(|row| matches!(row.kind, RowKind::Question))
                    .collect();
                let notices: Vec<&Row> = rows
                    .iter()
                    .filter(|row| matches!(row.kind, RowKind::Notice(_)))
                    .collect();
                if !questions.is_empty() {
                    widgets::menu_group(ui, "Waiting on you");
                    for row in questions {
                        if notification_row(ui, row).clicked() {
                            picked = Some(row.path.clone());
                        }
                    }
                }
                if !notices.is_empty() {
                    widgets::menu_group(ui, "Recent");
                    for row in notices {
                        if notification_row(ui, row).clicked() {
                            picked = Some(row.path.clone());
                        }
                    }
                }
            });
        (picked, mark_all)
    }

    /// The rows the list shows: the questions the service waits on, then the
    /// notices kept beside the board.
    fn notification_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (path, question) in self.pending_questions() {
            let title = self
                .board
                .find(&path)
                .map_or_else(|| file_name(&path), |issue| issue.title.clone());
            rows.push(Row {
                path,
                title,
                detail: question.title.clone(),
                at: None,
                kind: RowKind::Question,
                read: false,
            });
        }
        for notice in &self.session.notifications {
            rows.push(Row {
                path: notice.path.clone(),
                title: notice.title.clone(),
                detail: notice.detail.clone(),
                at: Some(notice.at),
                kind: RowKind::Notice(notice.kind),
                read: notice.read,
            });
        }
        rows
    }

    /// Loads the notices of every project on the board, once per board.
    pub(super) fn sync_notifications(&mut self) {
        let mut roots = vec![self.board.root.clone()];
        roots.extend(self.board.repos.iter().map(|repo| repo.root.clone()));
        roots.sort();
        roots.dedup();
        if self.session.notification_roots.as_ref() == Some(&roots) {
            return;
        }
        self.session.notifications = roots
            .iter()
            .flat_map(|root| iboard::notifications::load(root))
            .collect();
        self.session
            .notifications
            .sort_by_key(|notice| std::cmp::Reverse(notice.at));
        self.session.notification_roots = Some(roots);
    }

    /// Adds a notice for what just finished, when Settings › General keeps it.
    pub(super) fn notify(&mut self, kind: Kind, path: &AppPath, detail: String) {
        if !self.prefs.notifications.wants(kind) {
            return;
        }
        let Some(issue) = self.board.find(path) else {
            return;
        };
        let at = SystemTime::now();
        let stamp = chrono::DateTime::<chrono::Utc>::from(at).timestamp_millis();
        let notice = Notification {
            id: format!("{}:{}:{stamp}", kind.key(), path.display()),
            kind,
            path: path.to_path_buf(),
            title: issue.title.clone(),
            detail,
            at,
            read: false,
        };
        self.session
            .notifications
            .retain(|kept| kept.id != notice.id);
        self.session.notifications.push(notice);
        self.session
            .notifications
            .sort_by_key(|notice| std::cmp::Reverse(notice.at));
        self.session.notifications.truncate(40);
        self.persist_notifications(self.board.root_of(path).to_path_buf());
    }

    /// Marks every notice of `path` read: the issue was opened.
    pub(super) fn mark_notifications_read(&mut self, path: &AppPath) {
        let mut changed = false;
        for notice in &mut self.session.notifications {
            if notice.path == path && !notice.read {
                notice.read = true;
                changed = true;
            }
        }
        if changed {
            self.persist_notifications(self.board.root_of(path).to_path_buf());
        }
    }

    fn mark_all_notifications_read(&mut self) {
        let mut roots = Vec::new();
        for notice in &mut self.session.notifications {
            if !notice.read {
                notice.read = true;
                let root = self.board.root_of(&notice.path).to_path_buf();
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
        }
        for root in roots {
            self.persist_notifications(root);
        }
    }

    /// Writes one project's notices: the ones kept under its issues root.
    fn persist_notifications(&mut self, root: PathBuf) {
        let items: Vec<Notification> = self
            .session
            .notifications
            .iter()
            .filter(|notice| self.board.root_of(&notice.path) == root)
            .cloned()
            .collect();
        if let Err(error) = iboard::notifications::save(&root, &items) {
            self.error = Some(error);
        }
    }
}

/// One two-line row: what happened on the left, when on the right, and the
/// unread dot.
fn notification_row(ui: &mut egui::Ui, row: &Row) -> egui::Response {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 56.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("{}: {}", row.title, row.detail),
        )
    });
    if ui.is_rect_visible(rect) {
        let hover = widgets::hover_fade(ui.ctx(), response.id, response.hovered());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(theme::RADIUS_SM),
            p.surface_hover.gamma_multiply(hover),
        );
        let (icon, color) = match row.kind {
            RowKind::Question => (Icon::Bubble, p.status_doing),
            RowKind::Notice(Kind::BuildDone | Kind::AskDone) => (Icon::Log, p.status_done),
            RowKind::Notice(Kind::BuildFailed) => (Icon::Warning, p.danger),
            RowKind::Notice(Kind::ReviewReady) => (Icon::Review, p.review),
        };
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 18.0, rect.center().y),
            Vec2::splat(theme::ICON_SIZE),
        );
        theme::paint_icon(ui.painter(), icon, icon_rect, color);
        let time = match row.kind {
            RowKind::Question => "waiting".to_string(),
            RowKind::Notice(_) => row.at.map(relative_time).unwrap_or_default(),
        };
        let time_ink = match row.kind {
            RowKind::Question => p.status_doing,
            RowKind::Notice(_) => p.ink_faint,
        };
        let time = line(ui, &time, theme::meta(), time_ink, 64.0);
        let left = rect.left() + 38.0;
        let right = rect.right() - 12.0 - time.size().x - 8.0;
        let title = line(ui, &row.title, theme::ui_strong(), p.ink, right - left);
        ui.painter()
            .galley(egui::pos2(left, rect.top() + 10.0), title, p.ink);
        let detail = line(
            ui,
            &row.detail,
            theme::ui_small(),
            p.ink_muted,
            right - left,
        );
        ui.painter()
            .galley(egui::pos2(left, rect.top() + 29.0), detail, p.ink_muted);
        ui.painter().galley(
            egui::pos2(rect.right() - 12.0 - time.size().x, rect.top() + 10.0),
            time,
            time_ink,
        );
        if !row.read {
            ui.painter().circle_filled(
                egui::pos2(rect.right() - 10.0, rect.center().y),
                2.5,
                p.accent,
            );
        }
    }
    widgets::focus_ring(ui, &response, theme::RADIUS_SM);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `text` on one line, cut with an ellipsis at `width`.
fn line(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap.max_width = width.max(0.0);
    job.wrap.max_rows = 1;
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_row_keeps_the_title_and_the_detail() {
        let row = Row {
            path: PathBuf::from("doing/a.md"),
            title: "Create test issue".into(),
            detail: "Questions".into(),
            at: None,
            kind: RowKind::Question,
            read: false,
        };
        assert_eq!(row.title, "Create test issue");
        assert_eq!(row.detail, "Questions");
    }
}
