//! The filter field in the header bar, and its keys.

use super::*;

impl IboardApp {
    /// The header filter: 220pt at rest, 480pt while in use or filtering,
    /// growing to the left. Focused, it offers the suggestions as chips; a
    /// picked one becomes a token at the start of the field.
    pub(super) fn filter_field(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        let id = filter_id();
        let focused = ctx.memory(|m| m.has_focus(id));
        if !focused {
            self.chip_cursor = None;
        }
        let open = focused || self.filter_open;
        let active = !self.filter.is_empty();

        let target = if open || active {
            FILTER_OPEN
        } else {
            FILTER_REST
        };
        let width = ctx
            .animate_value_with_time(
                id.with("width"),
                target,
                theme::motion(&ctx, FILTER_GROW_SECS),
            )
            .min(ui.available_width() - CRUMB_KEEP)
            .max(FILTER_REST);
        let (rect, frame) =
            ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::click());
        // A press on a chip or a token's × takes focus from the text for a
        // frame; the field stays open through it.
        let pressed_inside = ctx.input(|i| {
            (i.pointer.any_down() || i.pointer.any_released())
                && i.pointer
                    .interact_pos()
                    .is_some_and(|pos| rect.contains(pos))
        });
        self.filter_open = focused || (self.filter_open && pressed_inside);
        if frame.clicked() {
            self.focus_filter = true;
        }

        let painter = ui.painter();
        let edge = if active && !open {
            p.accent
        } else {
            p.line_strong
        };
        painter.rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            p.surface,
            Stroke::new(1.0, edge),
            StrokeKind::Inside,
        );
        if open {
            painter.rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(theme::RADIUS_SM + 2),
                Stroke::new(2.0, p.accent),
                StrokeKind::Outside,
            );
        }

        let chips: Vec<String> = if open {
            let issues = self.board.columns.iter().flat_map(|column| &column.issues);
            filter::suggestions(self.suggestions.as_deref(), issues)
                .into_iter()
                .filter(|word| !self.filter.tokens.contains(word))
                .collect()
        } else {
            Vec::new()
        };
        let count = active.then(|| {
            let shown: usize = self.visible_grid().iter().map(Vec::len).sum();
            format!("{shown} of {}", self.board.issue_count())
        });

        let inner = Rect::from_min_max(
            rect.min + Vec2::new(theme::SPACE_3, 0.0),
            rect.max - Vec2::new(theme::SPACE_2, 0.0),
        );
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        row.spacing_mut().item_spacing.x = FILTER_GAP;
        let (icon, _) = row.allocate_exact_size(Vec2::splat(theme::ICON_SIZE), Sense::hover());
        theme::paint_icon(row.painter(), Icon::Search, icon, p.ink_muted);
        let mut removed = None;
        for (index, token) in self.filter.tokens.iter().enumerate() {
            if widgets::filter_token(&mut row, token).clicked() {
                removed = Some(index);
            }
        }

        // The chips hide when they would crowd the text.
        let try_label = self.filter.is_empty();
        let chips_width: f32 = chips
            .iter()
            .map(|word| widgets::text_width(&row, word, theme::meta()) + 14.0 + FILTER_GAP)
            .sum::<f32>()
            + if try_label {
                widgets::text_width(&row, "Labels", filter_try_font()) + FILTER_GAP
            } else {
                0.0
            };
        let count_width = count
            .as_ref()
            .map(|count| widgets::text_width(&row, count, theme::meta()) + FILTER_GAP)
            .unwrap_or(0.0);
        let text_width = widgets::text_width(&row, &self.filter.text, theme::ui_body());
        let chips = if row.available_width() - count_width - chips_width
            < text_width.max(FILTER_TEXT_MIN)
        {
            Vec::new()
        } else {
            chips
        };

        if focused {
            self.filter_keys(&ctx, &chips);
        }
        let current = self.chip_cursor;
        let mut picked: Option<String> = None;
        let mut response = None;
        row.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = FILTER_GAP;
            for (index, word) in chips.iter().enumerate().rev() {
                if widgets::chip(ui, word, current == Some(index)).clicked() {
                    picked = Some(word.clone());
                }
            }
            if !chips.is_empty() && try_label {
                ui.label(
                    RichText::new("Labels")
                        .font(filter_try_font())
                        .color(p.ink_muted),
                );
            }
            if let Some(count) = &count {
                ui.label(RichText::new(count).font(theme::meta()).color(p.ink_muted));
            }
            if !open {
                let key = if active { "esc" } else { "⌘F" };
                let width = widgets::text_width(ui, key, theme::meta()) + 8.0;
                let (cap, _) = ui.allocate_exact_size(Vec2::new(width, 16.0), Sense::hover());
                widgets::paint_kbd(ui, key, Pos2::new(cap.min.x, cap.center().y));
            }
            let hint = if self.filter.tokens.is_empty() {
                "Filter issues"
            } else {
                ""
            };
            response = Some(
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter.text)
                        .id(id)
                        .frame(Frame::NONE)
                        .hint_text(RichText::new(hint).color(p.ink_faint))
                        .font(theme::ui_body())
                        .margin(Margin::ZERO)
                        .desired_width(ui.available_width()),
                ),
            );
        });

        if let Some(index) = removed {
            self.filter.tokens.remove(index);
            self.focus_filter = true;
        }
        if let Some(word) = picked {
            self.filter.pick(&word);
            self.focus_filter = true;
        }
        if let Some(response) = response
            && std::mem::take(&mut self.focus_filter)
        {
            response.request_focus();
            self.filter_open = true;
        }
    }

    /// The keys the focused filter takes before its text does: `↓` reaches
    /// the chips, `← →` move between them, `↩` picks, `↑` goes back to the
    /// text, and `⌫` in an empty field removes the last token.
    pub(super) fn filter_keys(&mut self, ctx: &egui::Context, chips: &[String]) {
        let take = |key: Key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
        match self.chip_cursor {
            Some(_) if chips.is_empty() => self.chip_cursor = None,
            Some(at) => {
                let typed =
                    ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))));
                if typed || take(Key::ArrowUp) {
                    self.chip_cursor = None;
                } else if take(Key::ArrowLeft) {
                    self.chip_cursor = Some(at.saturating_sub(1));
                } else if take(Key::ArrowRight) {
                    self.chip_cursor = Some((at + 1).min(chips.len() - 1));
                } else if take(Key::Enter) {
                    self.filter.pick(&chips[at.min(chips.len() - 1)]);
                    // The picked chip leaves the row; stay on its neighbour.
                    self.chip_cursor = (chips.len() > 1).then(|| at.min(chips.len() - 2));
                } else {
                    take(Key::ArrowDown);
                }
            }
            None => {
                if !chips.is_empty() && take(Key::ArrowDown) {
                    self.chip_cursor = Some(0);
                } else if self.filter.text.is_empty()
                    && !self.filter.tokens.is_empty()
                    && take(Key::Backspace)
                {
                    self.filter.tokens.pop();
                }
            }
        }
    }
}

/// "Labels", before the suggestion chips.
pub(super) fn filter_try_font() -> egui::FontId {
    egui::FontId::proportional(11.0)
}

/// The board filter's text input.
pub(super) fn filter_id() -> egui::Id {
    egui::Id::new("filter")
}
