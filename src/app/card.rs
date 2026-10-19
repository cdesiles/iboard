//! An issue's card, on the board and in ColumnFocus.

use super::*;

/// Draws one issue card. A click selects it, a shift-click picks it into the
/// multi-selection, its Expand action or a double click reads it; dragging the card carries it to another column, with a
/// copy following the cursor. Returns the action the card asked for (if any)
/// and the rect it occupies.
pub(super) fn issue_card(
    ui: &mut egui::Ui,
    issue: &Issue,
    width: f32,
    look: CardLook,
    labels: &Labels,
    extras: CardExtras,
) -> (Option<CardAction>, Rect) {
    let card_id = egui::Id::new(("issue_card", &issue.path));

    // The card is one click-and-drag surface. Labels must not steal the pointer
    // to select text (which also showed an I-beam cursor and hijacked the drag).
    ui.style_mut().interaction.selectable_labels = false;

    // Actions show on hover or selection, never while any card is dragged.
    // Hover is read from last frame's rect, since the body is drawn first.
    let drag = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx());
    let dragging_self = drag.as_ref().is_some_and(|drag| drag.path == issue.path);
    let rect_id = card_id.with("rect");
    let hovered = ui
        .ctx()
        .data(|d| d.get_temp::<Rect>(rect_id))
        .is_some_and(|rect| ui.rect_contains_pointer(rect));
    // An open LabelPicker keeps its card's actions, and so its trigger, shown.
    let picking = label_picker_open(ui.ctx()).as_deref() == Some(issue.path.as_path());
    let show_actions =
        drag.is_none() && (hovered || picking || matches!(look, CardLook::Selected { .. }));

    let scope = ui.scope_builder(
        egui::UiBuilder::new()
            .id(card_id)
            .sense(Sense::click_and_drag()),
        |ui| {
            // The card being dragged stays in place, faded.
            if dragging_self {
                ui.set_opacity(0.4);
            }
            card_body(
                ui,
                issue,
                width,
                look != CardLook::Plain,
                Some(show_actions),
                labels,
                extras,
            )
        },
    );
    let mut body = scope.inner;
    let response = scope.response;
    ui.ctx().data_mut(|d| d.insert_temp(rect_id, response.rect));
    if show_actions && extras.expand {
        expand_action(ui, issue, response.rect, look, &mut body);
    }

    // Dragging the card carries its file path; a drop on a column moves it,
    // a drop on the DropZones dock archives or deletes it.
    response.dnd_set_drag_payload(IssueDrag {
        path: issue.path.clone(),
        archivable: issue.done_at.is_some(),
    });

    // The action icons are their own click targets: while the pointer is over
    // one, the card must not override the pointer cursor the icon set with its
    // own grab cursor below (and a click there must not open the issue).
    let over_action = show_actions
        && ui
            .ctx()
            .pointer_interact_pos()
            .is_some_and(|pos| body.action_rects.iter().any(|rect| rect.contains(pos)));

    // `esc` cancels a drag by dropping the payload: the card stays put.
    if response.dragged() && dragging_self {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

        if let Some(delta) = response.total_drag_delta() {
            drag_ghost(ui, issue, width, labels, response.rect.min + delta);
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
    let shift = ui.input(|i| i.modifiers.shift);
    let click = if response.double_clicked() && !shift {
        Some(CardAction::Open)
    } else if response.clicked() {
        Some(if shift {
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
    let inset = theme::SPACE_1 + 2.0;
    let rect = Rect::from_min_size(
        Pos2::new(
            card.max.x - inset - widgets::CARD_ACTION,
            card.min.y + inset,
        ),
        Vec2::splat(widgets::CARD_ACTION),
    );
    let fill = if look == CardLook::Plain {
        p.surface
    } else {
        p.accent_soft
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), fill);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let expand = widgets::icon_button(
        &mut child,
        Icon::Expand,
        egui::Id::new(("card_expand", &issue.path)),
        "Expand  ↩",
        widgets::Tone::Plain,
    );
    body.action_rects.push(expand.rect);
    if expand.clicked() {
        body.action = Some(CardAction::Open);
    }
}

/// The card the build agent works on: it flashes `status-doing` as it is
/// taken, then its edge breathes and a light runs along its foot.
fn at_work(ui: &egui::Ui, card: Rect, since: Instant) {
    const TAKEN: f32 = 0.6;
    const SWEEP: f32 = 1.6;
    let p = theme::palette(ui);
    let painter = ui.painter();
    let corner = CornerRadius::same(theme::RADIUS_MD);
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

/// The card's edge: 2pt `accent` when selected (scrolled into view when
/// asked), `line-strong` under the pointer.
fn card_edge(ui: &egui::Ui, response: &egui::Response, look: CardLook) {
    let p = theme::palette(ui);
    let edge = match look {
        CardLook::Selected { .. } => Some(Stroke::new(2.0, p.accent)),
        _ if response.hovered() && !response.dragged() => Some(Stroke::new(1.0, p.line_strong)),
        _ => None,
    };
    if let Some(edge) = edge {
        ui.painter().rect_stroke(
            response.rect,
            CornerRadius::same(theme::RADIUS_MD),
            edge,
            StrokeKind::Inside,
        );
    }
    if look == (CardLook::Selected { scroll: true }) {
        response.scroll_to_me(None);
    }
}

/// A copy of the dragged card under the pointer, so the issue stays visible
/// for the whole move instead of only at its ends. Anchored to the card's
/// position plus the total drag delta, the pointer keeps the spot on the card
/// where it was grabbed.
fn drag_ghost(ui: &egui::Ui, issue: &Issue, width: f32, labels: &Labels, at: Pos2) {
    let p = theme::palette(ui);
    let ghost = egui::Area::new(egui::Id::new(("issue_ghost", &issue.path)))
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
                    card_body(ui, issue, width, false, None, labels, CardExtras::default())
                });
            ui.painter().rect_stroke(
                lifted.response.rect,
                CornerRadius::same(theme::RADIUS_MD),
                Stroke::new(1.0, p.line_strong),
                StrokeKind::Inside,
            );
        });
    let response = ghost.response;
    tilt_layer(ui.ctx(), response.layer_id, response.rect.center());
}

