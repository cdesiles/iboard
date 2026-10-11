//! The reader: an issue's page, its sheet head, and what it still lacks.

use super::*;

impl IboardApp {
    pub(super) fn detail_ui(&mut self, ui: &mut egui::Ui, path: &Path) {
        if self.board.find(path).is_none() {
            page(ui, |ui| {
                ui.label(
                    RichText::new(
                        "This issue no longer exists. It may have been moved or deleted.",
                    )
                    .color(theme::palette(ui).ink_muted),
                );
            });
            return;
        }

        if self
            .review
            .as_ref()
            .is_some_and(|review| review.path == path)
        {
            let p = theme::palette(ui);
            egui::Panel::right("review")
                .exact_size(REVIEW_PANEL_WIDTH)
                .resizable(false)
                .frame(Frame::new().fill(p.bg).inner_margin(Margin {
                    left: theme::SPACE_5 as i8,
                    right: theme::SPACE_5 as i8,
                    top: theme::SPACE_5 as i8,
                    bottom: theme::SPACE_5 as i8,
                }))
                .show(ui, |ui| self.review_panel(ui));
        }

        match self.detail_mode {
            DetailMode::Preview => self.detail_preview(ui, path),
            DetailMode::Edit => self.detail_edit(ui, path),
        }
    }

    /// The reader sheet: a `surface` page that reaches down to the status bar.
    /// A meta head (column, path, modified, checklist progress or words), the
    /// rendered markdown, ghost blocks for missing standard sections, and the
    /// empty rest of the sheet, which edits at the end of the file.
    /// Checkboxes are live: ticking one writes the file. Double-clicking the
    /// text opens the editor.
    pub(super) fn detail_preview(&mut self, ui: &mut egui::Ui, path: &Path) {
        let Some(issue) = self.board.find(path) else {
            return;
        };
        // The title's `[label]` groups show as tags above it, not as text.
        let (mut text, title_line) = sheet::hide_title_labels(&issue.markdown);
        let modified = issue.modified;
        let issue_labels = issue.labels.clone();
        let p = theme::palette(ui);
        let (column_name, status, todo_issue) = self
            .board
            .column_of(path)
            .map(|column| {
                (
                    column.name.clone(),
                    status_color(p, column),
                    column.accepts_new_issues(),
                )
            })
            .unwrap_or_else(|| (String::new(), p.status_todo, false));
        let review_ready = self.agent.configured();
        // Doing and Done tickets open read-only: no click-to-edit affordance.
        let editable = self.can_edit(path);
        let relative = self.relative(path);
        let (ticked, tasks) = sheet::checklist(&text);
        let words = sheet::word_count(&text);
        let sections = &self.board.reader_sections;
        // Only Todo issues are still being written: Ready, Doing and Done
        // ones never get the "Not written yet" block.
        let missing: Vec<String> = if !todo_issue {
            Vec::new()
        } else {
            sheet::missing_sections(&text, sections)
                .into_iter()
                .map(str::to_string)
                .collect()
        };
        let none_written = !missing.is_empty() && missing.len() == sections.len();

        // The rendered reader is not a text surface: keep labels inert so a
        // double-click reliably opens the editor instead of selecting a word.
        ui.style_mut().interaction.selectable_labels = false;

        // Two blocks: the sheet, and under it the "Not written yet" block,
        // docked at the bottom of the window. The sheet takes the spare
        // height; the block stays put while a long issue scrolls.
        let full = ui.available_rect_before_wrap();
        let gutter = theme::SPACE_5;
        let page_width = (full.width() - 2.0 * gutter).min(theme::READER_MAX);
        let page_left = full.min.x + ((full.width() - page_width) / 2.0).max(gutter);
        let tiles_across = if page_width >= 560.0 { 4 } else { 2 }.min(missing.len().max(1));
        let block_height = if none_written {
            let rows = missing.len().div_ceil(tiles_across) as f32;
            2.0 * theme::SPACE_4
                + theme::CONTROL_H
                + theme::SPACE_3
                + rows * GHOST_TILE_H
                + (rows - 1.0) * theme::SPACE_3
        } else if !missing.is_empty() {
            40.0
        } else {
            0.0
        };
        let block_gap = if block_height > 0.0 {
            theme::SPACE_4
        } else {
            0.0
        };
        let scroll_height = (full.height() - block_height - block_gap).max(120.0);
        // The sheet fills its share between the header and the block.
        let sheet_height = scroll_height - 2.0 * gutter;
        let mut body_rect = Rect::NOTHING;
        let mut add_section: Option<String> = None;
        let mut draft_review = false;
        let mut edit_at_end = false;
        let mut changed = false;
        let mut label_clicked: Option<String> = None;
        {
            let cache = &mut self.cache;
            let labels = &self.labels;
            egui::ScrollArea::vertical()
                .id_salt(("detail", path))
                .auto_shrink([false, false])
                .max_height(scroll_height)
                .show(ui, |ui| {
                    page(ui, |ui| {
                        let top = ui.cursor().min.y;
                        let inner_height = sheet_height - 2.0 * theme::SPACE_6 - 2.0;

                        sheet_head(
                            ui,
                            SheetHead {
                                column: &column_name,
                                status,
                                path: &relative,
                                modified,
                                checklist: (ticked, tasks),
                                words,
                            },
                        );

                        // The labels, on their own row between the head and
                        // the title; a click filters the board by one.
                        if !issue_labels.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_2);
                                for name in &issue_labels {
                                    if widgets::label_pill(ui, name, labels.color(name))
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text(format!("Filter by {name}"))
                                        .clicked()
                                    {
                                        label_clicked = Some(name.clone());
                                    }
                                }
                            });
                        }

                        let body = ui.vertical(|ui| {
                            // The body spans the sheet, so a short issue still
                            // gets an outline wide enough for its legend.
                            ui.set_min_width(ui.available_width());
                            // Reading sizes: `read-body` 14, `read-h1` 20.
                            let styles = &mut ui.style_mut().text_styles;
                            styles.insert(egui::TextStyle::Body, theme::read_body());
                            styles.insert(egui::TextStyle::Heading, theme::read_h1());
                            styles
                                .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.5));
                            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                            CommonMarkViewer::new()
                                .default_implicit_uri_scheme(image_base_uri(path))
                                .show_mut(ui, cache, &mut text)
                                .response
                                .changed()
                        });
                        body_rect = body.response.rect;
                        changed = body.inner;

