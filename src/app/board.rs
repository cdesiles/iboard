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
use iboard::release;

mod drop;

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
        let running = self.building_now();
        let questions = self.pending_questions();
        let pass = Pass {
            width,
            height,
            new_card: new_card
                .as_ref()
                .map(|(path, left)| (path.as_path(), if fades { *left } else { 1.0 })),
            scroll_to_selected: std::mem::take(&mut self.session.scroll_to_selected),
            parked: self.has_parked_note(),
            builds: Builds {
                running: &running,
                agents: self.session.agents,
                watching: self.watching_ready(),
            },
            questions: &questions,
        };

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
                    for (index, column) in self.board.columns.iter().enumerate() {
                        let view = ColumnView::new(self, pass, index, column);
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

/// What every column shares in this frame.
#[derive(Clone, Copy)]
struct Pass<'a> {
    width: f32,
    height: f32,
    new_card: Option<(&'a Path, f32)>,
    scroll_to_selected: bool,
    /// NewIssueModal holds parked notes.
    parked: bool,
    builds: Builds<'a>,
    /// The questions the builds wait on, by issue.
    questions: &'a [(PathBuf, crate::agent::service::Question)],
}

/// Everything `column_ui` needs to know about one column.
struct ColumnView<'a> {
    column: &'a Column,
    index: usize,
    filter: &'a Filter,
    /// The column's issues the filter keeps.
    visible: Vec<&'a Issue>,
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
    /// The tags that mark a release, for Done's Release action.
    release_tags: &'a str,
    /// The cards' review pills.
    reviews: review::CardReviews<'a>,
    /// An agent is set up, so the selected Todo card offers Review.
    can_review: bool,
    /// The new issues whose titles are still to land.
    namings: &'a [Naming],
    /// The issues the build agents work on, and how many may.
    builds: Builds<'a>,
    /// The questions the builds wait on, by issue.
    questions: &'a [(PathBuf, crate::agent::service::Question)],
}

impl<'a> ColumnView<'a> {
    fn new(app: &'a IboardApp, pass: Pass<'a>, index: usize, column: &'a Column) -> Self {
        let filter = &app.filter_bar.filter;
        Self {
            column,
            index,
            filter,
            visible: column
                .issues
                .iter()
                .filter(|issue| filter.matches(issue))
                .collect(),
            width: pass.width,
            height: pass.height,
            new_card: pass.new_card,
            selected: app.session.selected.as_deref(),
            scroll_to_selected: pass.scroll_to_selected,
            picked: &app.session.picked,
            root: &app.board.root,
            board: &app.board,
            parked: pass.parked && column.accepts_new_issues(),
            labels: &app.prefs.labels,
            release_tags: &app.prefs.releases.tags,
            reviews: review::CardReviews {
                past: &app.past_reviews,
                runs: &app.session.review_runs,
            },
            can_review: app.prefs.agent.configured(),
            namings: &app.session.namings,
            builds: pass.builds,
            questions: pass.questions,
        }
    }

    fn visible_paths(&self) -> Vec<PathBuf> {
        self.visible
            .iter()
            .map(|issue| issue.path.clone())
            .collect()
    }
}