/// The lifted card's tilt, as the design system draws it.
const GHOST_TILT_DEG: f32 = -1.5;

/// Rotates what a layer painted this frame about `origin`. Rects and text
/// can't turn in epaint, so each shape becomes a mesh first.
fn tilt_layer(ctx: &egui::Context, layer: egui::LayerId, origin: Pos2) {
    let font_size = ctx.fonts(|fonts| fonts.font_image_size());
    let options = ctx.tessellation_options(|options| *options);
    let mut tessellator =
        egui::epaint::Tessellator::new(ctx.pixels_per_point(), options, font_size, Vec::new());
    let rotation = egui::emath::Rot2::from_angle(GHOST_TILT_DEG.to_radians());
    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        for index in 0..list.next_idx().0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                let mut mesh = egui::epaint::Mesh::default();
                let shape = std::mem::replace(&mut clipped.shape, Shape::Noop);
                tessellator.tessellate_shape(shape, &mut mesh);
                mesh.rotate(rotation, origin);
                clipped.shape = Shape::mesh(mesh);
                clipped.clip_rect = Rect::EVERYTHING;
            });
        }
    });
}

/// One entry of a wide card's facts row: a small `meta` label, with a check
/// icon when the section is written.
pub(super) fn fact(ui: &mut egui::Ui, text: &str, color: Color32, checked: bool) {
    let icon = 12.0;
    let gap = 4.0;
    let text_width = widgets::text_width(ui, text, theme::meta());
    let width = if checked { icon + gap } else { 0.0 } + text_width;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 16.0), Sense::hover());
    let cy = rect.center().y;
    let mut x = rect.min.x;
    if checked {
        let icon_rect = Rect::from_center_size(Pos2::new(x + icon / 2.0, cy), Vec2::splat(icon));
        theme::paint_icon(ui.painter(), Icon::Check, icon_rect, color);
        x += icon + gap;
    }
    ui.painter().text(
        Pos2::new(x, cy),
        egui::Align2::LEFT_CENTER,
        text,
        theme::meta(),
        color,
    );
}

/// A wide card's facts row: the written sections checked in `status-done`, the
/// missing ones as `Out of scope —` in `ink-muted`.
pub(super) fn fact_row(ui: &mut egui::Ui, issue: &Issue, sections: &[String]) {
    let p = theme::palette(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(10.0, 4.0);
        for (text, checked) in sheet::facts(issue, sections) {
            let color = if checked { p.status_done } else { p.ink_muted };
            fact(ui, &text, color, checked);
        }
    });
}