                        // The rest of the sheet: empty, and a click edits at
                        // the end of the file.
                        let used = ui.cursor().min.y - top;
                        let rest_height = (inner_height - used).max(56.0);
                        let (rest, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), rest_height),
                            Sense::click(),
                        );
                        if editable {
                            let response = response.on_hover_cursor(egui::CursorIcon::Text);
                            edit_at_end = response.clicked();
                            paint_rest_hint(ui, rest);
                        }
                    });
                });
        }

        if block_height > 0.0 {
            let rect = Rect::from_min_size(
                Pos2::new(page_left, full.max.y - gutter - block_height),
                Vec2::new(page_width, block_height),
            );
            let mut block = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            let picked = not_written_block(
                &mut block,
                rect,
                &missing,
                none_written,
                tiles_across,
                review_ready,
            );
            match picked {
                Some(NotWritten::Section(section)) => add_section = Some(section),
                Some(NotWritten::Review) => draft_review = true,
                None => {}
            }
        }

        // Hovering the text shows that it is one edit target: a `line-strong`
        // outline with the hint on its top edge.
        let outline = body_rect.expand(theme::SPACE_3);
        if editable && ui.rect_contains_pointer(outline) {
            let painter = ui.painter();
            painter.rect_stroke(
                outline,
                CornerRadius::same(theme::RADIUS_SM),
                Stroke::new(1.0, p.line_strong),
                StrokeKind::Outside,
            );
            let hint = egui::RichText::new("Double-click to edit")
                .font(egui::FontId::new(11.0, theme::ui_button().family))
                .color(p.ink_muted);
            let galley = egui::WidgetText::from(hint).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::FontSelection::Default,
            );
            let kbd_width = 18.0;
            let right = outline.max.x - theme::SPACE_3;
            let y = outline.min.y;
            let legend = Rect::from_min_max(
                Pos2::new(
                    right - kbd_width - theme::SPACE_2 - galley.size().x - theme::SPACE_2,
                    y - 8.0,
                ),
                Pos2::new(right + theme::SPACE_2, y + 8.0),
            );
            painter.rect_filled(legend, CornerRadius::ZERO, p.surface);
            let kbd_left = right - kbd_width;
            widgets::paint_kbd(ui, "E", Pos2::new(kbd_left, y));
            let text_pos = Pos2::new(
                kbd_left - theme::SPACE_2 - galley.size().x,
                y - galley.size().y / 2.0,
            );
            ui.painter().galley(text_pos, galley, p.ink_muted);
        }
        self.preview_rect = outline;

        if changed {
            if let Some((index, line)) = &title_line {
                text = sheet::restore_line(&text, *index, line);
            }
            self.write_issue(path, &text);
        }
        if let Some(label) = label_clicked {
            self.filter.pick(&label);
            self.back_to_board();
        } else if let Some(section) = add_section {
            self.add_section(path, &section);
        } else if edit_at_end {
            self.start_editing(path);
            self.focus_editor = true;
        } else if draft_review {
            self.start_review(path, false);
        }
    }
}