fn column_ui(ui: &mut egui::Ui, view: &ColumnView) -> Vec<Intent> {
    let column = view.column;
    let mut intents = Vec::new();
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
            column_head(ui, view, &mut intents);
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
                        let cards = column_cards(ui, view, &folder, over, &mut intents);
                        // While the DropZones dock covers the bottom of the board,
                        // the last cards can still scroll out from under it.
                        if egui::DragAndDrop::has_payload_of_type::<IssueDrag>(ui.ctx()) {
                            ui.add_space(DOCK_H);
                        }
                        if discovery::is_orderable_column(&column.dir)
                            && (drop_response.dnd_hover_payload::<IssueDrag>().is_some()
                                || drop_response.dnd_release_payload::<IssueDrag>().is_some())
                        {
                            anchor = drop::drop_line(ui, &cards);
                        }
                    },
                );
            });
        },
    );
    // An empty column's well is the highlight.
    if !view.visible.is_empty() && foreign_drag_over(view, &drop_response) {
        drop::highlight_drop(ui, drop_rect);
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
fn column_head(ui: &mut egui::Ui, view: &ColumnView, intents: &mut Vec<Intent>) {
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
    if view
        .visible
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
    // Doing's agents popover outlives the header's hover.
    let open = hovered
        || egui::Popup::is_id_open(
            head.ctx(),
            egui::Id::unique(("agents", view.index)).with("popup"),
        );
    let fade = widgets::hover_fade(
        head.ctx(),
        egui::Id::unique(("column_head", view.index)),
        open,
    );
    let actions = header_actions(view);
    let has_actions = !actions.is_empty();
    let mut title = None;
    head.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        if open && has_actions {
            ui.multiply_opacity(fade);
            for action in actions {
                intents.extend(header_action(ui, view, action, &mut title));
            }
        } else {
            for action in actions.into_iter().filter(|action| action.at_rest(view)) {
                intents.extend(header_action(ui, view, action, &mut title));
            }
        }
        if let Some(title) = &title {
            action_title(ui, title);
        } else if !open || !has_actions {
            let shown = view.visible.len();
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
        }
    });
}

/// The title of the action under the pointer: a `surface-hover` pill, left of
/// the row's icons, as the Reader's dynamic buttons show theirs.
fn action_title(ui: &mut egui::Ui, title: &str) {
    let p = theme::palette(ui);
    let mut font = theme::ui_strong();
    font.size = 12.0;
    Frame::new()
        .fill(p.surface_hover)
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Vec2::new(theme::SPACE_2, 1.0))
        .show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(title).font(font).color(p.ink)).truncate());
        });
}

/// What a column's header offers, right to left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaderAction {
    /// Todo's `+`, with an `accent` dot while NewIssueModal holds parked
    /// notes.
    NewIssue,
    /// Ready's play, which hands its issues to the build agent top to bottom.
    Build,
    /// Ready's stop, while the build runs or watches Ready for new issues.
    StopBuild,
    /// Doing's agents, right to left: `+`, how many, `−`. Down to none, which
    /// stops the AI build; up to [`build::MAX_AGENTS`].
    Agents,
    /// Done's "Archive all", which moves the issues it lists (after the
    /// filter) to `.archive/`.
    ArchiveAll,
    /// Done's flag: write a release note from the issues it lists, commit and
    /// tag it, then push.
    Release,
}

impl HeaderAction {
    /// Parked notes keep Todo's `+` and its dot in view, and a running build
    /// its Stop, while the header is not hovered.
    fn at_rest(self, view: &ColumnView) -> bool {
        match self {
            Self::NewIssue => view.parked,
            Self::StopBuild => true,
            Self::Build | Self::Agents | Self::ArchiveAll | Self::Release => false,
        }
    }
}

fn header_actions(view: &ColumnView) -> Vec<HeaderAction> {
    let column = view.column;
    let mut actions = Vec::new();
    if column.accepts_new_issues() {
        actions.push(HeaderAction::NewIssue);
    }
    if discovery::is_ready_column(&column.dir) {
        if view.builds.active() {
            actions.push(HeaderAction::StopBuild);
        } else if view.visible.iter().any(|issue| build::buildable(issue)) {
            actions.push(HeaderAction::Build);
        }
    }
    if discovery::is_doing_column(&column.dir) {
        actions.push(HeaderAction::Agents);
    }
    if column.name.eq_ignore_ascii_case("done") {
        if !view.visible.is_empty() {
            actions.push(HeaderAction::ArchiveAll);
        }
        // A workspace board merges several repositories: one release would
        // have to pick one of them, so only a single project offers it.
        if view.board.repos.is_empty() {
            actions.push(HeaderAction::Release);
        }
    }
    actions
}

