//! An issue's card on the board.

use super::dock::IssueDrag;
use super::labelling::{label_picker_open, set_label_anchor};
use super::review::ReviewPill;
use super::{
    Align, Color32, CornerRadius, Frame, Icon, Instant, Issue, Labels, Layout, LayoutJob, Pos2,
    Rect, RichText, Sense, Stroke, StrokeKind, Vec2, clock, theme, widgets,
};
use crate::agent::service::Question;

/// The strip laid over a card's bottom on hover, selection or focus. A card
/// keeps at least this much under its title, so the strip never changes its
/// height.
const STRIP_H: f32 = 28.0;
/// `ui-strong`, `ui-small` and `meta`'s line heights.
const TITLE_LINE: f32 = 18.0;
const SUMMARY_LINE: f32 = 17.0;
const META_LINE: f32 = 16.0;
/// A card's padding, and the edge drawn outside it.
const CARD_PADDING: Vec2 = Vec2::new(theme::SPACE_4, 10.0);
const CARD_BORDER: f32 = 1.0;

/// An issue action available from a card or the detail header.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum CardAction {
    /// Open the issue in the reader, from its Expand action.
    Open,
    /// A plain click: the card becomes the only one selected.
    Select,
    /// Command-click: add the issue to the multi-selection, or take it out.
    Pick,
    /// Shift-click: pick every card from the selected one to this one.
    PickRange,
    /// Delete the issue's file.
    Delete,
    /// Move a done issue to `.archive`.
    Archive,
    /// Show a done issue's diff: every commit that closed it.
    Diff,
    /// Open a done issue with the build agent's log beside it.
    AgentLog,
    /// Add the card's label to the filter.
    FilterLabel(String),
    /// Run the AI review (selected Todo card).
    Review,
    /// Reopen the last review, from the card's verdict pill.
    ShowReview,
    /// Open (or close) the LabelPicker on this card.
    AddLabel,
    /// Stop the agent that works on the card.
    StopBuild,
    /// Hand the issue to the build agent again, from scratch.
    RetryBuild,
    /// Answer the question the build waits on.
    Answer,
}

/// Where a card stands with the AI build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CardBuild {
    /// The build agent works on it, since then.
    Running(Instant),
    /// It waits in Doing with no build running: it can be built again.
    Retry,
}

/// What a card shows beyond its file: the last review's verdict pill, and
/// whether the selected card offers Review.
#[derive(Clone, Copy, Default)]
pub(super) struct CardExtras<'a> {
    /// The review running or ready, else the last verdict.
    pub(super) review_pill: Option<ReviewPill>,
    pub(super) review_button: bool,
    /// The hovered card offers Expand: every card but the picked ones that
    /// do not have focus.
    pub(super) expand: bool,
    pub(super) build: Option<CardBuild>,
    /// The question the build waits on.
    pub(super) question: Option<&'a Question>,
    /// Offers Add label: Doing and Done keep the labels they have.
    pub(super) labelling: bool,
    /// The labels the board is filtered by: they show as active.
    pub(super) active_labels: &'a [String],
    /// Its title is still to land.
    pub(super) naming: bool,
    /// When git, or the file's own times, says it was done, and how long its
    /// last build took: facts from [`CardFacts`], not the issue file.
    pub(super) done_at: Option<chrono::DateTime<chrono::Local>>,
    pub(super) built_in: Option<std::time::Duration>,
}

/// How a card stands out from the others.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum CardLook {
    Plain,
    /// Just created by the composer: `accent-soft` fading to `surface`, with
    /// `left` of it still there, from 1 to 0.
    Fresh {
        left: f32,
    },
    /// Selected: `accent` edge on `accent-soft`; `scroll` brings it into view.
    Selected {
        scroll: bool,
    },
}