/// The `file://` base a relative image link in `issue` resolves against: the
/// issue's own folder, so `assets/<stem>/ui.png` finds the mockup written beside
/// it. Passed to the markdown viewer as its implicit URI scheme.
pub(super) fn image_base_uri(issue: &Path) -> String {
    let dir = issue.parent().unwrap_or(Path::new("."));
    format!("file://{}/", dir.display())
}

/// Lays `add_contents` out in a column `reader-max` wide, centred, with the
/// window gutter around it.
pub(super) fn page_column<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let gutter = theme::SPACE_5;
    let width = (ui.available_width() - 2.0 * gutter).min(theme::READER_MAX);
    let left = ((ui.available_width() - width) / 2.0).max(gutter);
    let rect = Rect::from_min_size(
        ui.cursor().min + Vec2::new(left, gutter),
        Vec2::new(width, (ui.available_height() - 2.0 * gutter).max(0.0)),
    );
    let result = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
            add_contents,
        )
        .inner;
    ui.add_space(gutter);
    result
}

/// The "Not written yet" block under the reader sheet: its own `surface`
/// card. With nothing written, a head row (label, Draft with Review) over a
/// grid of ghost tiles; once some sections exist, one line of chips.
pub(super) fn not_written_block(
    ui: &mut egui::Ui,
    rect: Rect,
    missing: &[String],
    none_written: bool,
    across: usize,
    review_ready: bool,
) -> Option<NotWritten> {
    let p = theme::palette(ui);
    ui.painter().rect(
        rect,
        CornerRadius::same(theme::RADIUS_MD),
        p.surface,
        Stroke::new(1.0, p.line),
        StrokeKind::Inside,
    );
    let inner = rect.shrink2(Vec2::new(theme::SPACE_5, theme::SPACE_4));
    let mut ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    let mut picked = None;
    let label = |ui: &mut egui::Ui| {
        ui.label(
            RichText::new("NOT WRITTEN YET")
                .font(theme::ui_label())
                .extra_letter_spacing(theme::LABEL_TRACKING)
                .color(p.ink_muted),
        );
    };
    if !none_written {
        // One 40pt line: the label, then a chip per missing section.
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            label(ui);
            for section in missing {
                if widgets::chip(ui, &format!("+ {section}"), false)
                    .on_hover_text(format!("Add ## {section} in the editor"))
                    .clicked()
                {
                    picked = Some(NotWritten::Section(section.clone()));
                }
            }
        });
        return picked;
    }

    ui.allocate_ui_with_layout(
        Vec2::new(inner.width(), theme::CONTROL_H),
        Layout::left_to_right(Align::Center),
        |ui| {
            label(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let kind = if review_ready {
                    ButtonKind::Review
                } else {
                    ButtonKind::Disabled
                };
                let response = widgets::button(
                    ui,
                    kind,
                    Some(Icon::Review),
                    "Draft with Review",
                    Some("⌘⇧R"),
                );
                if review_ready {
                    if response
                        .on_hover_text("Ask the review agent to propose these sections")
                        .clicked()
                    {
                        picked = Some(NotWritten::Review);
                    }
                } else {
                    response.on_hover_text(
                        "No agent is set up yet: choose one in Settings › AI review (⌘,)",
                    );
                }
            });
        },
    );
    ui.add_space(theme::SPACE_3);
    let gap = theme::SPACE_3;
    let tile_w = (inner.width() - gap * (across as f32 - 1.0)) / across as f32;
    let top = ui.cursor().min.y;
    for (index, section) in missing.iter().enumerate() {
        let (row, col) = (index / across, index % across);
        let tile = Rect::from_min_size(
            Pos2::new(
                inner.min.x + col as f32 * (tile_w + gap),
                top + row as f32 * (GHOST_TILE_H + gap),
            ),
            Vec2::new(tile_w, GHOST_TILE_H),
        );
        let mut cell = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(tile)
                .layout(Layout::top_down(Align::Min)),
        );
        if widgets::ghost_section(&mut cell, section, sheet::section_hint(section))
            .on_hover_text(format!("Add ## {section} in the editor"))
            .clicked()
        {
            picked = Some(NotWritten::Section(section.clone()));
        }
    }
    picked
}