fn header_action(
    ui: &mut egui::Ui,
    view: &ColumnView,
    action: HeaderAction,
    title: &mut Option<String>,
) -> Option<Intent> {
    match action {
        HeaderAction::NewIssue => new_issue_button(ui, view, title),
        HeaderAction::Build => {
            let hover = "Build with AI, top to bottom";
            action_button(ui, view, Icon::Play, "build", "Build with AI", hover, title)
                .then(|| Intent::Build(view.visible_paths()))
        }
        HeaderAction::StopBuild => {
            let hover = "Stop the AI build";
            let button = action_button(ui, view, Icon::Stop, "build", "Stop", hover, title);
            button.then_some(Intent::StopBuild)
        }
        HeaderAction::Agents => agents_button(ui, view, title),
        HeaderAction::ArchiveAll => {
            let count = view.visible.len();
            let noun = if count == 1 { "issue" } else { "issues" };
            let hover = format!("Archive all: move the {count} {noun} to {ARCHIVE_DIR}/");
            action_button(
                ui,
                view,
                Icon::Archive,
                "archive_all",
                "Archive all",
                &hover,
                title,
            )
            .then(|| Intent::ArchiveAll(view.visible_paths()))
        }
        HeaderAction::Release => {
            let count = view.visible.len();
            let enabled = count > 0
                || release::has_commits_since_last_release(
                    &view.board.project_dir,
                    view.release_tags,
                );
            let hover = match (enabled, count) {
                (false, _) => "Release: no commits since the last release".to_string(),
                (true, 0) => "Release: write a note, commit, tag and push".to_string(),
                (true, count) => {
                    let noun = if count == 1 { "issue" } else { "issues" };
                    format!("Release: write a note from the {count} {noun}, commit, tag and push")
                }
            };
            ui.add_enabled_ui(enabled, |ui| {
                action_button(ui, view, Icon::Release, "release", "Release", &hover, title)
                    .then(|| Intent::Release(view.visible_paths()))
            })
            .inner
        }
    }
}

fn action_button(
    ui: &mut egui::Ui,
    view: &ColumnView,
    icon: Icon,
    salt: &str,
    short: &str,
    hover: &str,
    title: &mut Option<String>,
) -> bool {
    let id = egui::Id::unique((salt, view.index));
    let response = widgets::icon_button(ui, icon, id, hover, widgets::Tone::Ghost);
    if response.hovered() {
        *title = Some(short.to_string());
    }
    response.clicked()
}

fn new_issue_button(
    ui: &mut egui::Ui,
    view: &ColumnView,
    title: &mut Option<String>,
) -> Option<Intent> {
    let id = egui::Id::unique(("new_issue", view.index));
    let hover = if view.parked {
        "1 parked note  ⌘N"
    } else {
        "New issue  ⌘N"
    };
    let plus = widgets::icon_button(ui, Icon::Plus, id, hover, widgets::Tone::Ghost);
    if plus.hovered() {
        *title = Some("New issue".to_string());
    }
    if view.parked {
        let p = theme::palette(ui);
        ui.painter()
            .circle_filled(plus.rect.right_top() + Vec2::new(-4.0, 4.0), 3.0, p.accent);
    }
    plus.clicked().then_some(Intent::Compose)
}

/// How wide the agents popover is.
const AGENTS_POPUP_WIDTH: f32 = 236.0;

