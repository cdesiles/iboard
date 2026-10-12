//! The filter field in the header bar, and its keys.

use super::*;

/// The filter field's state: the filter itself, and how the field is being
/// used.
#[derive(Default)]
pub(super) struct FilterBar {
    pub(super) filter: Filter,
    /// Give the text focus on the next frame.
    pub(super) focus: bool,
    /// The one-click label tokens pinned in the config; `None` offers the
    /// board's three most used labels.
    pub(super) suggestions: Option<Vec<String>>,
    /// The suggestion chip reached with `↓`, while the field has focus.
    pub(super) chip_cursor: Option<usize>,
    /// The field is in use: focused, or being clicked (a click on a chip
    /// takes focus from the text for a frame).
    pub(super) open: bool,
}

/// What one frame of the field shows beyond the text.
struct Extras {
    open: bool,
    active: bool,
    chips: Vec<String>,
    /// "4 of 12" while filtering.
    count: Option<String>,
}

impl IboardApp {
    /// The header filter: 220pt at rest, 480pt while in use or filtering,
    /// growing to the left. Focused, it offers the suggestions as chips; a
    /// picked one becomes a token at the start of the field.
    pub(super) fn filter_field(&mut self, ui: &mut egui::Ui) {
        let shown = (!self.filter_bar.filter.is_empty())
            .then(|| self.visible_grid().iter().map(Vec::len).sum::<usize>());
        self.filter_bar.show(ui, &self.board, shown);
    }
}

impl FilterBar {
    fn show(&mut self, ui: &mut egui::Ui, board: &Board, shown: Option<usize>) {
        let ctx = ui.ctx().clone();
        let focused = ctx.memory(|m| m.has_focus(filter_id()));
        if !focused {
            self.chip_cursor = None;
        }
        let open = focused || self.open;
        let active = !self.filter.is_empty();
        let rect = self.frame(ui, open, active, focused);
        let extras = Extras {
            open,
            active,
            chips: if open { self.chips(board) } else { Vec::new() },
            count: shown.map(|shown| format!("{shown} of {}", board.issue_count())),
        };
        self.row(ui, rect, focused, extras);
    }