/// The visual body of a card, shared by the card in the board and the ghost
/// that follows the pointer while dragging: title, summary clamped to three
/// lines, file name. A `highlighted` card (new or selected) is `accent-soft`.
/// The meta row ends with a slot for the delete (and, for Done, archive) icon
/// buttons; `actions` says whether they show, and `None` (the lifted copy
/// while dragging) leaves it empty. The slot is always reserved so the file
/// name and date never move when the buttons appear.
pub(super) fn card_body(
    ui: &mut egui::Ui,
    issue: &Issue,
    width: f32,
    highlighted: bool,
    actions: Option<bool>,
    labels: &Labels,
    extras: CardExtras,
) -> CardBody {
    let p = theme::palette(ui);
    // Wide cards (ColumnFocus) get more room: 12×14 padding and 6pt gaps.
    let padding = if extras.wide {
        Margin::symmetric(14, 12)
    } else {
        Margin::symmetric(theme::SPACE_4 as i8, 10)
    };
    let border = 1.0;
    // The frame's border is drawn outside the padding, so take it off too.
    let inner = width - f32::from(padding.left) - f32::from(padding.right) - 2.0 * border;
    let shown = Frame::new()
        .fill(if highlighted {
            p.accent_soft
        } else {
            p.surface
        })
        .stroke(Stroke::new(border, p.line))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(padding)
        .show(ui, |ui| {
            ui.set_width(inner);
            ui.spacing_mut().item_spacing.y = if extras.wide { 6.0 } else { theme::SPACE_2 };
            let (review, label_clicks) = card_labels(ui, issue, labels, &extras);
            card_text(ui, issue, inner, &extras);
            let (mut action, mut action_rects) = ui
                .horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        card_meta(ui, issue, actions, &extras)
                    })
                    .inner
                })
                .inner;
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
            let pill = widgets::label_pill(ui, label, labels.color(label))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!("Filter by {label}"));
            clicks.push((label.clone(), pill.rect, pill.clicked()));
        }
        review
    });
    (review.inner, clicks)
}

/// The review running or ready, or the last verdict: a click opens the
/// issue with it.
fn review_pill(ui: &mut egui::Ui, word: &str, color: Color32, hint: &str) -> egui::Response {
    let response = widgets::pill(ui, word, color).interact(Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, word));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hint)
}

/// The title, the summary (six lines on a wide card, three otherwise) and,
/// on a wide card, its facts row.
fn card_text(ui: &mut egui::Ui, issue: &Issue, inner: f32, extras: &CardExtras) {
    let p = theme::palette(ui);
    ui.add(
        egui::Label::new(
            RichText::new(&issue.title)
                .font(theme::ui_strong())
                .color(p.ink),
        )
        .wrap(),
    );
    if !issue.summary.is_empty() {
        let mut job =
            LayoutJob::simple(issue.summary.clone(), theme::ui_small(), p.ink_muted, inner);
        job.wrap.max_rows = if extras.wide { 6 } else { 3 };
        ui.add(egui::Label::new(job).show_tooltip_when_elided(false));
    }
    if extras.wide {
        fact_row(ui, issue, extras.sections);
    }
}

/// The card's last row, right to left: its actions, the date, then the file
/// name. The action clicked, and every action's rect.
fn card_meta(
    ui: &mut egui::Ui,
    issue: &Issue,
    actions: Option<bool>,
    extras: &CardExtras,
) -> (Option<CardAction>, Vec<Rect>) {
    let p = theme::palette(ui);
    let mut action = None;
    let mut action_rects = Vec::new();
    // Add label, Review and Delete, or Diff and Archive on Done: 70pt.
    let slots = 3.0;
    let slot = Vec2::new(
        slots * widgets::CARD_ACTION + (slots - 1.0) * theme::SPACE_1,
        widgets::CARD_ACTION,
    );
    ui.allocate_ui_with_layout(slot, Layout::right_to_left(Align::Center), |ui| {
        ui.set_min_size(slot);
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        let Some(visible) = actions else {
            return;
        };
        // Hidden actions are also disabled, so a click there opens the card
        // instead.
        if !visible {
            ui.set_invisible();
        }
        card_actions(ui, issue, extras, &mut action, &mut action_rects);
    });
    if let Some(CardBuild::Running(since)) = extras.build {
        build_timer(ui, issue, since);
    }
    card_date_label(ui, issue, extras.wide);
    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        ui.add(
            egui::Label::new(
                RichText::new(issue.file_name())
                    .font(theme::meta())
                    .color(p.ink_muted),
            )
            .truncate(),
        );
    });
    (action, action_rects)
}