/// Draws one issue card. A click selects it, a command-click picks it into the
/// multi-selection, a shift-click picks the range up to it, its Expand action or a double click reads it; dragging the
/// card carries it to another column, with a copy following the cursor. Returns the action the card asked for (if any)
/// and the rect it occupies.
pub(super) fn issue_card(
    ui: &mut egui::Ui,
    issue: &Issue,
    width: f32,
    look: CardLook,
    labels: &Labels,
    extras: CardExtras,
) -> (Option<CardAction>, Rect) {
    let card_id = egui::Id::unique(("issue_card", &issue.path));

    // The card is one click-and-drag surface. Labels must not steal the pointer
    // to select text (which also showed an I-beam cursor and hijacked the drag).
    ui.style_mut().interaction.selectable_labels = false;

    // Hover is read from last frame's rect, since the body is drawn first.
    let drag = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx());
    let dragging_self = drag.as_ref().is_some_and(|drag| drag.path == issue.path);
    let rect_id = card_id.with("rect");
    let hovered = ui
        .ctx()
        .data(|d| d.get_temp::<Rect>(rect_id))
        .is_some_and(|rect| ui.rect_contains_pointer(rect));
    // An open LabelPicker keeps its card's strip, and so its trigger, shown.
    let picking = label_picker_open(ui.ctx()).as_deref() == Some(issue.path.as_path());
    let wanted = hovered
        || picking
        || matches!(look, CardLook::Selected { .. })
        || focus_within(ui.ctx(), issue, card_id, &extras);
    let running = matches!(extras.build, Some(CardBuild::Running(_)));
    let strip = if drag.is_none() {
        wanted || running
    } else {
        running && !dragging_self
    };

    let scope = ui.scope_builder(
        egui::UiBuilder::new()
            .scope_id(card_id)
            .sense(Sense::click_and_drag()),
        |ui| {
            // The card being dragged stays in place, faded.
            if dragging_self {
                ui.set_opacity(0.4);
            }
            card_body(ui, issue, width, look, strip, labels, extras)
        },
    );
    let mut body = scope.inner;
    let response = scope.response;
    ui.ctx().accesskit_node_builder(card_id, |node| {
        node.set_role(egui::accesskit::Role::Group);
        node.set_label(issue.title.as_str());
    });
    ui.ctx().data_mut(|d| d.insert_temp(rect_id, response.rect));
    if strip {
        card_strip(ui, issue, response.rect, look, &extras, &mut body);
        if extras.expand {
            expand_action(ui, issue, response.rect, look, &mut body);
        }
    }

    // Dragging the card carries its file path; a drop on a column moves it,
    // a drop on the DropZones dock archives or deletes it.
    response.dnd_set_drag_payload(IssueDrag {
        path: issue.path.clone(),
        archivable: extras.done_at.is_some(),
    });

    // The action icons are their own click targets: while the pointer is over
    // one, the card must not override the pointer cursor the icon set with its
    // own grab cursor below (and a click there must not open the issue).
    let over_action = ui
        .ctx()
        .pointer_interact_pos()
        .is_some_and(|pos| body.action_rects.iter().any(|rect| rect.contains(pos)));

    // `esc` cancels a drag by dropping the payload: the card stays put.
    if response.dragged() && dragging_self {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

        if let Some(delta) = response.total_drag_delta() {
            drag_ghost(ui, issue, width, labels, extras, response.rect.min + delta);
        }
    } else if response.hovered() && drag.is_none() && !over_action {
        // Grab cursor: this card can be clicked or picked up, but an action
        // button under the pointer keeps its own pointing hand.
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    if let Some(CardBuild::Running(since)) = extras.build {
        at_work(ui, response.rect, since);
    }
    card_edge(ui, &response, look);
    if let Some(question) = extras.question {
        question_pin(ui, issue, response.rect, question, &mut body);
    }
    let modifiers = ui.input(|i| i.modifiers);
    let click = if response.double_clicked() && !modifiers.shift && !modifiers.command {
        Some(CardAction::Open)
    } else if response.clicked() {
        Some(if modifiers.shift {
            CardAction::PickRange
        } else if modifiers.command {
            CardAction::Pick
        } else {
            CardAction::Select
        })
    } else {
        None
    };
    let action = body.action.or(click.filter(|_| !over_action));

    (action, response.rect)
}

