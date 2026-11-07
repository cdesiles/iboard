//! The board: its columns of cards.

use super::build::{self, Builds, card_build};
use super::card::{CardAction, CardExtras, CardLook, issue_card};
use super::dock::{DOCK_H, IssueDrag};
use super::intent::Intent;
use super::naming::{Naming, is_naming};
use super::{
    ARCHIVE_DIR, Align, Board, Color32, Column, CornerRadius, Duration, Filter, Frame, IboardApp,
    Icon, Issue, Labels, Layout, LayoutJob, MarginF32, Palette, Path, PathBuf, Pos2, Rect,
    RichText, Sense, Stroke, StrokeKind, Vec2, discovery, review, theme, widgets,
};

/// The start of the gutter after a column, where its thumb floats clear of
/// the cards.
const THUMB_LANE: f32 = theme::SCROLL_HIT;

/// How long an issue created from the composer takes to fade from
/// `accent-soft` to `surface`.
const NEW_CARD_FLASH: Duration = Duration::from_secs(2);

impl IboardApp {
    pub(super) fn board_ui(&mut self, ui: &mut egui::Ui) {
        if self.board.columns.is_empty() {
            Frame::new().inner_margin(theme::SPACE_5).show(ui, |ui| {
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
            ui.ctx().request_repaint();
        }
        let fades = !theme::reduced_motion(ui.ctx());
        let selected = self.session.selected.clone();
        let scroll_to_selected = std::mem::take(&mut self.session.scroll_to_selected);
        let running = self.building_now();
        let agents = self.session.agents;
        let watching = self.watching_ready();

        let dragged = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx())
            .and_then(|drag| drag.path.parent().map(Path::to_path_buf));
        let mut dragged_column = None;
        let mut intents = Vec::new();
        let background = ui.interact(
            ui.available_rect_before_wrap(),
            egui::Id::unique("board_background"),
            Sense::click(),
        );
        if background.clicked() {
            intents.push(Intent::ClearPicked);
        }
        theme::scroll_area(
            ui,
            egui::ScrollArea::horizontal()
                .id_salt("board")
                .auto_shrink([false, false]),
            |ui| {
                ui.add_space(gutter);
                ui.horizontal_top(|ui| {
                    // Each column's scrollbar lane takes the start of the
                    // gutter after it.
                    ui.spacing_mut().item_spacing.x = gutter - THUMB_LANE;
                    // The window gutter on the left; item spacing after the last
                    // column gives the same on the right.
                    ui.add_space(gutter);
                    let parked = self.has_parked_note();
                    for (index, column) in self.board.columns.iter().enumerate() {
                        let view = ColumnView {
                            column,
                            index,
                            filter: &self.filter_bar.filter,
                            width,
                            height,
                            new_card: new_card.as_ref().map(|(path, left)| {
                                (path.as_path(), if fades { *left } else { 1.0 })
                            }),
                            selected: selected.as_deref(),
                            scroll_to_selected,
                            picked: &self.session.picked,
                            root: &self.board.root,
                            board: &self.board,
                            parked: parked && column.accepts_new_issues(),
                            labels: &self.prefs.labels,
                            reviews: review::CardReviews {
                                past: &self.past_reviews,
                                runs: &self.session.review_runs,
                            },
                            can_review: self.prefs.agent.configured(),
                            namings: &self.session.namings,
                            builds: Builds {
                                running: &running,
                                agents,
                                watching,
                            },
                        };
                        if dragged.as_deref() == Some(column.dir.as_path()) {
                            let left = ui.cursor().min.x;
                            dragged_column = Some(egui::Rangef::new(left, left + width));
                        }
                        intents.extend(column_ui(ui, &view));
                    }
                    // Ends the scrollable width one gutter after the last column.
                    ui.allocate_space(Vec2::ZERO);
                });
            },
        );
        self.dragged_column = dragged_column;
        self.apply(intents);
    }

    /// The issue the composer just wrote, while its card still fades, and
    /// how much of the highlight is left, from 1 to 0.
    pub(super) fn flashing_card(&self) -> Option<(PathBuf, f32)> {
        self.session
            .new_card
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NEW_CARD_FLASH)
            .map(|(path, at)| {
                let gone = at.elapsed().as_secs_f32() / NEW_CARD_FLASH.as_secs_f32();
                (path.clone(), 1.0 - gone)
            })
    }
}

/// Where a column's issues stand, from its folder's role.
pub(super) fn column_status(column: &Column) -> theme::Status {
    match column.name.to_lowercase().as_str() {
        "ready" => theme::Status::Ready,
        "doing" | "in progress" | "in-progress" | "wip" | "review" => theme::Status::Doing,
        "done" => theme::Status::Done,
        _ => theme::Status::Todo,
    }
}

