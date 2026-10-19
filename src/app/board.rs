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
        let new_card = self.flashing_card();
        if new_card.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }
        let selected = self.session.selected.clone();
        let scroll_to_selected = std::mem::take(&mut self.session.scroll_to_selected);
        let running = self.building_now();
        let agents = self.session.agents;
        let watching = self.watching_ready();

        let mut intents = Vec::new();
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
                            self.session.composer.as_mut()
                        } else {
                            None
                        };
                        let view = ColumnView {
                            column,
                            index,
                            filter: &self.filter_bar.filter,
                            width,
                            height,
                            new_card: new_card.as_deref(),
                            selected: selected.as_deref(),
                            scroll_to_selected,
                            picked: &self.session.picked,
                            root: &self.board.root,
                            labels: &self.labels,
                            reviews: review::CardReviews {
                                past: &self.past_reviews,
                                runs: &self.session.review_runs,
                            },
                            can_review: self.agent.configured(),
                            builds: Builds {
                                running: &running,
                                agents,
                                watching,
                            },
                        };
                        intents.extend(column_ui(ui, &view, composer));
                    }
                    // Ends the scrollable width one gutter after the last column.
                    ui.allocate_space(Vec2::ZERO);
                });
            });
        self.apply(intents);
    }

    /// The issue the composer just wrote, while its card is still highlighted.
    pub(super) fn flashing_card(&self) -> Option<PathBuf> {
        self.session
            .new_card
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NEW_CARD_FLASH)
            .map(|(path, _)| path.clone())
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
/// fill. Clicking it expands the column to the whole window (ColumnFocus).
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
    let width = pad * 2.0 + text;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(expand_tooltip(name, shortcut));
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
    }
    response
}

fn expand_tooltip(name: &str, shortcut: Option<&str>) -> String {
    match shortcut {
        Some(key) => format!("Show {name} full screen  {key}"),
        None => format!("Show {name} full screen"),
    }
}

pub(super) fn column_ui(
    ui: &mut egui::Ui,
    view: &ColumnView,
    composer: Option<&mut Composer>,
) -> Vec<Intent> {
    let column = view.column;
    let mut intents = Vec::new();
    let visible: Vec<&Issue> = column
        .issues
        .iter()
        .filter(|issue| view.filter.matches(issue))
        .collect();
    // `todo/`, or empty for the issues at the issues root.
    let folder = match column.dir.strip_prefix(view.root) {
        Ok(relative) if !relative.as_os_str().is_empty() => format!("{}/", relative.display()),
        _ => String::new(),
    };

    // Reserve the whole column as a drop target before drawing into it, so the
    // empty space below the cards accepts a drop too.
    let origin = ui.cursor().min;
    let drop_rect = Rect::from_min_size(origin, Vec2::new(view.width, view.height));
    // A double-click on the column (its header or empty space; cards take
    // their own clicks) expands it to the whole window, like its title.
    let drop_response = ui.interact(
        drop_rect,
        egui::Id::new(("column_drop", view.index)),
        Sense::click(),
    );
    if drop_response.double_clicked() {
        intents.push(Intent::FocusColumn(column.dir.clone()));
    }

    let mut anchor = None;
    ui.allocate_ui_with_layout(
        Vec2::new(view.width, view.height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_width(view.width);
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            column_head(ui, view, &visible, &mut intents);
            // The composer stays at the top of Todo; the cards scroll under it.
            if let Some(composer) = composer {
                composer_ui(ui, composer, column, &folder, &mut intents);
            }
            egui::ScrollArea::vertical()
                .id_salt(("column", view.index))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(view.width);
                    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                    let cards = column_cards(ui, view, &visible, &folder, &mut intents);
                    // While the DropZones dock covers the bottom of the board,
                    // the last cards can still scroll out from under it.
                    if egui::DragAndDrop::has_payload_of_type::<IssueDrag>(ui.ctx()) {
                        ui.add_space(DOCK_H);
                    }
                    if drop_response.dnd_hover_payload::<IssueDrag>().is_some()
                        || drop_response.dnd_release_payload::<IssueDrag>().is_some()
                    {
                        anchor = drop_line(ui, &cards);
                    }
                });
        },
    );
    highlight_foreign_drop(ui, column, drop_rect, &drop_response);
    if let Some(drag) = drop_response.dnd_release_payload::<IssueDrag>() {
        intents.push(Intent::Place {
            path: drag.path.clone(),
            column: column.dir.clone(),
            anchor,
        });
    }
    intents
}