/// The keyboard is on the card or on one of its actions.
fn focus_within(ctx: &egui::Context, issue: &Issue, card: egui::Id, extras: &CardExtras) -> bool {
    let Some(focused) = ctx.memory(egui::Memory::focused) else {
        return false;
    };
    focused == card
        || focused == egui::Id::unique(("card_expand", &issue.path))
        || card_buttons(extras.done_at.is_some(), extras)
            .iter()
            .any(|button| focused == egui::Id::unique((button.id, &issue.path)))
}

/// Where the strip lies: the card's bottom, inside its edge.
fn strip_rect(card: Rect) -> Rect {
    let inner = card.shrink(CARD_BORDER);
    inner.with_min_y(inner.max.y - STRIP_H)
}

/// The strip, over the card's bottom on the card's own fill: the file name,
/// Done's date, the build's time, then the actions. It covers the last summary
/// line or the date row.
fn card_strip(
    ui: &mut egui::Ui,
    issue: &Issue,
    card: Rect,
    look: CardLook,
    extras: &CardExtras,
    body: &mut CardBody,
) {
    let p = theme::palette(ui);
    let strip = strip_rect(card);
    let corner = theme::RADIUS_MD - 1;
    ui.painter().rect_filled(
        strip,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: corner,
            se: corner,
        },
        card_fill(p, look),
    );
    let row = Rect::from_min_max(
        strip.min + Vec2::new(CARD_PADDING.x, 0.0),
        strip.max - Vec2::new(6.0, 2.0),
    );
    let mut ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row)
            .layout(Layout::right_to_left(Align::Center)),
    );
    ui.spacing_mut().item_spacing.x = theme::SPACE_1;
    card_actions(
        &mut ui,
        issue,
        extras,
        &mut body.action,
        &mut body.action_rects,
    );
    ui.add_space(theme::SPACE_3 - theme::SPACE_1);
    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
    if let Some(CardBuild::Running(since)) = extras.build {
        build_timer(&mut ui, issue, since);
    } else if let Some(took) = extras.built_in {
        ui.label(meta_text(clock(took), p.ink_muted));
    }
    if let Some(done_at) = extras.done_at {
        ui.label(meta_text(done_date(done_at), p.ink_muted))
            .on_hover_text(done_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, false));
    }
    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        ui.add(egui::Label::new(meta_text(issue.file_name(), p.ink_muted)).truncate());
    });
}

/// Expand, in the card's top right corner over the card's fill: it opens the
/// Reader.
fn expand_action(
    ui: &mut egui::Ui,
    issue: &Issue,
    card: Rect,
    look: CardLook,
    body: &mut CardBody,
) {
    let p = theme::palette(ui);
    let inset = 6.0;
    let rect = Rect::from_min_size(
        Pos2::new(
            card.max.x - inset - widgets::CARD_ACTION,
            card.min.y + inset,
        ),
        Vec2::splat(widgets::CARD_ACTION),
    );
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::RADIUS_SM),
        card_fill(p, look),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let expand = widgets::icon_button(
        &mut child,
        Icon::Expand,
        egui::Id::unique(("card_expand", &issue.path)),
        "Expand  ↩",
        widgets::Tone::Plain,
    );
    body.action_rects.push(expand.rect);
    if expand.clicked() {
        body.action = Some(CardAction::Open);
    }
}