/// The reader's `surface` page: `reader-max` wide, centred, 24×32 padding.
pub(super) fn page<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    page_column(ui, |ui| {
        let p = theme::palette(ui);
        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(Margin::symmetric(
                theme::SPACE_7 as i8,
                theme::SPACE_6 as i8,
            ))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add_contents(ui)
            })
    })
}

/// The sheet's `meta` head: the column as a status mark and the file path on
/// the left, "modified 1h ago" and checklist progress (or the word count) on
/// the right, over a `line` rule.
pub(super) fn sheet_head(ui: &mut egui::Ui, head: SheetHead) {
    let p = theme::palette(ui);
    let meta = |text: String, color: Color32| {
        egui::Label::new(RichText::new(text).font(theme::meta()).color(color))
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if !head.column.is_empty() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, head.status);
                ui.label(
                    RichText::new(head.column.to_uppercase())
                        .font(theme::ui_label())
                        .extra_letter_spacing(theme::LABEL_TRACKING)
                        .color(p.ink),
                );
            });
            ui.add(meta("·".into(), p.ink_faint));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (ticked, total) = head.checklist;
            if total > 0 {
                ui.add(meta(format!("{ticked}/{total} done"), p.ink_muted));
                ui.spacing_mut().item_spacing.x = 6.0;
                widgets::meter(ui, ticked, total);
                ui.spacing_mut().item_spacing.x = 10.0;
            } else {
                let unit = if head.words == 1 { "word" } else { "words" };
                ui.add(meta(format!("{} {unit}", head.words), p.ink_muted));
            }
            if let Some(modified) = head.modified {
                ui.add(meta("·".into(), p.ink_faint));
                ui.add(meta(
                    format!("modified {}", relative_time(modified)),
                    p.ink_muted,
                ));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(meta(head.path.to_string(), p.ink_muted).truncate());
            });
        });
    });
    ui.add_space(theme::SPACE_4);
    let (rule, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(rule.x_range(), rule.center().y, Stroke::new(1.0, p.line));
    ui.add_space(20.0);
}

/// "Click below the text to keep writing E" in `ink-faint`, at the foot of
/// the empty rest of the sheet.
pub(super) fn paint_rest_hint(ui: &egui::Ui, rest: Rect) {
    let p = theme::palette(ui);
    let text = egui::WidgetText::from(
        RichText::new("Click below the text to keep writing")
            .font(theme::ui_small())
            .color(p.ink_faint),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    );
    let kbd_width = 18.0;
    let gap = 6.0;
    let width = theme::ICON_SIZE + gap + text.size().x + gap + kbd_width;
    let y = rest.max.y - 8.0;
    let mut x = rest.center().x - width / 2.0;
    let icon = Rect::from_min_size(
        Pos2::new(x, y - theme::ICON_SIZE / 2.0),
        Vec2::splat(theme::ICON_SIZE),
    );
    theme::paint_icon(ui.painter(), Icon::Edit, icon, p.ink_faint);
    x += theme::ICON_SIZE + gap;
    ui.painter().galley(
        Pos2::new(x, y - text.size().y / 2.0),
        text.clone(),
        p.ink_faint,
    );
    x += text.size().x + gap;
    widgets::paint_kbd(ui, "E", Pos2::new(x, y));
}