/// The status color of a column.
pub(super) fn status_color(p: &Palette, column: &Column) -> Color32 {
    column_status(column).color(p)
}

/// Everything `column_ui` needs to know about one column.
struct ColumnView<'a> {
    column: &'a Column,
    index: usize,
    filter: &'a Filter,
    width: f32,
    height: f32,
    /// An issue just created by the composer, and how much of its highlight
    /// is left.
    new_card: Option<(&'a Path, f32)>,
    /// The card selected with the arrow keys or by opening it.
    selected: Option<&'a Path>,
    /// Scroll the selected card into view this frame.
    scroll_to_selected: bool,
    /// The multi-selection.
    picked: &'a [PathBuf],
    root: &'a Path,
    /// The board, which knows each card's project.
    board: &'a Board,
    /// NewIssueModal holds parked notes: Todo's `+` shows a dot.
    parked: bool,
    /// The label colours, for the chip on each card.
    labels: &'a Labels,
    /// The cards' review pills.
    reviews: review::CardReviews<'a>,
    /// An agent is set up, so the selected Todo card offers Review.
    can_review: bool,
    /// The new issues whose titles are still to land.
    namings: &'a [Naming],
    /// The issues the build agents work on, and how many may.
    builds: Builds<'a>,
}

fn column_ui(ui: &mut egui::Ui, view: &ColumnView) -> Vec<Intent> {
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
    let drop_response = ui.interact(
        drop_rect,
        egui::Id::unique(("column_drop", view.index)),
        Sense::hover(),
    );

    let mut anchor = None;
    ui.allocate_ui_with_layout(
        Vec2::new(view.width, view.height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_width(view.width);
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            column_head(ui, view, &visible, &mut intents);
            let lane = Rect::from_min_size(
                ui.cursor().min,
                Vec2::new(view.width + THUMB_LANE, ui.available_height()),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(lane), |ui| {
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .id_salt(("column", view.index))
                        .content_margin(MarginF32 {
                            right: THUMB_LANE,
                            ..MarginF32::ZERO
                        })
                        .auto_shrink([false, false]),
                    |ui| {
                        ui.set_width(view.width);
                        ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                        let over = foreign_drag_over(view, &drop_response);
                        let cards = column_cards(ui, view, &visible, &folder, over, &mut intents);
                        // While the DropZones dock covers the bottom of the board,
                        // the last cards can still scroll out from under it.
                        if egui::DragAndDrop::has_payload_of_type::<IssueDrag>(ui.ctx()) {
                            ui.add_space(DOCK_H);
                        }
                        if discovery::is_orderable_column(&column.dir)
                            && (drop_response.dnd_hover_payload::<IssueDrag>().is_some()
                                || drop_response.dnd_release_payload::<IssueDrag>().is_some())
                        {
                            anchor = drop_line(ui, &cards);
                        }
                    },
                );
            });
        },
    );
    // An empty column's well is the highlight.
    if !visible.is_empty() && foreign_drag_over(view, &drop_response) {
        highlight_drop(ui, drop_rect);
    }
    if let Some(drag) = drop_response.dnd_release_payload::<IssueDrag>() {
        intents.push(Intent::Place {
            path: drag.path.clone(),
            column: column.dir.clone(),
            anchor,
        });
    }
    intents
}