/// The question pin: a `bubble` in `status-doing` on its own pale tint, at
/// the card's top right and left of Expand, so a build's question is visible
/// before the pointer arrives. A click opens the ticket on its question.
fn question_pin(
    ui: &mut egui::Ui,
    issue: &Issue,
    card: Rect,
    question: &Question,
    body: &mut CardBody,
) {
    let inset = 6.0;
    let rect = Rect::from_min_size(
        Pos2::new(
            card.max.x - inset - 2.0 * widgets::CARD_ACTION - 4.0,
            card.min.y + inset,
        ),
        Vec2::splat(widgets::CARD_ACTION),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let title = question.title.trim();
    let hover = if title.is_empty() {
        "Question".to_string()
    } else {
        format!("Question: {title}")
    };
    let pin = widgets::icon_button(
        &mut child,
        Icon::Bubble,
        egui::Id::unique(("card_question_pin", &issue.path)),
        &hover,
        widgets::Tone::Doing,
    );
    body.action_rects.push(pin.rect);
    if pin.clicked() {
        body.action = Some(CardAction::Answer);
    }
}

/// The card the build agent works on: it flashes `status-doing` as it is
/// taken, then its edge breathes and a light runs along its foot. With reduced
/// motion the edge stays steady and nothing else moves.
fn at_work(ui: &egui::Ui, card: Rect, since: Instant) {
    const TAKEN: f32 = 0.6;
    const SWEEP: f32 = 1.6;
    let p = theme::palette(ui);
    let painter = ui.painter();
    let corner = CornerRadius::same(theme::RADIUS_MD);
    if theme::reduced_motion(ui.ctx()) {
        let edge = Stroke::new(2.0, p.status_doing);
        painter.rect_stroke(card, corner, edge, StrokeKind::Inside);
        return;
    }
    let t = since.elapsed().as_secs_f32();
    if t < TAKEN {
        let flash = p.status_doing.gamma_multiply(0.3 * (1.0 - t / TAKEN));
        painter.rect_filled(card, corner, flash);
    }
    let breath = 0.6 + 0.4 * (t * std::f32::consts::PI).cos();
    let edge = Stroke::new(2.0, p.status_doing.gamma_multiply(breath));
    painter.rect_stroke(card, corner, edge, StrokeKind::Inside);
    let track = card.shrink2(Vec2::new(f32::from(theme::RADIUS_MD), 0.0));
    let span = track.width() * 0.3;
    let start = track.min.x - span + (t / SWEEP).fract() * (track.width() + span);
    let y = card.max.y - 3.0;
    let light = Rect::from_min_max(
        Pos2::new(start.max(track.min.x), y - 1.0),
        Pos2::new((start + span).min(track.max.x), y + 1.0),
    );
    if light.width() > 0.0 {
        painter.rect_filled(light, CornerRadius::same(1), p.status_doing);
    }
    ui.ctx().request_repaint();
}

/// A card's ground: `surface`, `accent-soft` when selected, and between the
/// two while a new card fades.
fn card_fill(p: &theme::Palette, look: CardLook) -> Color32 {
    match look {
        CardLook::Plain => p.surface,
        CardLook::Fresh { left } => p.surface.lerp_to_gamma(p.accent_soft, left),
        CardLook::Selected { .. } => p.accent_soft,
    }
}

/// The card's edge: 2pt `accent` when selected (scrolled into view when
/// asked), `line-strong` under the pointer.
fn card_edge(ui: &egui::Ui, response: &egui::Response, look: CardLook) {
    let p = theme::palette(ui);
    let edge = match look {
        CardLook::Selected { .. } => Some(Stroke::new(2.0, p.accent)),
        _ => {
            let hovered = response.hovered() && !response.dragged();
            let fade = widgets::hover_fade(ui.ctx(), response.id, hovered);
            (fade > 0.0).then(|| Stroke::new(1.0, p.line_strong.gamma_multiply(fade)))
        }
    };
    if let Some(edge) = edge {
        ui.painter().rect_stroke(
            response.rect,
            CornerRadius::same(theme::RADIUS_MD),
            edge,
            StrokeKind::Inside,
        );
    }
    widgets::focus_ring(ui, response, theme::RADIUS_MD);
    if look == (CardLook::Selected { scroll: true }) {
        response.scroll_to_me(None);
    }
}

/// A copy of the dragged card under the pointer, so the issue stays visible
/// for the whole move instead of only at its ends. Anchored to the card's
/// position plus the total drag delta, the pointer keeps the spot on the card
/// where it was grabbed.
fn drag_ghost(
    ui: &egui::Ui,
    issue: &Issue,
    width: f32,
    labels: &Labels,
    extras: CardExtras,
    at: Pos2,
) {
    let p = theme::palette(ui);
    let ghost = egui::Area::new(egui::Id::unique(("issue_ghost", &issue.path)))
        .order(egui::Order::Tooltip)
        .fixed_pos(at)
        .constrain(false)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            ui.set_max_width(width);
            // The lifted copy: `shadow-popover` and a `line-strong` edge.
            let lifted = Frame::new()
                .shadow(ui.visuals().popup_shadow)
                .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                .show(ui, |ui| {
                    card_body(
                        ui,
                        issue,
                        width,
                        CardLook::Plain,
                        false,
                        labels,
                        CardExtras {
                            done_at: extras.done_at,
                            built_in: extras.built_in,
                            ..CardExtras::default()
                        },
                    )
                });
            ui.painter().rect_stroke(
                lifted.response.rect,
                CornerRadius::same(theme::RADIUS_MD),
                Stroke::new(1.0, p.line_strong),
                StrokeKind::Inside,
            );
        });
    let response = ghost.response;
    widgets::tilt_layer(
        ui.ctx(),
        response.layer_id,
        response.rect.center(),
        GHOST_TILT_DEG,
    );
}

