//! The RefinePanel's head and its why pane: the earlier rounds, the
//! challenger's TL;DR and its reasoning.

use super::{
    Align, CommonMarkCache, CommonMarkViewer, CornerRadius, Frame, Layout, MEASURE, Margin,
    NextTurn, Pos2, Rect, Refinement, RichText, Sense, Turn, Vec2, agent, markdown, side_panel,
    theme, thinking_why,
};

/// "Refining `todo/x.md`", the role as a pill, and the agent at the right,
/// which truncates first.
pub(super) fn refine_head(ui: &mut egui::Ui, shown: &str, agent: &str, round: usize) {
    let p = theme::palette(ui);
    side_panel::head_frame(ui)
        .inner_margin(Vec2::new(theme::SPACE_5, 6.0))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), theme::CONTROL_H),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    let review = |text: &str, font| RichText::new(text).font(font).color(p.review);
                    ui.label(review("Refining", theme::ui_strong()));
                    ui.label(review(shown, theme::ui_strong()));
                    role_pill(ui, "Challenger");
                    if round > 1 {
                        ui.label(
                            RichText::new(format!("round {round}"))
                                .font(theme::meta())
                                .color(p.review),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(agent).font(theme::meta()).color(p.ink_muted),
                            )
                            .truncate(),
                        );
                    });
                },
            );
        });
}

/// The agent's role: `review` text and edge on `surface`, 18pt tall.
fn role_pill(ui: &mut egui::Ui, role: &str) {
    let p = theme::palette(ui);
    let font = egui::FontId::new(11.0, theme::ui_button().family);
    let text = ui
        .painter()
        .layout_no_wrap(role.to_string(), font, p.review);
    let size = Vec2::new(text.size().x + 14.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, role));
    ui.painter().rect(
        rect,
        CornerRadius::same(theme::RADIUS_PILL),
        p.surface,
        egui::Stroke::new(1.0, p.review),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - text.size() / 2.0, text, p.review);
}

/// The earlier rounds, folded; the challenger's TL;DR, then its reasoning
/// (folded too on a narrow panel). Returns whether the fold was clicked.
pub(super) fn why_pane(
    ui: &mut egui::Ui,
    refine: &Refinement,
    cache: &mut CommonMarkCache,
    narrow: bool,
) -> bool {
    let p = theme::palette(ui);
    let mut unfold = false;
    let margin = if narrow {
        Vec2::new(20.0, 16.0)
    } else {
        Vec2::new(theme::SPACE_6, 20.0)
    };
    Frame::new().inner_margin(margin).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = theme::SPACE_4;
        rounds(ui, refine);
        if refine.is_running() {
            thinking_why(ui);
            return;
        }
        if let NextTurn::Failed(error) = &refine.next {
            ui.label(RichText::new(format!("{}: {error}", refine.agent)).color(p.danger));
        }
        let Some((at, turn)) = refine
            .turns
            .iter()
            .enumerate()
            .rev()
            .find_map(|(at, turn)| turn.challenge().map(|turn| (at, turn)))
        else {
            return;
        };
        let folds = narrow && turn.tldr.is_some();
        if let Some(tldr) = &turn.tldr {
            tldr_block(ui, tldr);
        }
        if folds {
            unfold = reasoning_fold(ui, &turn.text, refine.reasoning_open);
        }
        let reasoning_shown = !folds || refine.reasoning_open;
        if reasoning_shown {
            reasoning(ui, at, &turn.text, cache);
        }
        if turn.proposal.is_some() {
            ui.label(
                RichText::new("Proposed a refined issue: Apply to draft to read it.")
                    .font(theme::meta())
                    .color(p.review),
            );
        }
    });
    unfold
}

/// One receipt per answer already sent: "Round 1 · you chose **Split it**".
fn rounds(ui: &mut egui::Ui, refine: &Refinement) {
    let p = theme::palette(ui);
    let answers = refine.turns.iter().filter_map(|turn| match turn {
        Turn::Author { choice, note } => Some((choice, note)),
        Turn::Challenger(_) => None,
    });
    for (round, (choice, note)) in answers.enumerate() {
        let round = round + 1;
        let (verb, said, note) = match choice {
            Some(choice) => ("you chose", agent::choice_parts(choice).0, note.as_str()),
            None => (
                "you answered",
                format!("“{}”", note.lines().next().unwrap_or_default()),
                "",
            ),
        };
        let mut job = egui::text::LayoutJob::default();
        let muted = egui::TextFormat::simple(theme::ui_small(), p.ink_muted);
        let strong = egui::TextFormat::simple(theme::ui_button(), p.ink);
        job.append(&format!("Round {round} · {verb} "), 0.0, muted.clone());
        job.append(&said, 0.0, strong);
        if let Some(first) = note.lines().next().filter(|line| !line.is_empty()) {
            job.append(&format!(", with the note “{first}”"), 0.0, muted);
        }
        Frame::new()
            .fill(p.surface_sunken)
            .corner_radius(CornerRadius::same(theme::RADIUS_SM))
            .inner_margin(Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_max_width(MEASURE);
                ui.add(egui::Label::new(job).truncate());
            });
    }
}

/// The challenger's advice at a glance: `ui-title` beside a 3pt `review`
/// rule, under a `TL;DR` caption.
fn tldr_block(ui: &mut egui::Ui, tldr: &str) {
    let p = theme::palette(ui);
    let rect = Frame::new()
        .inner_margin(Margin {
            left: 15,
            right: 0,
            top: 2,
            bottom: 2,
        })
        .show(ui, |ui| {
            ui.set_max_width(MEASURE);
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            ui.label(
                RichText::new("TL;DR")
                    .font(theme::ui_label())
                    .color(p.review),
            );
            ui.label(RichText::new(tldr).font(theme::ui_title()).color(p.ink));
        })
        .response
        .rect;
    let rule = Rect::from_min_max(rect.min, Pos2::new(rect.min.x + 3.0, rect.max.y));
    ui.painter().rect_filled(rule, 0.0, p.review);
}

/// "Show the reasoning (3 paragraphs)", or "Hide the reasoning" once open.
fn reasoning_fold(ui: &mut egui::Ui, text: &str, open: bool) -> bool {
    let p = theme::palette(ui);
    let label = if open {
        "Hide the reasoning".to_string()
    } else {
        match markdown::blocks(text).len() {
            1 => "Show the reasoning (1 paragraph)".to_string(),
            count => format!("Show the reasoning ({count} paragraphs)"),
        }
    };
    ui.add(
        egui::Label::new(RichText::new(label).font(theme::ui_small()).color(p.accent))
            .sense(Sense::click()),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .clicked()
}

/// The challenger's paragraphs, in markdown at reading size and measure.
fn reasoning(ui: &mut egui::Ui, at: usize, text: &str, cache: &mut CommonMarkCache) {
    ui.scope(|ui| {
        ui.set_max_width(MEASURE);
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Body, theme::read_body());
        ui.spacing_mut().item_spacing.y = theme::SPACE_3;
        ui.push_id(("turn", at), |ui| {
            for (index, block) in markdown::blocks(text).into_iter().enumerate() {
                ui.push_id(index, |ui| {
                    CommonMarkViewer::new().show(ui, cache, &text[block]);
                });
            }
        });
    });
}
