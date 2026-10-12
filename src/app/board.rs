//! The board: its columns of cards.

use super::*;

impl IboardApp {
    pub(super) fn board_ui(&mut self, ui: &mut egui::Ui) {
        if self.board.columns.is_empty() {
            Frame::new()
                .inner_margin(Margin::same(theme::SPACE_5 as i8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("No columns found in this board.")
                            .color(theme::palette(ui).ink_faint),
                    );
                });
            return;
        }

        // The board is a set of click/drag surfaces, not a text document: no
        // I-beam, no text selection anywhere on it.
        ui.style_mut().interaction.selectable_labels = false;

        let gutter = theme::SPACE_5;
        let count = self.board.columns.len() as f32;
        let available = ui.available_width() - 2.0 * gutter - (count - 1.0) * gutter;
        let width = (available / count).clamp(theme::COLUMN_MIN, theme::COLUMN_MAX);
        let height = (ui.available_height() - 2.0 * gutter).max(120.0);
        let filter = &self.filter;
        let labels = &self.labels;
        let new_card = self
            .new_card
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NEW_CARD_FLASH)
            .map(|(path, _)| path.clone());
        if new_card.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }

        let selected = self.selected.clone();
        let scroll_to_selected = std::mem::take(&mut self.scroll_to_selected);

        let mut clicked: Option<PathBuf> = None;
        let mut dropped: Option<(PathBuf, PathBuf, Option<PathBuf>)> = None;
        let mut delete: Option<PathBuf> = None;
        let mut archive: Option<PathBuf> = None;
        let mut compose = false;
        let mut submit: Option<bool> = None;
        let mut close_composer = false;
        let mut title_settings = false;
        let mut filter_label: Option<String> = None;
        let mut review: Option<PathBuf> = None;
        let mut show_review: Option<PathBuf> = None;
        let mut add_label: Option<PathBuf> = None;
        let mut focus_column: Option<PathBuf> = None;
        let mut archive_all: Vec<PathBuf> = Vec::new();

        egui::ScrollArea::horizontal()
            .id_salt("board")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(gutter);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gutter;
                    // The window gutter on the left; item spacing after the last
                    // column gives the same on the right.
                    ui.add_space(gutter);
                    for (index, column) in self.board.columns.iter().enumerate() {
                        let composer = if column.accepts_new_issues() {
                            self.composer.as_mut()
                        } else {
                            None
                        };
                        let view = ColumnView {
                            column,
                            index,
                            filter,
                            width,
                            height,
                            new_card: new_card.as_deref(),
                            selected: selected.as_deref(),
                            scroll_to_selected,
                            root: &self.board.root,
                            labels,
                            past_reviews: &self.past_reviews,
                            can_review: self.agent.configured(),
                        };
                        let actions = column_ui(ui, &view, composer);
                        if let Some(found) = actions.clicked {
                            clicked = Some(found);
                        }
                        if let Some(path) = actions.dropped {
                            dropped = Some((path, column.dir.clone(), actions.drop_anchor));
                        }
                        if delete.is_none() {
                            delete = actions.delete;
                        }
                        if archive.is_none() {
                            archive = actions.archive;
                        }
                        compose |= actions.compose;
                        submit = submit.or(actions.submit);
                        close_composer |= actions.close_composer;
                        title_settings |= actions.title_settings;
                        if filter_label.is_none() {
                            filter_label = actions.filter_label;
                        }
                        if review.is_none() {
                            review = actions.review;
                        }
                        if show_review.is_none() {
                            show_review = actions.show_review;
                        }
                        if add_label.is_none() {
                            add_label = actions.add_label;
                        }
                        if focus_column.is_none() {
                            focus_column = actions.focus_column;
                        }
                        if !actions.archive_all.is_empty() {
                            archive_all = actions.archive_all;
                        }
                    }
                    // Ends the scrollable width one gutter after the last column.
                    ui.allocate_space(Vec2::ZERO);
                });
            });

        if let Some((path, target, anchor)) = dropped {
            self.place_issue(&path, &target, anchor);
        }
        if let Some(path) = delete {
            self.request_delete(path);
        }
        if let Some(path) = archive {
            self.archive_issue(&path);
        }
        if let Some(path) = add_label {
            self.toggle_label_picker(path);
        }
        if compose {
            self.open_composer();
        }
        if let Some(close) = submit {
            self.submit_composer(close);
        } else if close_composer {
            self.composer = None;
        }
        if title_settings {
            self.open_title_settings();
        }
        if let Some(label) = filter_label {
            self.filter.pick(&label);
        }
        if let Some(dir) = focus_column {
            self.focus_column(dir);
        }
        if !archive_all.is_empty() {
            self.archive_all(&archive_all);
        }
        if let Some(path) = review {
            self.open_issue(path.clone());
            self.start_review(&path, false);
        } else if let Some(path) = show_review {
            self.show_past_review(path);
        }
        if let Some(path) = clicked {
            self.open_issue(path);
        }
    }
}