    /// Allocates and paints the field at its animated width, and keeps it
    /// open through a click inside it.
    fn frame(&mut self, ui: &mut egui::Ui, open: bool, active: bool, focused: bool) -> Rect {
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        let target = if open || active {
            FILTER_OPEN
        } else {
            FILTER_REST
        };
        let width = ctx
            .animate_value_with_time(
                filter_id().with("width"),
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
        self.open = focused || (self.open && pressed_inside);
        if frame.clicked() {
            self.focus = true;
        }
        let edge = if active && !open {
            p.accent
        } else {
            p.line_strong
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            p.surface,
            Stroke::new(1.0, edge),
            StrokeKind::Inside,
        );
        if open {
            widgets::paint_focus_ring(ui, rect, theme::RADIUS_SM);
        }
        rect
    }

    /// The suggestions not already picked.
    fn chips(&self, board: &Board) -> Vec<String> {
        let issues = board.columns.iter().flat_map(|column| &column.issues);
        filter::suggestions(self.suggestions.as_deref(), issues)
            .into_iter()
            .filter(|word| !self.filter.tokens.contains(word))
            .collect()
    }

    /// Inside the field: the search icon, the tokens, the text, then the
    /// chips, the count and the key hint on the right.
    fn row(&mut self, ui: &mut egui::Ui, rect: Rect, focused: bool, mut extras: Extras) {
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
        theme::paint_icon(
            row.painter(),
            Icon::Search,
            icon,
            theme::palette(ui).ink_muted,
        );
        let mut removed = None;
        for (index, token) in self.filter.tokens.iter().enumerate() {
            if widgets::filter_token(&mut row, token).clicked() {
                removed = Some(index);
            }
        }
        if !self.chips_fit(&row, &extras) {
            extras.chips.clear();
        }
        if focused {
            self.keys(row.ctx(), &extras.chips);
        }
        let (picked, response) = self.trailing(&mut row, &extras);

        if let Some(index) = removed {
            self.filter.tokens.remove(index);
            self.focus = true;
        }
        if let Some(word) = picked {
            self.filter.pick(&word);
            self.focus = true;
        }
        if std::mem::take(&mut self.focus) {
            response.request_focus();
            self.open = true;
        }
    }

    /// Whether the chips leave the text room enough; they hide otherwise.
    fn chips_fit(&self, row: &egui::Ui, extras: &Extras) -> bool {
        let try_label = self.filter.is_empty();
        let chips_width: f32 = extras
            .chips
            .iter()
            .map(|word| widgets::text_width(row, word, theme::meta()) + 14.0 + FILTER_GAP)
            .sum::<f32>()
            + if try_label {
                widgets::text_width(row, "Labels", filter_try_font()) + FILTER_GAP
            } else {
                0.0
            };
        let count_width = extras
            .count
            .as_ref()
            .map(|count| widgets::text_width(row, count, theme::meta()) + FILTER_GAP)
            .unwrap_or(0.0);
        let text_width = widgets::text_width(row, &self.filter.text, theme::ui_body());
        row.available_width() - count_width - chips_width >= text_width.max(FILTER_TEXT_MIN)
    }

    /// Right to left: the chips, "Labels", the count, the key hint, then the
    /// text taking what is left. Returns the chip picked and the text's
    /// response.
    fn trailing(
        &mut self,
        row: &mut egui::Ui,
        extras: &Extras,
    ) -> (Option<String>, egui::Response) {
        let p = theme::palette(row);
        let current = self.chip_cursor;
        let try_label = self.filter.is_empty();
        let mut picked = None;
        let response = row
            .with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = FILTER_GAP;
                for (index, word) in extras.chips.iter().enumerate().rev() {
                    if widgets::chip(ui, word, current == Some(index)).clicked() {
                        picked = Some(word.clone());
                    }
                }
                if !extras.chips.is_empty() && try_label {
                    ui.label(
                        RichText::new("Labels")
                            .font(filter_try_font())
                            .color(p.ink_muted),
                    );
                }
                if let Some(count) = &extras.count {
                    ui.label(RichText::new(count).font(theme::meta()).color(p.ink_muted));
                }
                if !extras.open {
                    let key = if extras.active { "esc" } else { "⌘F" };
                    let width = widgets::text_width(ui, key, theme::meta()) + 8.0;
                    let (cap, _) = ui.allocate_exact_size(Vec2::new(width, 16.0), Sense::hover());
                    widgets::paint_kbd(ui, key, Pos2::new(cap.min.x, cap.center().y));
                }
                let hint = if self.filter.tokens.is_empty() {
                    "Filter issues"
                } else {
                    ""
                };
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter.text)
                        .id(filter_id())
                        .frame(Frame::NONE)
                        .hint_text(RichText::new(hint).color(p.ink_faint))
                        .font(theme::ui_body())
                        .margin(Margin::ZERO)
                        .desired_width(ui.available_width()),
                )
            })
            .inner;
        (picked, response)
    }

    /// The keys the focused filter takes before its text does: `↓` reaches
    /// the chips, `← →` move between them, `↩` picks, `↑` goes back to the
    /// text, and `⌫` in an empty field removes the last token.
    fn keys(&mut self, ctx: &egui::Context, chips: &[String]) {
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

    /// `esc` in the open field leaves the chips, then clears, then leaves the
    /// field. egui drops focus on `esc` before we see it, so the field asks
    /// for it back unless this is the last step.
    pub(super) fn escape(&mut self, ctx: &egui::Context) {
        if self.chip_cursor.take().is_some() {
            self.focus = true;
        } else if !self.filter.is_empty() {
            self.filter.clear();
            self.focus = true;
        } else {
            ctx.memory_mut(|m| m.surrender_focus(filter_id()));
            self.open = false;
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
