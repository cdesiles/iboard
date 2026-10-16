//! ColumnFocus: one column across the whole window, as a grid.

use super::*;

/// ColumnFocus's state, kept for the session.
#[derive(Default)]
pub(super) struct ColumnFocus {
    /// The column expanded to the whole window, by folder.
    pub(super) column: Option<PathBuf>,
    /// The sort picked per folder.
    pub(super) sorts: HashMap<PathBuf, Sort>,
    /// Split the grid into sections by label.
    pub(super) group_by_label: bool,
}

impl IboardApp {
    /// Expands a column to the whole window (ColumnFocus).
    pub(super) fn focus_column(&mut self, dir: PathBuf) {
        if self.board.columns.iter().any(|column| column.dir == dir) {
            self.column_focus.column = Some(dir);
        }
    }

    /// Leaves ColumnFocus for the board, keeping the selection.
    pub(super) fn unfocus_column(&mut self) {
        self.column_focus.column = None;
    }

    /// The sort a focused column uses: the one the user picked, or its
    /// default.
    pub(super) fn column_sort(&self, dir: &Path) -> Sort {
        self.column_focus
            .sorts
            .get(dir)
            .copied()
            .unwrap_or_else(|| default_sort(dir))
    }

    /// ColumnFocus: one column across the whole window (the design's
    /// ColumnFocus component). The header stays; under it a focus bar holds the
    /// column tabs, the sort select, Group by label and Board, then the cards
    /// flow in a wide grid.
    pub(super) fn column_focus_ui(&mut self, ui: &mut egui::Ui) {
        let Some(dir) = self.column_focus.column.clone() else {
            return;
        };
        let p = theme::palette(ui);
        let tabs: Vec<FocusTab> = self
            .board
            .columns
            .iter()
            .map(|column| FocusTab {
                name: column.name.clone(),
                dir: column.dir.clone(),
                count: column.issues.len(),
                color: status_color(p, column),
                active: column.dir == dir,
            })
            .collect();
        let done = self
            .board
            .columns
            .iter()
            .find(|column| column.dir == dir)
            .is_some_and(|column| column.name.eq_ignore_ascii_case("done"));
        let sort = self.column_sort(&dir);
        let intents = focus_bar(ui, &tabs, sort, &mut self.column_focus.group_by_label, done);
        self.apply(intents);

        // The bar may have switched or left the focus; redraw with the state it
        // set. When it left, the next frame draws the board.
        let Some(dir) = self.column_focus.column.clone() else {
            return;
        };
        let Some(column) = self.board.columns.iter().find(|column| column.dir == dir) else {
            return;
        };
        let selected = self.session.selected.clone();
        let scroll_to_selected = std::mem::take(&mut self.session.scroll_to_selected);
        let new_card = self.flashing_card();
        let running = self.building_now();
        let view = FocusGrid {
            column,
            root: &self.board.root,
            filter: &self.filter_bar.filter,
            sort: self.column_sort(&dir),
            group: self.column_focus.group_by_label,
            labels: &self.labels,
            sections: &self.board.reader_sections,
            past_reviews: &self.past_reviews,
            can_review: self.agent.configured(),
            selected: selected.as_deref(),
            scroll_to_selected,
            picked: &self.session.picked,
            new_card: new_card.as_deref(),
            builds: Builds {
                running: &running,
                agents: self.session.agents,
                watching: self.watching_ready(),
            },
        };
        let intents = focus_grid(ui, &view);
        self.apply(intents);
    }
}

/// The ColumnFocus focus bar: the segmented column tabs on the left, then the
/// Sort select, Group by label and the bordered Board button on the right,
/// over a `line` rule.
pub(super) fn focus_bar(
    ui: &mut egui::Ui,
    tabs: &[FocusTab],
    sort: Sort,
    group: &mut bool,
    done: bool,
) -> Vec<Intent> {
    let p = theme::palette(ui);
    let mut intents = Vec::new();
    let column = tabs
        .iter()
        .find(|tab| tab.active)
        .map(|tab| tab.dir.clone());
    let bar = Frame::new()
        .inner_margin(Margin::symmetric(
            theme::SPACE_5 as i8,
            theme::SPACE_4 as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                focus_tabs(ui, tabs, &mut intents);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                    // Board, the visible way out (`esc`).
                    if widgets::button(
                        ui,
                        ButtonKind::Secondary,
                        Some(Icon::Collapse),
                        "Board",
                        Some("esc"),
                    )
                    .on_hover_text("Back to the whole board (esc)")
                    .clicked()
                    {
                        intents.push(Intent::UnfocusColumn);
                    }
                    widgets::checkbox(ui, group, "Group by label");
                    if let Some(sort) = sort_select(ui, sort, done)
                        && let Some(column) = column.clone()
                    {
                        intents.push(Intent::Sort { column, sort });
                    }
                    let cap = RichText::new("Sort").font(theme::meta()).color(p.ink_muted);
                    ui.label(cap);
                });
            });
        });
    // The `line` rule under the bar, across the whole window.
    ui.painter().hline(
        bar.response.rect.x_range(),
        bar.response.rect.max.y - 0.5,
        Stroke::new(1.0, p.line),
    );
    intents
}