/// Doing's agents: one icon whose title says how many work at once, and whose
/// click opens the selector under it. The header keeps it in view while the
/// popover is open, so it outlives the header's hover.
fn agents_button(
    ui: &mut egui::Ui,
    view: &ColumnView,
    title: &mut Option<String>,
) -> Option<Intent> {
    let agents = view.builds.agents;
    let noun = if agents == 1 { "agent" } else { "agents" };
    let short = format!("{agents} {noun}");
    let id = egui::Id::unique(("agents", view.index));
    let hover = format!("Build agents working at once: {short}  Click to choose");
    let button = widgets::icon_button(ui, Icon::Agents, id, &hover, widgets::Tone::Ghost);
    if button.hovered() {
        *title = Some(short);
    }
    egui::Popup::from_toggle_button_response(&button)
        .align(egui::RectAlign::BOTTOM_END)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(AGENTS_POPUP_WIDTH)
        .frame(widgets::popover_frame(ui).inner_margin(theme::SPACE_3))
        .show(|ui| {
            ui.set_min_width(AGENTS_POPUP_WIDTH);
            agents_selector(ui, view)
        })
        .and_then(|inner| inner.inner)
}

fn agents_selector(ui: &mut egui::Ui, view: &ColumnView) -> Option<Intent> {
    let p = theme::palette(ui);
    let agents = view.builds.agents;
    let mut intent = None;
    ui.vertical(|ui| {
        ui.label(
            RichText::new("Build agents")
                .font(theme::ui_strong())
                .color(p.ink),
        );
        ui.label(
            RichText::new(
                "How many work at once; each takes the next Ready issue. None stops the build.",
            )
            .font(theme::meta())
            .color(p.ink_muted),
        );
        ui.add_space(theme::SPACE_3);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_2;
            ui.add_enabled_ui(agents > 0, |ui| {
                let id = egui::Id::unique(("fewer_agents", view.index));
                let hover = "One build agent fewer";
                let tone = widgets::Tone::Ghost;
                if widgets::icon_button(ui, Icon::Minus, id, hover, tone).clicked() {
                    intent = Some(Intent::Agents(agents - 1));
                }
            });
            let noun = if agents == 1 { "agent" } else { "agents" };
            ui.label(
                RichText::new(format!("{agents} {noun}"))
                    .font(theme::ui_strong())
                    .color(p.ink),
            );
            ui.add_enabled_ui(agents < build::MAX_AGENTS, |ui| {
                let id = egui::Id::unique(("more_agents", view.index));
                let hover = "One more build agent";
                let tone = widgets::Tone::Ghost;
                if widgets::icon_button(ui, Icon::Plus, id, hover, tone).clicked() {
                    intent = Some(Intent::Agents(agents + 1));
                }
            });
        });
    });
    intent
}

/// The column's cards, or its empty well.
/// Returns each card's issue and rect, for placing a drop.
fn column_cards(
    ui: &mut egui::Ui,
    view: &ColumnView,
    folder: &str,
    over: bool,
    intents: &mut Vec<Intent>,
) -> Vec<(PathBuf, Rect)> {
    let column = view.column;
    if view.visible.is_empty() {
        let message = if column.issues.is_empty() {
            if folder.is_empty() {
                "No issues at the root".to_string()
            } else {
                format!("No issues in {folder}")
            }
        } else {
            view.filter.empty_message()
        };
        drop::empty_well(ui, &message, view.width, over);
    }
    let mut cards = Vec::new();
    for issue in &view.visible {
        let (action, rect) = board_card(ui, view, issue);
        cards.push((issue.path.clone(), rect));
        intents.extend(action.map(|action| action.on(&issue.path)));
    }

    cards
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
        question: view
            .questions
            .iter()
            .find(|(path, _)| *path == issue.path)
            .map(|(_, question)| question),
        labelling: discovery::is_editable_column(&view.column.dir),
        active_labels: &view.filter.tokens,
        naming: is_naming(view.namings, &issue.path),
    };
    issue_card(ui, issue, view.width, look, view.labels, extras)
}

/// A card from another column is over this one.
fn foreign_drag_over(view: &ColumnView, drop_response: &egui::Response) -> bool {
    drop_response
        .dnd_hover_payload::<IssueDrag>()
        .is_some_and(|drag| !view.board.in_column(&drag.path, &view.column.dir))
}

pub(super) use crate::widgets::dashed_rect;