/// The column's header: dot, uppercase name, count, and the 2pt status rule
/// under it. Hovered, the count gives way to the column actions. Picked cards
/// put "Clear selection" beside the title. A
/// column holding picked cards offers to clear the multi-selection.
fn column_head(
    ui: &mut egui::Ui,
    view: &ColumnView,
    visible: &[&Issue],
    intents: &mut Vec<Intent>,
) {
    let p = theme::palette(ui);
    let column = view.column;
    let status = status_color(p, column);
    let (head_rect, _) =
        ui.allocate_exact_size(Vec2::new(view.width, theme::CONTROL_H), Sense::hover());
    let mut head = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(head_rect.shrink2(Vec2::new(theme::SPACE_2, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    head.spacing_mut().item_spacing.x = theme::SPACE_3;
    let (dot, _) = head.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
    head.painter().circle_filled(dot.center(), 4.0, status);
    // The title is a button: it expands the column to the whole window.
    let shortcut = (view.index < 4).then(|| ["⌘1", "⌘2", "⌘3", "⌘4"][view.index]);
    if column_title_button(&mut head, &column.name, shortcut).clicked() {
        intents.push(Intent::FocusColumn(column.dir.clone()));
    }
    if visible
        .iter()
        .any(|issue| view.picked.contains(&issue.path))
    {
        let id = egui::Id::new(("clear_picked", view.index));
        let hover = "Clear selection  esc";
        if widgets::icon_button(&mut head, Icon::Close, id, hover, widgets::Tone::Plain).clicked() {
            intents.push(Intent::ClearPicked);
        }
    }
    head.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        if ui.rect_contains_pointer(head_rect) {
            column_actions(ui, view, visible, shortcut, intents);
            return;
        }
        // A running build keeps its Stop, and Doing its agents, in view.
        if view.builds.active() {
            build_action(ui, view, visible, intents);
        }
        agents_stepper(ui, view, intents);
        let shown = visible.len();
        // Filtered: "2/6", the total in `ink-faint`.
        let mut count = LayoutJob::default();
        let part = |color| egui::TextFormat::simple(theme::meta(), color);
        if view.filter.is_empty() {
            count.append(&column.issues.len().to_string(), 0.0, part(p.ink_muted));
        } else {
            count.append(&shown.to_string(), 0.0, part(p.ink_muted));
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
}

/// The hovered header's actions, right to left: Todo's `+`, Ready's play,
/// Done's archive, then expand.
fn column_actions(
    ui: &mut egui::Ui,
    view: &ColumnView,
    visible: &[&Issue],
    shortcut: Option<&str>,
    intents: &mut Vec<Intent>,
) {
    let column = view.column;
    let id = egui::Id::new(("new_issue", view.index));
    if column.accepts_new_issues()
        && widgets::icon_button(ui, Icon::Plus, id, "New issue  ⌘N", widgets::Tone::Plain).clicked()
    {
        intents.push(Intent::Compose);
    }
    build_action(ui, view, visible, intents);
    agents_stepper(ui, view, intents);
    if column.name.eq_ignore_ascii_case("done") && !visible.is_empty() {
        done_controls(ui, view, visible, intents);
    }
    let id = egui::Id::new(("expand_column", view.index));
    let hover = expand_tooltip(&column.name, shortcut);
    if widgets::icon_button(ui, Icon::Expand, id, &hover, widgets::Tone::Plain).clicked() {
        intents.push(Intent::FocusColumn(column.dir.clone()));
    }
}

/// Ready's play, which hands its issues to the build agent top to bottom,
/// or stop while that build runs or watches Ready for new issues.
fn build_action(
    ui: &mut egui::Ui,
    view: &ColumnView,
    visible: &[&Issue],
    intents: &mut Vec<Intent>,
) {
    if !discovery::is_ready_column(&view.column.dir) {
        return;
    }
    let id = egui::Id::new(("build", view.index));
    let tone = widgets::Tone::Plain;
    if view.builds.active() {
        if widgets::icon_button(ui, Icon::Stop, id, "Stop the AI build", tone).clicked() {
            intents.push(Intent::StopBuild);
        }
    } else if !visible.is_empty()
        && widgets::icon_button(ui, Icon::Play, id, "Build with AI, top to bottom", tone).clicked()
    {
        intents.push(Intent::Build(
            visible.iter().map(|issue| issue.path.clone()).collect(),
        ));
    }
}

/// Doing's agents, right to left: `+`, how many, `−`. Down to none, which
/// stops the AI build; up to [`build::MAX_AGENTS`].
fn agents_stepper(ui: &mut egui::Ui, view: &ColumnView, intents: &mut Vec<Intent>) {
    if !discovery::is_doing_column(&view.column.dir) {
        return;
    }
    let agents = view.builds.agents;
    let tone = widgets::Tone::Plain;
    ui.add_enabled_ui(agents < build::MAX_AGENTS, |ui| {
        let id = egui::Id::new(("more_agents", view.index));
        if widgets::icon_button(ui, Icon::Plus, id, "One more build agent", tone).clicked() {
            intents.push(Intent::Agents(agents + 1));
        }
    });
    let noun = if agents == 1 { "agent" } else { "agents" };
    ui.label(
        RichText::new(format!("{agents} {noun}"))
            .font(theme::meta())
            .color(theme::palette(ui).ink_muted),
    )
    .on_hover_text("Build agents working at once; none stops the AI build");
    ui.add_enabled_ui(agents > 0, |ui| {
        let id = egui::Id::new(("fewer_agents", view.index));
        if widgets::icon_button(ui, Icon::Minus, id, "One build agent fewer", tone).clicked() {
            intents.push(Intent::Agents(agents - 1));
        }
    });
}

/// The column's cards, or its empty well.
/// Returns each card's issue and rect, for placing a drop.
fn column_cards(
    ui: &mut egui::Ui,
    view: &ColumnView,
    visible: &[&Issue],
    folder: &str,
    intents: &mut Vec<Intent>,
) -> Vec<(PathBuf, Rect)> {
    let column = view.column;
    if visible.is_empty() {
        let message = if column.issues.is_empty() {
            if folder.is_empty() {
                "No issues at the root".to_string()
            } else {
                format!("No issues in {folder}")
            }
        } else {
            view.filter.empty_message()
        };
        empty_well(ui, &message, view.width);
    }
    let mut cards = Vec::new();
    for issue in visible {
        let (action, rect) = board_card(ui, view, issue);
        cards.push((issue.path.clone(), rect));
        intents.extend(action.map(|action| action.on(&issue.path)));
    }

    cards
}

/// Done's header controls, shown while it is hovered: "Archive all" moves
/// the issues it lists (after the filter) to `.archive/`.
fn done_controls(
    ui: &mut egui::Ui,
    view: &ColumnView,
    visible: &[&Issue],
    intents: &mut Vec<Intent>,
) {
    let noun = if visible.len() == 1 {
        "issue"
    } else {
        "issues"
    };
    let hover = format!(
        "Archive all: move the {} {noun} to {ARCHIVE_DIR}/",
        visible.len()
    );
    let id = egui::Id::new(("archive_all", view.index));
    if widgets::icon_button(ui, Icon::Archive, id, &hover, widgets::Tone::Plain).clicked() {
        intents.push(Intent::ArchiveAll(
            visible.iter().map(|issue| issue.path.clone()).collect(),
        ));
    }
}

/// One card on the board, in its look: selected, just written, or plain.
fn board_card(ui: &mut egui::Ui, view: &ColumnView, issue: &Issue) -> (Option<CardAction>, Rect) {
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
        review_pill: view.reviews.pill(&issue.path, p),
        review_button: view.can_review
            && view.column.accepts_new_issues()
            && matches!(look, CardLook::Selected { .. }),
        expand: view.picked.is_empty() || view.selected == Some(issue.path.as_path()),
        build: card_build(view.column, issue, view.builds),
        labelling: discovery::is_editable_column(&view.column.dir),
        ..CardExtras::default()
    };
    issue_card(ui, issue, view.width, look, view.labels, extras)
}

/// Where the dragged card would land: before the first card whose middle is
/// below the pointer, else last. Draws the `accent` line there and returns the
/// card it lands in front of.
fn drop_line(ui: &egui::Ui, cards: &[(PathBuf, Rect)]) -> Option<PathBuf> {
    let p = theme::palette(ui);
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
    cards.get(insert).map(|(path, _)| path.clone())
}

/// Highlights the column while a card from elsewhere hovers over it: the
/// cards' area, under the header and its status rule.
fn highlight_foreign_drop(
    ui: &egui::Ui,
    column: &Column,
    drop_rect: Rect,
    drop_response: &egui::Response,
) {
    let p = theme::palette(ui);
    let hovering_foreign = drop_response
        .dnd_hover_payload::<IssueDrag>()
        .filter(|drag| drag.path.parent() != Some(column.dir.as_path()))
        .is_some();
    if !hovering_foreign {
        return;
    }
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
