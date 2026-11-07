//! The RefinePanel's pick pane: the challenger's choices as cards, and the
//! note.

use super::{
    Align, CornerRadius, Frame, Layout, NextTurn, Pos2, Refinement, RichText, Sense, Vec2, agent,
    note_id, theme, thinking_pick, widgets,
};

/// The choices on `bg`, then the note; while the challenger answers, what
/// it is doing instead.
pub(super) fn pick_pane(
    ui: &mut egui::Ui,
    refine: &mut Refinement,
    min_height: f32,
) -> egui::InnerResponse<()> {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.bg)
        .inner_margin(Vec2::new(theme::SPACE_5, 20.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height((min_height - 40.0).max(0.0));
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            if let Some(NextTurn::Waiting(answer)) = &refine.next {
                thinking_pick(ui, answer.elapsed(), &refine.turns);
                return;
            }
            let options = refine.options().to_vec();
            if !options.is_empty() {
                question_head(ui, options.len());
            }
            for (index, option) in options.iter().enumerate() {
                let picked = refine.choice == Some(index);
                let card = choice_card(ui, index, option, picked, refine.cursor == Some(index));
                if card.clicked() {
                    refine.toggle_choice(index);
                }
            }
            note_field(ui, refine, options.len());
        })
}

/// "HOW SHOULD WE RESOLVE IT?", and the count of choices at the right.
fn question_head(ui: &mut egui::Ui, count: usize) {
    let p = theme::palette(ui);
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 16.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.label(
                RichText::new("HOW SHOULD WE RESOLVE IT?")
                    .font(theme::ui_label())
                    .extra_letter_spacing(theme::LABEL_TRACKING)
                    .color(p.ink_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{count} choices"))
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
            });
        },
    );
}

/// A choice as a bordered card: its key, its title over its detail, and a
/// radio. Picked, it takes the `accent` edge, `accent-soft` fill and key.
fn choice_card(
    ui: &mut egui::Ui,
    index: usize,
    option: &str,
    picked: bool,
    cursor: bool,
) -> egui::Response {
    let p = theme::palette(ui);
    let (title, detail) = agent::choice_parts(option);
    let pad = Vec2::new(theme::SPACE_4, 10.0);
    let (lead, radio) = (30.0, 24.0);
    let width = ui.available_width();
    let wrap = width - 2.0 * pad.x - lead - radio;
    let painter = ui.painter();
    let title_font = egui::FontId::new(13.0, theme::ui_title().family);
    let title_text = painter.layout(title.clone(), title_font, p.ink, wrap);
    let detail_text = detail
        .map(|detail| painter.layout(detail.to_string(), theme::ui_small(), p.ink_muted, wrap));
    let title_h = title_text.size().y;
    let text_h = title_h + detail_text.as_ref().map_or(0.0, |text| 2.0 + text.size().y);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, text_h + 2.0 * pad.y), Sense::CLICK);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, picked, &title)
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let (fill, edge, key) = if picked {
        (p.accent_soft, egui::Stroke::new(2.0, p.accent), p.accent)
    } else if response.hovered() {
        (
            p.surface_hover,
            egui::Stroke::new(1.0, p.line_strong),
            p.ink_muted,
        )
    } else {
        (p.surface, egui::Stroke::new(1.0, p.line_soft), p.ink_muted)
    };
    let painter = ui.painter();
    let radius = CornerRadius::same(theme::RADIUS_MD);
    painter.rect(rect, radius, fill, edge, egui::StrokeKind::Inside);
    let top = rect.min.y + pad.y;
    let first_line = title_text
        .rows
        .first()
        .map_or(title_h, |row| row.rect().height());
    let line_center = top + first_line / 2.0;
    let number = (index + 1).to_string();
    widgets::paint_kbd_in(ui, &number, Pos2::new(rect.min.x + pad.x, line_center), key);
    let text_x = rect.min.x + pad.x + lead;
    painter.galley(Pos2::new(text_x, top), title_text, p.ink);
    if let Some(text) = detail_text {
        painter.galley(Pos2::new(text_x, top + title_h + 2.0), text, p.ink_muted);
    }
    let dot = Pos2::new(rect.max.x - pad.x - 7.0, line_center);
    if picked {
        painter.circle_filled(dot, 7.0, p.accent);
        painter.circle_filled(dot, 3.0, p.surface);
    } else {
        painter.circle(dot, 6.5, p.surface, egui::Stroke::new(1.0, p.line_strong));
    }
    if cursor {
        widgets::paint_focus_ring(ui, rect, theme::RADIUS_MD);
    }
    response
}

/// The note, whose label follows the choice: a note to it, or the answer.
fn note_field(ui: &mut egui::Ui, refine: &mut Refinement, choices: usize) {
    let p = theme::palette(ui);
    let label = match (refine.choice, choices) {
        (Some(at), _) => format!("Add a note to choice {} (optional)", at + 1),
        (None, 0) => "Your answer".to_string(),
        (None, _) => "Or answer in your own words".to_string(),
    };
    ui.add_space(theme::SPACE_2);
    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
    let label = ui.label(
        RichText::new(label)
            .font(theme::ui_small())
            .color(p.ink_muted),
    );
    let note = ui
        .add(
            egui::TextEdit::multiline(&mut refine.note)
                .id(note_id())
                .font(theme::ui_body())
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .lock_focus(true),
        )
        .labelled_by(label.id);
    widgets::focus_halo(ui, &note);
}