/// The lifted card's tilt, as the design system draws it.
const GHOST_TILT_DEG: f32 = -1.5;

/// What `card_body` drew: the action a child button asked for, and the rects
/// those buttons cover.
struct CardBody {
    action: Option<CardAction>,
    action_rects: Vec<Rect>,
}

/// The visual body of a card, shared by the card in the board and the ghost
/// that follows the pointer while dragging, `space-3` between its rows: labels,
/// title, summary, Done's date, "Naming…". While the `strip` shows it holds the
/// date, so the row stays but its text does not.
fn card_body(
    ui: &mut egui::Ui,
    issue: &Issue,
    width: f32,
    look: CardLook,
    strip: bool,
    labels: &Labels,
    extras: CardExtras,
) -> CardBody {
    let p = theme::palette(ui);
    // The frame's border is drawn outside the padding, so take it off too.
    let inner = width - 2.0 * CARD_PADDING.x - 2.0 * CARD_BORDER;
    let shown = Frame::new()
        .fill(card_fill(p, look))
        .stroke(Stroke::new(CARD_BORDER, p.line_soft))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(CARD_PADDING)
        .show(ui, |ui| {
            ui.set_width(inner);
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            let pinned = if extras.question.is_some() {
                2.0 * widgets::CARD_ACTION + theme::SPACE_2
            } else {
                0.0
            };
            let (review, label_clicks) = card_labels(ui, issue, labels, &extras);
            card_text(ui, issue, (inner - pinned).max(60.0));
            let mut action = None;
            let mut action_rects = Vec::new();
            if let Some(question) = extras.question {
                let note = question_note(ui, issue, question);
                action_rects.push(note.rect);
                if note.clicked() {
                    action = Some(CardAction::Answer);
                }
            }
            card_tail(ui, issue, inner, strip, extras);
            if let Some(review) = review {
                action_rects.push(review.rect);
                if review.clicked() {
                    action = Some(CardAction::ShowReview);
                }
            }
            // Each label is its own click target: it filters the board.
            for (label, rect, clicked) in label_clicks {
                action_rects.push(rect);
                if clicked {
                    action = Some(CardAction::FilterLabel(label));
                }
            }
            (action, action_rects)
        });
    CardBody {
        action: shown.inner.0,
        action_rects: shown.inner.1,
    }
}

