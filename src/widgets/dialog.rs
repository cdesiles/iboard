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

/// The soft 3pt `focus-halo` around a focused text field, outside its one
/// `focus` border: text entry never shows a second ring.
pub fn focus_halo(ui: &Ui, response: &Response) {
    if response.has_focus() {
        paint_focus_halo(ui, response.rect, theme::RADIUS_SM);
    }
}

/// Adds a framed text field, with its halo while focused.
pub fn text_entry(ui: &mut Ui, edit: egui::TextEdit<'_>) -> Response {
    let response = ui.add(edit);
    focus_halo(ui, &response);
    response
}

/// Paints the `focus-halo` outside `rect`, focused or not.
pub fn paint_focus_halo(ui: &Ui, rect: Rect, radius: u8) {
    ui.painter().rect_stroke(
        rect.expand(0.5),
        CornerRadius::same(radius + 1),
        Stroke::new(3.0, theme::palette(ui).focus_halo),
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

/// A modal's frame: as [`popover_frame`], under `shadow-modal`.
pub fn modal_frame(ui: &Ui) -> Frame {
    let p = theme::palette(ui);
    popover_frame(ui).shadow(egui::Shadow {
        offset: [0, 12],
        blur: 40,
        spread: 0,
        color: p.modal_shadow,
    })
}

/// What a [`confirm`] asks: its question, the subject (a title and a path),
/// what follows, and the verb on its danger button.
pub struct Ask<'a> {
    pub id: &'a str,
    pub question: &'a str,
    pub title: &'a str,
    pub detail: &'a str,
    pub consequence: &'a str,
    pub confirm: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Pending,
    Confirmed,
    Cancelled,
}

/// ConfirmDialog: a Modal S over the scrim that asks before what cannot be
/// undone. Its danger button starts focused; Cancel or the scrim cancels.
/// The caller owns `↩` and `esc`.
pub fn confirm(ui: &Ui, ask: &Ask) -> Answer {
    let p = theme::palette(ui);
    let pad = 20.0;
    let mut answer = Answer::Pending;
    let frame = modal_frame(ui).inner_margin(Margin {
        left: pad as i8,
        right: pad as i8,
        top: pad as i8,
        bottom: theme::SPACE_5 as i8,
    });
    let modal = egui::Modal::new(egui::Id::new(ask.id))
        .frame(frame)
        .backdrop_color(p.scrim)
        .show(ui.ctx(), |ui| {
            let inner = theme::MODAL_S - 2.0 * pad;
            ui.set_width(inner);
            ui.style_mut().interaction.selectable_labels = false;
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            ui.label(
                RichText::new(ask.question)
                    .font(theme::ui_title())
                    .color(p.ink),
            );
            ui.add_space(theme::SPACE_2);
            Frame::new()
                .fill(p.surface_sunken)
                .corner_radius(CornerRadius::same(theme::RADIUS_SM))
                .inner_margin(Margin::symmetric(10, theme::SPACE_3 as i8))
                .show(ui, |ui| {
                    ui.set_width(inner - 20.0);
                    ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                    let title = RichText::new(ask.title)
                        .font(theme::ui_strong())
                        .color(p.ink);
                    ui.add(egui::Label::new(title).truncate());
                    let detail = RichText::new(ask.detail)
                        .font(theme::meta())
                        .color(p.ink_muted);
                    ui.add(egui::Label::new(detail).truncate());
                });
            ui.label(
                RichText::new(ask.consequence)
                    .font(theme::ui_small())
                    .color(p.ink_muted),
            );
            ui.add_space(theme::SPACE_3);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                // The danger button starts focused: `Del` then `↩` is the fast
                // path, and the dialog itself is the safety step.
                let confirm = button(
                    ui,
                    ButtonKind::Danger,
                    Some(Icon::Trash),
                    ask.confirm,
                    Some("↩"),
                );
                paint_focus_ring(ui, confirm.rect, theme::RADIUS_SM);
                if confirm.clicked() {
                    answer = Answer::Confirmed;
                }
                if button(ui, ButtonKind::Secondary, None, "Cancel", Some("esc")).clicked() {
                    answer = Answer::Cancelled;
                }
            });
        });
    if answer == Answer::Pending && modal.should_close() {
        answer = Answer::Cancelled;
    }
    answer
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
    text_entry(
        ui,
        egui::TextEdit::singleline(text)
            .hint_text(RichText::new(hint).color(p.ink_faint))
            .font(FontId::monospace(12.0))
            .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
            .min_size(Vec2::new(0.0, theme::CONTROL_H))
            .desired_width(f32::INFINITY),
    )
}

/// A segmented control on a `surface-sunken` track: the current option is
/// raised on `surface`. Returns the option clicked this frame.
pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, current: T, options: &[(T, &str)]) -> Option<T> {
    let p = theme::palette(ui);
    let mut picked = None;
    Frame::new()
        .fill(p.surface_sunken)
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for &(option, name) in options {
                    let active = option == current;
                    let text = RichText::new(name)
                        .font(theme::ui_button())
                        .color(if active { p.ink } else { p.ink_muted });
                    let edge = if active {
                        p.line_strong
                    } else {
                        Color32::TRANSPARENT
                    };
                    let button = egui::Button::new(text)
                        .fill(if active {
                            p.surface
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::new(1.0, edge))
                        .corner_radius(CornerRadius::same(theme::RADIUS_SM - 1))
                        .min_size(Vec2::new(0.0, theme::CONTROL_H - 4.0));
                    if ui.add(button).clicked() {
                        picked = Some(option);
                    }
                }
            });
        });
    picked
}

/// A 1pt dashed edge just inside `rect`, as on wells and drop zones.
pub fn dashed_rect(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(0.5);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(egui::Shape::dashed_line(
        &corners,
        Stroke::new(1.0, color),
        4.0,
        3.0,
    ));
}