/// The icon actions, right to left: Delete (Stop on the card being built),
/// Retry on Doing's cards no agent is on, Review on the selected Todo card and Add label
/// first of the cluster; or Archive and Diff on Done.
fn card_actions(
    ui: &mut egui::Ui,
    issue: &Issue,
    extras: &CardExtras,
    action: &mut Option<CardAction>,
    rects: &mut Vec<Rect>,
) {
    let mut button = |ui: &mut egui::Ui, icon, id: &str, hover: &str, tone, picked| {
        let response =
            widgets::icon_button(ui, icon, egui::Id::new((id, &issue.path)), hover, tone);
        rects.push(response.rect);
        if response.clicked() {
            *action = Some(picked);
        }
        response
    };
    if issue.done_at.is_some() {
        button(
            ui,
            Icon::Archive,
            "card_archive",
            "Archive  A",
            widgets::Tone::Plain,
            CardAction::Archive,
        );
        button(
            ui,
            Icon::Commit,
            "card_diff",
            "Show the diff",
            widgets::Tone::Plain,
            CardAction::Diff,
        );
        return;
    }
    if matches!(extras.build, Some(CardBuild::Running(_))) {
        button(
            ui,
            Icon::Stop,
            "card_stop_build",
            "Stop the AI build",
            widgets::Tone::Plain,
            CardAction::StopBuild,
        );
    } else {
        button(
            ui,
            Icon::Trash,
            "card_delete",
            "Delete  Del",
            widgets::Tone::Danger,
            CardAction::Delete,
        );
    }
    if matches!(extras.build, Some(CardBuild::Retry)) {
        button(
            ui,
            Icon::Refresh,
            "card_retry_build",
            "Retry with AI",
            widgets::Tone::Plain,
            CardAction::RetryBuild,
        );
    }
    if extras.review_button {
        button(
            ui,
            Icon::Review,
            "card_review",
            "Review  ⌘⇧R",
            widgets::Tone::Review,
            CardAction::Review,
        );
    }
    if !extras.labelling {
        return;
    }
    // Add label opens the LabelPicker under it.
    let picking = label_picker_open(ui.ctx()).as_deref() == Some(&issue.path);
    let tone = if picking {
        widgets::Tone::Open
    } else {
        widgets::Tone::Plain
    };
    let add_label = button(
        ui,
        Icon::TagPlus,
        "card_add_label",
        "Add label  L",
        tone,
        CardAction::AddLabel,
    );
    set_label_anchor(ui.ctx(), &issue.path, add_label.rect);
}

/// How long the build agent has been on the issue, `mm:ss` in `status-doing`.
fn build_timer(ui: &mut egui::Ui, issue: &Issue, since: Instant) {
    let color = theme::palette(ui).status_doing;
    ui.scope_builder(
        egui::UiBuilder::new()
            .id(egui::Id::new(("card_build_timer", &issue.path)))
            .sense(Sense::hover()),
        |ui| {
            ui.label(
                RichText::new(stopwatch(since.elapsed()))
                    .font(theme::meta())
                    .color(color),
            )
        },
    );
}

/// Wide cards always show the modified date, as on the board's Done cards:
/// "today 18:02", "yesterday", "2 Oct". Done issues on the board show the
/// day they were done, the exact time on hover.
fn card_date_label(ui: &mut egui::Ui, issue: &Issue, wide: bool) {
    let p = theme::palette(ui);
    let (text, hover) = if wide {
        let Some(modified) = issue.modified else {
            return;
        };
        (card_date(modified), relative_time(modified))
    } else {
        let Some(done_at) = issue.done_at else {
            return;
        };
        (
            done_at.format("%Y-%m-%d").to_string(),
            done_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        )
    };
    ui.add(egui::Label::new(
        RichText::new(text).font(theme::meta()).color(p.ink_muted),
    ))
    .on_hover_text(hover);
}