/// What follows the summary: Done's date and "Naming…". With neither, nor a
/// summary, the card keeps the strip's room under its title.
fn card_tail(ui: &mut egui::Ui, issue: &Issue, inner: f32, strip: bool, extras: CardExtras) {
    let p = theme::palette(ui);
    if let Some(done_at) = extras.done_at {
        if strip {
            ui.allocate_exact_size(Vec2::new(inner, META_LINE), Sense::hover());
        } else {
            ui.label(meta_text(done_date(done_at), p.ink_muted));
        }
    }
    if extras.naming {
        ui.label(meta_text("Naming…".to_owned(), p.ink_muted));
    }
    if issue.summary.is_empty() && extras.done_at.is_none() && !extras.naming {
        ui.add_space(STRIP_H - CARD_PADDING.y - theme::SPACE_3);
    }
}

/// The top row, above the title: the last review's verdict first, then the
/// labels, 4pt apart and wrapping; neither, no row. The verdict's response,
/// and each label with its rect and whether it was clicked.
fn card_labels(
    ui: &mut egui::Ui,
    issue: &Issue,
    labels: &Labels,
    extras: &CardExtras,
) -> (Option<egui::Response>, Vec<(String, Rect, bool)>) {
    let mut clicks = Vec::new();
    if issue.labels.is_empty() && extras.review_pill.is_none() {
        return (None, clicks);
    }
    let review = ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_2);
        let review = extras
            .review_pill
            .map(|(word, color, hint)| review_pill(ui, word, color, hint));
        for label in &issue.labels {
            if label.starts_with("group:") {
                continue;
            }
            let shown = label.strip_prefix("repo:").unwrap_or(label);
            let active = extras.active_labels.contains(label);
            let pill = widgets::label_pill_lit(ui, shown, labels.style(label), active)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!("Filter by {label}"));
            clicks.push((label.clone(), pill.rect, pill.clicked()));
        }
        review
    });
    (review.inner, clicks)
}

/// The question the build waits on, on its card: a click opens its answer
/// form.
fn question_note(ui: &mut egui::Ui, issue: &Issue, question: &Question) -> egui::Response {
    let p = theme::palette(ui);
    let shown = Frame::new()
        .fill(p.surface_sunken)
        .stroke(Stroke::new(1.0, p.status_doing.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                let (icon, _) =
                    ui.allocate_exact_size(Vec2::splat(theme::ICON_SIZE), Sense::hover());
                theme::paint_icon(ui.painter(), Icon::Bubble, icon, p.status_doing);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(meta_text("Question".to_owned(), p.status_doing));
                    ui.add(
                        egui::Label::new(
                            RichText::new(&question.title)
                                .font(theme::ui_small())
                                .color(p.ink),
                        )
                        .truncate(),
                    );
                });
            });
        });
    let response = ui.interact(
        shown.response.rect,
        egui::Id::unique(("card_question", &issue.path)),
        Sense::click(),
    );
    let (label, hint) = if question.answerable() {
        (
            format!("Answer: {}", question.title),
            "Open the ticket to answer",
        )
    } else {
        (
            format!("Waiting: {}", question.title),
            "Open the ticket to see how to answer",
        )
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hint)
}

/// The review running or ready, or the last verdict: a click opens the
/// issue with it.
fn review_pill(ui: &mut egui::Ui, word: &str, color: Color32, hint: &str) -> egui::Response {
    let response = widgets::pill(ui, word, color).interact(Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, word));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hint)
}

/// The title and the summary, each clamped to two lines with an ellipsis.
fn card_text(ui: &mut egui::Ui, issue: &Issue, inner: f32) {
    let p = theme::palette(ui);
    let title = clamped(&issue.title, theme::ui_strong(), TITLE_LINE, p.ink, inner);
    ui.add(egui::Label::new(title).show_tooltip_when_elided(false));
    if !issue.summary.is_empty() {
        let summary = clamped(
            &issue.summary,
            theme::ui_small(),
            SUMMARY_LINE,
            p.ink_muted,
            inner,
        );
        ui.add(egui::Label::new(summary).show_tooltip_when_elided(false));
    }
}