/// The column's header: its status icon, its name and its count. Hovered, the
/// count gives way to the column actions, fading in. A column holding picked
/// cards offers to clear the multi-selection beside its title.
fn column_head(
    ui: &mut egui::Ui,
    view: &ColumnView,
    visible: &[&Issue],
    intents: &mut Vec<Intent>,
) {
    let p = theme::palette(ui);
    let column = view.column;
    let (head_rect, _) =
        ui.allocate_exact_size(Vec2::new(view.width, theme::CONTROL_H), Sense::hover());
    let mut head = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(head_rect.shrink2(Vec2::new(theme::SPACE_2, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    head.spacing_mut().item_spacing.x = theme::SPACE_3;
    let (icon, _) = head.allocate_exact_size(Vec2::splat(theme::STATUS_ICON), Sense::hover());
    theme::status_icon(head.painter(), column_status(column), icon, p);
    head.label(
        RichText::new(&column.name)
            .font(theme::ui_strong())
            .color(p.ink),
    );
    if visible
        .iter()
        .any(|issue| view.picked.contains(&issue.path))
    {
        let id = egui::Id::unique(("clear_picked", view.index));
        let hover = "Clear selection  esc";
        if widgets::icon_button(&mut head, Icon::Close, id, hover, widgets::Tone::Ghost).clicked() {
            intents.push(Intent::ClearPicked);
        }
    }
    let hovered = head.rect_contains_pointer(head_rect);
    let fade = widgets::hover_fade(
        head.ctx(),
        egui::Id::unique(("column_head", view.index)),
        hovered,
    );
    let acts = has_actions(view, visible);
    head.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        if hovered && acts {
            ui.multiply_opacity(fade);
            column_actions(ui, view, visible, intents);
            return;
        }
        // Parked notes keep Todo's `+` and its dot in view.
        if view.parked {
            new_issue_button(ui, view, intents);
        }
        // A running build keeps its Stop in view.
        if view.builds.active() {
            build_action(ui, view, visible, intents);
        }
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
}

/// The hovered header's actions, right to left: Todo's `+`, Ready's play,
/// Doing's agents and Done's archive.
/// Whether [`column_actions`] has anything to offer in place of the count.
fn has_actions(view: &ColumnView, visible: &[&Issue]) -> bool {
    let column = view.column;
    let builds = discovery::is_ready_column(&column.dir)
        && (view.builds.active() || visible.iter().any(|issue| build::buildable(issue)));
    column.accepts_new_issues()
        || builds
        || discovery::is_doing_column(&column.dir)
        || column.name.eq_ignore_ascii_case("done") && !visible.is_empty()
}

fn column_actions(
    ui: &mut egui::Ui,
    view: &ColumnView,
    visible: &[&Issue],
    intents: &mut Vec<Intent>,
) {
    let column = view.column;
    if column.accepts_new_issues() {
        new_issue_button(ui, view, intents);
    }
    build_action(ui, view, visible, intents);
    agents_stepper(ui, view, intents);
    if column.name.eq_ignore_ascii_case("done") && !visible.is_empty() {
        done_controls(ui, view, visible, intents);
    }
}

/// Todo's `+`, with an `accent` dot while NewIssueModal holds parked notes.
fn new_issue_button(ui: &mut egui::Ui, view: &ColumnView, intents: &mut Vec<Intent>) {
    let id = egui::Id::unique(("new_issue", view.index));
    let hover = if view.parked {
        "1 parked note  ⌘N"
    } else {
        "New issue  ⌘N"
    };
    let plus = widgets::icon_button(ui, Icon::Plus, id, hover, widgets::Tone::Ghost);
    if view.parked {
        let p = theme::palette(ui);
        ui.painter()
            .circle_filled(plus.rect.right_top() + Vec2::new(-4.0, 4.0), 3.0, p.accent);
    }
    if plus.clicked() {
        intents.push(Intent::Compose);
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
    let id = egui::Id::unique(("build", view.index));
    let tone = widgets::Tone::Ghost;
    if view.builds.active() {
        if widgets::icon_button(ui, Icon::Stop, id, "Stop the AI build", tone).clicked() {
            intents.push(Intent::StopBuild);
        }
    } else if visible.iter().any(|issue| build::buildable(issue))
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
    let tone = widgets::Tone::Ghost;
    ui.add_enabled_ui(agents < build::MAX_AGENTS, |ui| {
        let id = egui::Id::unique(("more_agents", view.index));
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
        let id = egui::Id::unique(("fewer_agents", view.index));
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
    over: bool,
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
        empty_well(ui, &message, view.width, over);
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
    let id = egui::Id::unique(("archive_all", view.index));
    if widgets::icon_button(ui, Icon::Archive, id, &hover, widgets::Tone::Ghost).clicked() {
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
    } else if let Some((_, left)) = view
        .new_card
        .filter(|(path, _)| *path == issue.path.as_path())
    {
        CardLook::Fresh { left }
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
        active_labels: &view.filter.tokens,
        naming: is_naming(view.namings, &issue.path),
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

/// A card from another column is over this one.
fn foreign_drag_over(view: &ColumnView, drop_response: &egui::Response) -> bool {
    drop_response
        .dnd_hover_payload::<IssueDrag>()
        .is_some_and(|drag| !view.board.in_column(&drag.path, &view.column.dir))
}

/// Highlights the column's cards area, under its header, as a drop target.
fn highlight_drop(ui: &egui::Ui, drop_rect: Rect) {
    let p = theme::palette(ui);
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

/// The dashed well of an empty column, with no fill. It is also where a
/// dragged card lands: while one is `over` it, solid `accent` on `accent-soft`.
fn empty_well(ui: &mut egui::Ui, message: &str, width: f32, over: bool) {
    let p = theme::palette(ui);
    let corner = CornerRadius::same(theme::RADIUS_MD);
    let response = Frame::new()
        .fill(if over {
            p.accent_soft
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(corner)
        .inner_margin(theme::SPACE_4)
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
    if over {
        let edge = Stroke::new(1.0, p.accent);
        ui.painter()
            .rect_stroke(response.rect, corner, edge, StrokeKind::Inside);
    } else {
        dashed_rect(ui.painter(), response.rect, theme::RADIUS_MD, p.line);
    }
}

pub(super) use crate::widgets::dashed_rect;
