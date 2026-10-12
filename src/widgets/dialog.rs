//! The pieces dialogs and popovers share: their frame, a modal's header and
//! footer, the focus ring, and form fields with their label and help line.

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, Response, RichText, Stroke,
    StrokeKind, Ui, Vec2,
};

use super::{ButtonKind, button};
use crate::theme::{self, Icon};

/// Paints the 2pt focus ring outside a focused control.
pub fn focus_ring(ui: &Ui, response: &Response, radius: u8) {
    if response.has_focus() {
        paint_focus_ring(ui, response.rect, radius);
    }
}

/// Paints the 2pt focus ring outside `rect`, focused or not.
pub fn paint_focus_ring(ui: &Ui, rect: Rect, radius: u8) {
    ui.painter().rect_stroke(
        rect.expand(2.0),
        CornerRadius::same(radius + 2),
        Stroke::new(2.0, theme::palette(ui).accent),
        StrokeKind::Outside,
    );
}

/// The raised `surface` card of a modal, menu or popover: a `line-strong`
/// edge, `radius-md` corners and the popup shadow.
pub fn popover_frame(ui: &Ui) -> Frame {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_strong))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .shadow(ui.visuals().popup_shadow)
}

/// A modal's `bar-h` header: its title, `extra` (Settings puts its tabs
/// there), and a close button on the right. True when close was clicked.
pub fn modal_header(ui: &mut Ui, title: &str, extra: impl FnOnce(&mut Ui)) -> bool {
    let p = theme::palette(ui);
    let mut close = false;
    Frame::new()
        .inner_margin(Margin {
            left: theme::SPACE_5 as i8,
            right: theme::SPACE_3 as i8,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), theme::BAR_H),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.label(RichText::new(title).font(theme::ui_title()).color(p.ink));
                    ui.add_space(theme::SPACE_4);
                    extra(ui);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        close = button(ui, ButtonKind::Ghost, Some(Icon::Close), "", None)
                            .on_hover_text("Close (esc)")
                            .clicked();
                    });
                },
            );
        });
    close
}

/// What a modal's footer was asked for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FooterClick {
    None,
    Primary,
    Cancel,
}

/// A modal's footer: `note` on the left, Cancel and the `primary` button
/// (`⌘↩`) on the right.
pub fn modal_footer(ui: &mut Ui, note: &str, primary: &str) -> FooterClick {
    let p = theme::palette(ui);
    let mut click = FooterClick::None;
    Frame::new()
        .inner_margin(Margin::symmetric(
            theme::SPACE_5 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), theme::CONTROL_H),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    if button(ui, ButtonKind::Primary, None, primary, Some("⌘↩")).clicked() {
                        click = FooterClick::Primary;
                    }
                    if button(ui, ButtonKind::Secondary, None, "Cancel", None).clicked() {
                        click = FooterClick::Cancel;
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(note).font(theme::meta()).color(p.ink_muted),
                        )
                        .truncate(),
                    );
                },
            );
        });
    click
}

/// The `line` rule between a modal's header, body and footer.
pub fn rule(ui: &mut Ui) {
    let p = theme::palette(ui);
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, p.line);
}

/// A field's label, above its control.
pub fn field_label(ui: &mut Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(RichText::new(text).font(theme::ui_button()).color(p.ink));
    ui.add_space(theme::SPACE_2);
}

/// The `ui-small` line under a field.
pub fn help(ui: &mut Ui, text: &str, color: Color32) {
    ui.add(egui::Label::new(RichText::new(text).font(theme::ui_small()).color(color)).wrap());
}

/// A full-width mono text field.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    let p = theme::palette(ui);
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(RichText::new(hint).color(p.ink_faint))
            .font(FontId::monospace(12.0))
            .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
            .min_size(Vec2::new(0.0, theme::CONTROL_H))
            .desired_width(f32::INFINITY),
    )
}