/// The status color of a column, from its folder's role.
pub(super) fn status_color(p: &Palette, column: &Column) -> Color32 {
    match column.name.to_lowercase().as_str() {
        "ready" => p.status_ready,
        "doing" | "in progress" | "in-progress" | "wip" | "review" => p.status_doing,
        "done" => p.status_done,
        _ => p.status_todo,
    }
}

/// The column header's title as a button (see the Column component): at rest
/// it is just the uppercase `ui-label`, on hover it takes a `surface-hover`
/// fill and shows a small expand icon in `ink-muted`. Clicking it expands the
/// column to the whole window (ColumnFocus); the tooltip names the shortcut.
pub(super) fn column_title_button(
    ui: &mut egui::Ui,
    name: &str,
    shortcut: Option<&str>,
) -> egui::Response {
    let p = theme::palette(ui);
    let label = name.to_uppercase();
    let text = widgets::text_width(ui, &label, theme::ui_label())
        + label.chars().count() as f32 * theme::LABEL_TRACKING;
    let pad = theme::SPACE_2;
    let gap = 6.0;
    let icon = 12.0;
    let width = pad * 2.0 + text + gap + icon;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::click());
    let tooltip = match shortcut {
        Some(key) => format!("Show {name} full screen ({key})"),
        None => format!("Show {name} full screen"),
    };
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
        }
        let mut job = LayoutJob::default();
        job.append(
            &label,
            0.0,
            egui::TextFormat {
                font_id: theme::ui_label(),
                color: p.ink,
                extra_letter_spacing: theme::LABEL_TRACKING,
                ..Default::default()
            },
        );
        let label_galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        painter.galley(
            Pos2::new(
                rect.min.x + pad,
                rect.center().y - label_galley.size().y / 2.0,
            ),
            label_galley,
            p.ink,
        );
        if response.hovered() {
            let icon_rect = Rect::from_center_size(
                Pos2::new(rect.max.x - pad - icon / 2.0, rect.center().y),
                Vec2::splat(icon),
            );
            theme::paint_icon(painter, Icon::Expand, icon_rect, p.ink_muted);
        }
    }
    response
}