/// `text` in at most two lines of `width`, cut with an ellipsis.
fn clamped(text: &str, font: egui::FontId, line: f32, color: Color32, width: f32) -> LayoutJob {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: font,
            line_height: Some(line),
            color,
            ..Default::default()
        },
    );
    job.wrap.max_width = width;
    job.wrap.max_rows = 2;
    job
}

/// `meta` text on its 16pt line.
fn meta_text(text: String, color: Color32) -> RichText {
    RichText::new(text)
        .font(theme::meta())
        .line_height(Some(META_LINE))
        .color(color)
}

/// Draws the card's buttons. Add label opens the LabelPicker under it.
fn card_actions(
    ui: &mut egui::Ui,
    issue: &Issue,
    extras: &CardExtras,
    action: &mut Option<CardAction>,
    rects: &mut Vec<Rect>,
) {
    let picking = label_picker_open(ui.ctx()).as_deref() == Some(&issue.path);
    for button in card_buttons(extras.done_at.is_some(), extras) {
        let add_label = button.action == CardAction::AddLabel;
        let tone = if add_label && picking {
            widgets::Tone::Open
        } else {
            button.tone
        };
        let id = egui::Id::unique((button.id, &issue.path));
        let response = widgets::icon_button(ui, button.icon, id, button.hover, tone);
        rects.push(response.rect);
        if add_label {
            set_label_anchor(ui, &issue.path, response.rect);
        }
        if response.clicked() {
            *action = Some(button.action);
        }
    }
}

/// One of a card's icon buttons; tests find it by `id` and the issue's path.
struct CardButton {
    icon: Icon,
    id: &'static str,
    hover: &'static str,
    tone: widgets::Tone,
    action: CardAction,
}

/// The buttons a card offers, right to left: Delete (Stop on the card being
/// built), Retry on Doing's cards no agent is on, Review on the selected Todo
/// card and Add label first of the cluster; or Archive, Diff and Agent log on
/// Done.
fn card_buttons(done: bool, extras: &CardExtras) -> Vec<CardButton> {
    let button = |icon, id, hover, tone, action| CardButton {
        icon,
        id,
        hover,
        tone,
        action,
    };
    if done {
        return vec![
            button(
                Icon::Archive,
                "card_archive",
                "Archive  A",
                widgets::Tone::Plain,
                CardAction::Archive,
            ),
            button(
                Icon::Commit,
                "card_diff",
                "Show the diff",
                widgets::Tone::Plain,
                CardAction::Diff,
            ),
            button(
                Icon::Log,
                "card_agent_log",
                "Show the agent log",
                widgets::Tone::Plain,
                CardAction::AgentLog,
            ),
        ];
    }
    let mut buttons = vec![if matches!(extras.build, Some(CardBuild::Running(_))) {
        button(
            Icon::Stop,
            "card_stop_build",
            "Stop the AI build",
            widgets::Tone::Plain,
            CardAction::StopBuild,
        )
    } else {
        button(
            Icon::Trash,
            "card_delete",
            "Delete  Del",
            widgets::Tone::Danger,
            CardAction::Delete,
        )
    }];
    if matches!(extras.build, Some(CardBuild::Retry)) {
        buttons.push(button(
            Icon::Refresh,
            "card_retry_build",
            "Retry with AI",
            widgets::Tone::Plain,
            CardAction::RetryBuild,
        ));
    }
    if extras.review_button {
        buttons.push(button(
            Icon::Review,
            "card_review",
            "Review  ⌘⇧R",
            widgets::Tone::Review,
            CardAction::Review,
        ));
    }
    if extras.labelling {
        buttons.push(button(
            Icon::TagPlus,
            "card_add_label",
            "Add label  L",
            widgets::Tone::Plain,
            CardAction::AddLabel,
        ));
    }
    buttons
}