/// The column tabs: a click shows that column (the active one leaves
/// ColumnFocus). While a card is dragged they are drop targets: a drop moves
/// it to that column without leaving the view.
fn focus_tabs(ui: &mut egui::Ui, tabs: &[FocusTab], intents: &mut Vec<Intent>) {
    let p = theme::palette(ui);
    let shown = widgets::column_tabs(
        ui,
        &tabs
            .iter()
            .map(|tab| widgets::ColumnTab {
                name: &tab.name,
                count: tab.count,
                color: tab.color,
                active: tab.active,
            })
            .collect::<Vec<_>>(),
    );
    if let Some(tab) = shown.clicked.and_then(|at| tabs.get(at)) {
        intents.push(if tab.active {
            Intent::UnfocusColumn
        } else {
            Intent::FocusColumn(tab.dir.clone())
        });
    }
    let Some(payload) = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx()) else {
        return;
    };
    for (at, (rect, tab)) in shown.tab_rects.iter().zip(tabs).enumerate() {
        let response = ui.interact(*rect, egui::Id::new(("coltab_drop", at)), Sense::hover());
        if response.dnd_hover_payload::<IssueDrag>().is_some() {
            ui.painter().rect_stroke(
                *rect,
                CornerRadius::same(3),
                Stroke::new(2.0, p.accent),
                StrokeKind::Outside,
            );
        }
        if response.dnd_release_payload::<IssueDrag>().is_some() {
            intents.push(Intent::Move {
                path: payload.path.clone(),
                to: tab.dir.clone(),
            });
        }
    }
}

/// The 170pt Sort select and its menu; Done adds Finished first. Returns
/// the sort picked.
fn sort_select(ui: &mut egui::Ui, sort: Sort, done: bool) -> Option<Sort> {
    let response = widgets::select_sized(
        ui,
        egui::Id::new("focus-sort"),
        sort.label(),
        None,
        false,
        170.0,
    );
    let options: Vec<Sort> = if done {
        let mut options = vec![Sort::Finished];
        options.extend(Sort::MENU);
        options
    } else {
        Sort::MENU.to_vec()
    };
    egui::Popup::menu(&response)
        .width(170.0)
        .align(egui::RectAlign::BOTTOM_START)
        .show(|ui| {
            ui.set_min_width(170.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let mut picked = None;
            for option in &options {
                let row = widgets::MenuOption {
                    name: option.label(),
                    mono: false,
                    muted: false,
                    meta: "",
                    checked: *option == sort,
                    active: false,
                };
                if widgets::menu_option(ui, row).clicked() {
                    picked = Some(*option);
                }
            }
            picked
        })
        .and_then(|inner| inner.inner)
}

/// Where a card dropped at `pos` lands among `cards` (row-major): the index to
/// insert before, or `cards.len()` at the end.
pub(super) fn insertion_index(cards: &[(PathBuf, Rect)], pos: Pos2) -> usize {
    for (index, (_, rect)) in cards.iter().enumerate() {
        if pos.y < rect.center().y {
            return index;
        }
        if pos.y <= rect.max.y && pos.x < rect.center().x {
            return index;
        }
    }
    cards.len()
}

/// The label heading of a Group by label section: the chip and the count over a
/// `line` rule.
pub(super) fn section_heading(
    ui: &mut egui::Ui,
    label: Option<&str>,
    count: usize,
    labels: &Labels,
) {
    let p = theme::palette(ui);
    ui.add_space(theme::SPACE_4);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        match label {
            Some(label) => {
                widgets::label_pill(ui, label, labels.color(label));
            }
            None => {
                ui.label(
                    RichText::new("No label")
                        .font(theme::ui_strong())
                        .color(p.ink_muted),
                );
            }
        }
        let noun = if count == 1 { "issue" } else { "issues" };
        ui.label(
            RichText::new(format!("{count} {noun}"))
                .font(theme::meta())
                .color(p.ink_muted),
        );
    });
    let y = ui.cursor().min.y + theme::SPACE_1;
    let left = ui.max_rect().min.x;
    let right = ui.max_rect().max.x;
    ui.painter()
        .hline(left..=right, y, Stroke::new(1.0, p.line));
    ui.add_space(theme::SPACE_3);
}