pub(super) fn column_ui(
    ui: &mut egui::Ui,
    view: &ColumnView,
    composer: Option<&mut Composer>,
) -> ColumnActions {
    let ColumnView {
        column,
        index,
        filter,
        width,
        height,
        new_card,
        selected,
        scroll_to_selected,
        root,
        labels,
        past_reviews,
        can_review,
    } = *view;
    let p = theme::palette(ui);
    let mut actions = ColumnActions::default();
    let visible: Vec<&Issue> = column
        .issues
        .iter()
        .filter(|issue| filter.matches(issue))
        .collect();
    // `todo/`, or empty for the issues at the issues root.
    let folder = match column.dir.strip_prefix(root) {
        Ok(relative) if !relative.as_os_str().is_empty() => format!("{}/", relative.display()),
        _ => String::new(),
    };

    // Reserve the whole column as a drop target before drawing into it, so the
    // empty space below the cards accepts a drop too.
    let origin = ui.cursor().min;
    let drop_rect = Rect::from_min_size(origin, Vec2::new(width, height));
    // A double-click on the column (its header or empty space; cards take
    // their own clicks) expands it to the whole window, like its title.
    let drop_response = ui.interact(
        drop_rect,
        egui::Id::new(("column_drop", index)),
        Sense::click(),
    );
    if drop_response.double_clicked() {
        actions.focus_column = Some(column.dir.clone());
    }

    ui.allocate_ui_with_layout(
        Vec2::new(width, height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            let status = status_color(p, column);

            // Header: dot, uppercase name, count, Todo's `+`, 2pt status rule.
            let (head_rect, _) =
                ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::hover());
            let mut head = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(head_rect.shrink2(Vec2::new(theme::SPACE_2, 0.0)))
                    .layout(Layout::left_to_right(Align::Center)),
            );
            head.spacing_mut().item_spacing.x = theme::SPACE_3;
            let (dot, _) = head.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            head.painter().circle_filled(dot.center(), 4.0, status);
            // The title is a button: it expands the column to the whole window.
            let shortcut = (index < 4).then(|| ["⌘1", "⌘2", "⌘3", "⌘4"][index]);
            if column_title_button(&mut head, &column.name, shortcut).clicked() {
                actions.focus_column = Some(column.dir.clone());
            }
            head.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                if column.accepts_new_issues()
                    && widgets::button(ui, ButtonKind::Ghost, Some(Icon::Plus), "", None)
                        .on_hover_text("New issue in Todo (⌘N)")
                        .clicked()
                {
                    actions.compose = true;
                }
                // Filtered: "2/6", the total in `ink-faint`.
                let mut count = LayoutJob::default();
                let part = |color| egui::TextFormat::simple(theme::meta(), color);
                if filter.is_empty() {
                    count.append(&column.issues.len().to_string(), 0.0, part(p.ink_muted));
                } else {
                    count.append(&visible.len().to_string(), 0.0, part(p.ink_muted));
                    count.append(&format!("/{}", column.issues.len()), 0.0, part(p.ink_faint));
                }
                ui.label(count);
            });
            ui.painter().line_segment(
                [
                    Pos2::new(head_rect.min.x, head_rect.max.y - 1.0),
                    Pos2::new(head_rect.max.x, head_rect.max.y - 1.0),
                ],
                Stroke::new(2.0, status),
            );

            // The composer stays at the top of Todo; the cards scroll under it.
            if let Some(composer) = composer {
                composer_ui(ui, composer, column, &folder, &mut actions);
            }
            egui::ScrollArea::vertical()
                .id_salt(("column", index))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                    if visible.is_empty() {
                        let message = if column.issues.is_empty() {
                            if folder.is_empty() {
                                "No issues at the root".to_string()
                            } else {
                                format!("No issues in {folder}")
                            }
                        } else {
                            filter.empty_message()
                        };
                        empty_well(ui, &message, width);
                    }
                    let mut cards: Vec<(PathBuf, Rect)> = Vec::new();
                    let visible_paths: Vec<PathBuf> =
                        visible.iter().map(|issue| issue.path.clone()).collect();
                    for issue in visible {
                        let fresh = new_card == Some(issue.path.as_path());
                        let look = if selected == Some(issue.path.as_path()) {
                            CardLook::Selected {
                                scroll: scroll_to_selected,
                            }
                        } else if fresh {
                            CardLook::Fresh
                        } else {
                            CardLook::Plain
                        };
                        let pill = past_reviews.get(&issue.path).map(|past| past.card_pill(p));
                        let extras = CardExtras {
                            review_pill: pill,
                            review_button: can_review
                                && column.accepts_new_issues()
                                && matches!(look, CardLook::Selected { .. }),
                            ..CardExtras::default()
                        };
                        let (action, rect) = issue_card(ui, issue, width, look, labels, extras);
                        cards.push((issue.path.clone(), rect));
                        match action {
                            Some(CardAction::FilterLabel(label)) => {
                                actions.filter_label = Some(label);
                            }
                            Some(CardAction::Review) => {
                                actions.review = Some(issue.path.clone());
                            }
                            Some(CardAction::ShowReview) => {
                                actions.show_review = Some(issue.path.clone());
                            }
                            Some(CardAction::AddLabel) => {
                                actions.add_label = Some(issue.path.clone());
                            }
                            Some(CardAction::Open) => {
                                actions.clicked = Some(issue.path.clone());
                            }
                            Some(CardAction::Delete) => {
                                actions.delete = Some(issue.path.clone());
                            }
                            Some(CardAction::Archive) => {
                                actions.archive = Some(issue.path.clone());
                            }
                            None => {}
                        }
                    }

                    // At the end of a session, Done can be cleared at once:
                    // the issues it lists (after the filter) go to `.archive/`.
                    if column.name.eq_ignore_ascii_case("done") && visible_paths.len() > 1 {
                        ui.add_space(theme::SPACE_1);
                        let label = format!("Archive the {} issues above", visible_paths.len());
                        if widgets::button(
                            ui,
                            ButtonKind::Secondary,
                            Some(Icon::Archive),
                            &label,
                            None,
                        )
                        .on_hover_text(format!("Move them all to {ARCHIVE_DIR}/"))
                        .clicked()
                        {
                            actions.archive_all = visible_paths.clone();
                        }
                    }

                    // While the DropZones dock covers the bottom of the board,
                    // the last cards can still scroll out from under it.
                    if egui::DragAndDrop::has_payload_of_type::<IssueDrag>(ui.ctx()) {
                        ui.add_space(DOCK_H);
                    }

                    // Where the dragged card would land: before the first card
                    // whose middle is below the pointer, else last.
                    if drop_response.dnd_hover_payload::<IssueDrag>().is_some()
                        || drop_response.dnd_release_payload::<IssueDrag>().is_some()
                    {
                        let pointer = ui
                            .ctx()
                            .pointer_interact_pos()
                            .or(ui.ctx().pointer_hover_pos());
                        let insert = pointer
                            .map(|pos| {
                                cards
                                    .iter()
                                    .position(|(_, rect)| pos.y < rect.center().y)
                                    .unwrap_or(cards.len())
                            })
                            .unwrap_or(cards.len());
                        actions.drop_anchor = cards.get(insert).map(|(path, _)| path.clone());
                        let line_y = cards
                            .get(insert)
                            .map(|(_, rect)| rect.min.y - theme::SPACE_3 / 2.0)
                            .or_else(|| {
                                cards
                                    .last()
                                    .map(|(_, rect)| rect.max.y + theme::SPACE_3 / 2.0)
                            });
                        if let (Some(y), Some((_, first))) = (line_y, cards.first()) {
                            ui.painter().line_segment(
                                [Pos2::new(first.min.x, y), Pos2::new(first.max.x, y)],
                                Stroke::new(2.0, p.accent),
                            );
                        }
                    }
                });
        },
    );

    // Highlight the column while a card from elsewhere hovers over it: the
    // cards' area, under the header and its status rule.
    let hovering_foreign = drop_response
        .dnd_hover_payload::<IssueDrag>()
        .filter(|drag| drag.path.parent() != Some(column.dir.as_path()))
        .is_some();
    if hovering_foreign {
        let rect = drop_rect
            .with_min_y(drop_rect.min.y + theme::CONTROL_H + theme::SPACE_3 / 2.0)
            .expand2(Vec2::new(theme::SPACE_2, 0.0))
            .with_max_y(drop_rect.max.y + theme::SPACE_2);
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_MD),
            p.accent_soft.gamma_multiply(0.5),
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(theme::RADIUS_MD),
            Stroke::new(2.0, p.accent),
            StrokeKind::Inside,
        );
    }

    if let Some(drag) = drop_response.dnd_release_payload::<IssueDrag>() {
        actions.dropped = Some(drag.path.clone());
    }

    actions
}

/// The dashed `surface-sunken` well of an empty column. It is also where a
/// dragged card lands.
pub(super) fn empty_well(ui: &mut egui::Ui, message: &str, width: f32) {
    let p = theme::palette(ui);
    let response = Frame::new()
        .fill(p.surface_sunken)
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(Margin::same(theme::SPACE_4 as i8))
        .show(ui, |ui| {
            ui.set_width(width - 2.0 * theme::SPACE_4);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(message)
                        .font(theme::ui_small())
                        .color(p.ink_faint),
                );
            });
        })
        .response;
    dashed_rect(ui.painter(), response.rect, p.line_strong);
}

/// A 1pt dashed edge just inside `rect`, as on wells and drop zones.
pub(super) fn dashed_rect(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(0.5);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(Shape::dashed_line(
        &corners,
        Stroke::new(1.0, color),
        4.0,
        3.0,
    ));
}