/// How long the build agent has been on the issue, in `status-doing`.
fn build_timer(ui: &mut egui::Ui, issue: &Issue, since: Instant) {
    let color = theme::palette(ui).status_doing;
    ui.scope_builder(
        egui::UiBuilder::new()
            .scope_id(egui::Id::unique(("card_build_timer", &issue.path)))
            .sense(Sense::hover()),
        |ui| ui.label(meta_text(clock(since.elapsed()), color)),
    );
}

/// The day a Done issue was finished.
fn done_date(done_at: chrono::DateTime<chrono::Local>) -> String {
    done_at.format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(done: bool, extras: CardExtras) -> Vec<CardAction> {
        card_buttons(done, &extras)
            .into_iter()
            .map(|button| button.action)
            .collect()
    }

    /// The rects a running card paints, and whether it asks for the next frame.
    fn at_work_paints(reduce: bool) -> (Vec<egui::epaint::RectShape>, bool) {
        let ctx = egui::Context::default();
        theme::set_reduced_motion(&ctx, reduce);
        let card = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(200.0, 80.0));
        let frame = || {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                at_work(ui, card, Instant::now());
            })
        };
        for _ in 0..3 {
            frame().textures_delta.clear();
        }
        let mut output = frame();
        output.textures_delta.clear();
        let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        let rects = output
            .shapes
            .into_iter()
            .filter_map(|clipped| match clipped.shape {
                egui::Shape::Rect(rect) => Some(rect),
                _ => None,
            })
            .collect();
        (rects, delay.is_zero())
    }

    #[test]
    fn a_running_card_holds_still_under_reduced_motion() {
        let (rects, repaints) = at_work_paints(false);
        assert_eq!(rects.len(), 3, "flash, edge and a light along its foot");
        assert!(repaints);

        let (rects, repaints) = at_work_paints(true);
        let [edge] = rects.as_slice() else {
            panic!("only the edge: {rects:?}");
        };
        assert_eq!(edge.stroke.color.a(), 255, "steady");
        assert_eq!(edge.fill, Color32::TRANSPARENT);
        assert!(!repaints, "nothing moves");
    }

    #[test]
    fn each_card_offers_its_buttons_right_to_left() {
        let todo = CardExtras {
            labelling: true,
            ..CardExtras::default()
        };
        assert_eq!(
            offered(false, todo),
            [CardAction::Delete, CardAction::AddLabel]
        );
        let selected = CardExtras {
            review_button: true,
            ..todo
        };
        assert_eq!(
            offered(false, selected),
            [CardAction::Delete, CardAction::Review, CardAction::AddLabel]
        );
        assert_eq!(
            offered(true, selected),
            [CardAction::Archive, CardAction::Diff, CardAction::AgentLog],
            "a Done card offers its own three, whatever else it has"
        );
        let building = CardExtras {
            build: Some(CardBuild::Running(Instant::now())),
            ..CardExtras::default()
        };
        assert_eq!(offered(false, building), [CardAction::StopBuild]);
        let retry = CardExtras {
            build: Some(CardBuild::Retry),
            ..CardExtras::default()
        };
        assert_eq!(
            offered(false, retry),
            [CardAction::Delete, CardAction::RetryBuild]
        );
    }

    #[test]
    fn a_new_card_fades_from_accent_soft_to_surface() {
        let p = &theme::LIGHT;
        assert_eq!(card_fill(p, CardLook::Fresh { left: 1.0 }), p.accent_soft);
        assert_eq!(card_fill(p, CardLook::Fresh { left: 0.0 }), p.surface);
        let halfway = card_fill(p, CardLook::Fresh { left: 0.5 });
        assert!(halfway != p.accent_soft && halfway != p.surface);
    }
}