/// The ColumnFocus grid: the focused column's cards at a 300pt-minimum, in
/// `SPACE_4`-apart columns, optionally split by label. Cards are the board's
/// [`issue_card`] with the wide extras: more summary, a facts row and the
/// modified date.
pub(super) fn focus_grid(ui: &mut egui::Ui, view: &FocusGrid) -> Vec<Intent> {
    let mut intents = Vec::new();
    let issues = focus_issues(view.column, view.filter, view.sort);
    let empty = issues.is_empty();
    let groups = if view.group {
        group_issues(issues)
    } else {
        vec![(None, issues)]
    };

    let gutter = theme::SPACE_5;
    let gap = theme::SPACE_4;
    let min = 300.0;
    let available = (ui.available_width() - 2.0 * gutter).max(0.0);
    let columns = (((available + gap) / (min + gap)).floor() as usize).max(1);
    let cell = ((available - (columns as f32 - 1.0) * gap) / columns as f32).max(1.0);

    let mut cards: Vec<(PathBuf, Rect)> = Vec::new();
    egui::ScrollArea::vertical()
        .id_salt("focus")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(gutter);
            ui.horizontal(|ui| {
                ui.add_space(gutter);
                ui.vertical(|ui| {
                    ui.set_width(available);
                    ui.spacing_mut().item_spacing.y = gap;
                    if empty {
                        empty_well(ui, &focus_empty_message(view), available);
                        return;
                    }
                    for (group_index, (label, group)) in groups.iter().enumerate() {
                        if view.group {
                            if group_index > 0 {
                                ui.add_space(theme::SPACE_4);
                            }
                            section_heading(ui, label.as_deref(), group.len(), view.labels);
                        }
                        // Row by row, `columns` cards each: `horizontal_top`
                        // keeps every card in a row at the same top, and the
                        // vertical `gap` spaces the rows.
                        for row in group.chunks(columns) {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                for issue in row {
                                    let (action, rect) = focus_card(ui, view, issue, cell);
                                    cards.push((issue.path.clone(), rect));
                                    intents.extend(action.map(|action| action.on(&issue.path)));
                                }
                            });
                        }
                    }
                    ui.add_space(gutter);
                    intents.extend(focus_drop(ui, view, &cards, gap));
                });
            });
        });
    intents
}

/// What an empty focused column says: no issues in its folder, or none
/// matching the filter.
fn focus_empty_message(view: &FocusGrid) -> String {
    let folder = match view.column.dir.strip_prefix(view.root) {
        Ok(relative) if !relative.as_os_str().is_empty() => format!("{}/", relative.display()),
        _ => String::new(),
    };
    if !view.column.issues.is_empty() {
        view.filter.empty_message()
    } else if folder.is_empty() {
        "No issues at the root".to_string()
    } else {
        format!("No issues in {folder}")
    }
}

/// One wide card in a `cell`-wide slot of the grid.
fn focus_card(
    ui: &mut egui::Ui,
    view: &FocusGrid,
    issue: &Issue,
    cell: f32,
) -> (Option<CardAction>, Rect) {
    let p = theme::palette(ui);
    let look = if view.selected == Some(issue.path.as_path()) {
        CardLook::Selected {
            scroll: view.scroll_to_selected,
        }
    } else if view.picked.contains(&issue.path) {
        CardLook::Selected { scroll: false }
    } else if view.new_card == Some(issue.path.as_path()) {
        CardLook::Fresh
    } else {
        CardLook::Plain
    };
    let extras = CardExtras {
        review_pill: view
            .past_reviews
            .get(&issue.path)
            .map(|past| past.card_pill(p)),
        review_button: view.can_review
            && view.column.accepts_new_issues()
            && matches!(look, CardLook::Selected { .. }),
        expand: view.picked.is_empty() || view.selected == Some(issue.path.as_path()),
        wide: true,
        sections: view.sections,
        build: card_build(view.column, issue, view.builds),
        labelling: discovery::is_editable_column(&view.column.dir),
    };
    ui.allocate_ui_with_layout(Vec2::new(cell, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(cell);
        issue_card(ui, issue, cell, look, view.labels, extras)
    })
    .inner
}

/// Reordering by drag, offered only where the order is the files' order on
/// the board: the `accent` line where the card would land, and the drop.
fn focus_drop(
    ui: &mut egui::Ui,
    view: &FocusGrid,
    cards: &[(PathBuf, Rect)],
    gap: f32,
) -> Option<Intent> {
    if view.sort != Sort::Board {
        return None;
    }
    let drag = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx())?;
    let p = theme::palette(ui);
    let response = ui.interact(ui.min_rect(), egui::Id::new("focus_drop"), Sense::hover());
    let pointer = ui
        .ctx()
        .pointer_interact_pos()
        .or(ui.ctx().pointer_hover_pos());
    let insert = pointer
        .map(|pos| insertion_index(cards, pos))
        .unwrap_or(cards.len());
    if response.dnd_hover_payload::<IssueDrag>().is_some()
        && pointer.is_some()
        && let Some((_, last)) = cards.last()
    {
        let (x, y0, y1) = match cards.get(insert) {
            Some((_, rect)) => (rect.min.x - gap / 2.0, rect.min.y, rect.max.y),
            None => (last.max.x + gap / 2.0, last.min.y, last.max.y),
        };
        ui.painter().line_segment(
            [Pos2::new(x, y0), Pos2::new(x, y1)],
            Stroke::new(2.0, p.accent),
        );
    }
    response
        .dnd_release_payload::<IssueDrag>()
        .is_some()
        .then(|| Intent::Place {
            path: drag.path.clone(),
            column: view.column.dir.clone(),
            anchor: cards.get(insert).map(|(path, _)| path.clone()),
        })
}
