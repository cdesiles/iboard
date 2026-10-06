//! The LabelPicker: a small popup that adds labels to one issue without the
//! Editor. It opens under its trigger (the card's Add label action or the
//! Reader's Add label button), lists the board's labels most used first, and
//! adds the typed one on `↩`. It stays open for the next label.

use std::path::{Path, PathBuf};

use egui::text::LayoutJob;
use egui::{
    Color32, CornerRadius, FontId, Frame, Key, Margin, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, TextFormat, Vec2,
};

use crate::labels::{self, Labels};
use crate::theme::{self, Icon};
use crate::widgets;

/// Width of the popup.
const WIDTH: f32 = 264.0;
/// How long an added label's pill keeps its `status-done` edge.
const FLASH_SECONDS: f64 = 1.2;
/// The list scrolls after five rows of 18pt pills, 4pt apart.
const LIST_MAX_H: f32 = 5.0 * 18.0 + 4.0 * 4.0;

/// An open picker, on one issue.
pub struct LabelPicker {
    pub path: PathBuf,
    pub text: String,
    pub note: Note,
    /// The label just added, and when, for the `status-done` flash.
    flash: Option<(String, f64)>,
    /// Put the cursor back in the field on the next frame.
    focus_field: bool,
    /// The popup's size last frame, to flip it above its trigger near the
    /// window's bottom edge.
    size: Vec2,
}

/// What the hint line says beyond the typed text.
pub enum Note {
    None,
    /// "Added ux to todo/ai-assist.md".
    Added {
        label: String,
        file: String,
    },
    /// A write failed: the hint turns `danger`.
    Failed(String),
}

impl LabelPicker {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            text: String::new(),
            note: Note::None,
            flash: None,
            focus_field: true,
            size: Vec2::new(WIDTH, 160.0),
        }
    }

    /// After a label was written: clear the field, keep the cursor in it,
    /// flash the label's pill and say what happened.
    pub fn added(&mut self, label: String, file: String, now: f64) {
        self.text.clear();
        self.flash = Some((label.clone(), now));
        self.note = Note::Added { label, file };
        self.focus_field = true;
    }

    pub fn failed(&mut self, message: String) {
        self.note = Note::Failed(message);
        self.focus_field = true;
    }
}

/// What the user asked for this frame.
pub enum Outcome {
    /// Add this label (already normalized).
    Add(String),
    /// A click outside: close, dropping the field's text.
    Close,
}

fn open_id() -> egui::Id {
    egui::Id::new("label_picker_open")
}

fn anchor_id(path: &Path) -> egui::Id {
    egui::Id::new(("label_picker_anchor", path))
}

/// Tells the cards which issue's picker is open, so its actions stay shown.
pub fn set_open(ctx: &egui::Context, path: Option<&Path>) {
    ctx.data_mut(|d| match path {
        Some(path) => {
            d.insert_temp(open_id(), path.to_path_buf());
        }
        None => d.remove::<PathBuf>(open_id()),
    });
}

/// The issue whose picker is open, if any.
pub fn open_path(ctx: &egui::Context) -> Option<PathBuf> {
    ctx.data(|d| d.get_temp::<PathBuf>(open_id()))
}

/// A trigger records where it is, every frame, so the popup follows it.
pub fn set_anchor(ctx: &egui::Context, path: &Path, rect: Rect) {
    ctx.data_mut(|d| d.insert_temp(anchor_id(path), rect));
}

fn anchor(ctx: &egui::Context, path: &Path) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Rect>(anchor_id(path)))
}

/// Draws the popup. `has` are the issue's labels, `board` every label on the
/// board, most used first.
pub fn show(
    ui: &mut egui::Ui,
    picker: &mut LabelPicker,
    has: &[String],
    board: &[String],
    colors: &Labels,
) -> Option<Outcome> {
    let ctx = ui.ctx().clone();
    let p = theme::palette(ui);
    let screen = ctx.content_rect();
    // Under the trigger, right-aligned with it; above it when it would leave
    // the window. Without a trigger on screen (it scrolled away), centred.
    let trigger = anchor(&ctx, &picker.path);
    let pos = match trigger {
        Some(rect) => {
            let below = rect.max.y + theme::SPACE_2;
            let y = if below + picker.size.y > screen.max.y - theme::SPACE_3 {
                (rect.min.y - theme::SPACE_2 - picker.size.y).max(screen.min.y)
            } else {
                below
            };
            let x = (rect.max.x - WIDTH)
                .min(screen.max.x - WIDTH - theme::SPACE_3)
                .max(screen.min.x + theme::SPACE_3);
            Pos2::new(x, y)
        }
        None => screen.center() - picker.size / 2.0,
    };
    let now = ctx.input(|i| i.time);
    let field_id = egui::Id::new("label_picker_field");
    let typed = labels::normalize(&picker.text);
    let mut outcome = None;

    let area = egui::Area::new(egui::Id::new("label_picker"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .constrain(false)
        .show(&ctx, |ui| {
            Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.line_strong))
                .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                .shadow(ui.visuals().popup_shadow)
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    let inner = WIDTH - 16.0 - 2.0;
                    ui.set_width(inner);
                    ui.spacing_mut().item_spacing.y = 6.0;

                    // 1. The field, focused on open.
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut picker.text)
                            .id(field_id)
                            .hint_text("Add a label")
                            .font(FontId::monospace(12.0))
                            .desired_width(inner)
                            .margin(Margin::symmetric(6, 4)),
                    );
                    if std::mem::take(&mut picker.focus_field) {
                        field.request_focus();
                    }
                    if field.changed() && !matches!(picker.note, Note::Failed(_)) {
                        picker.note = Note::None;
                    }
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if enter {
                        picker.focus_field = true;
                        if let Some(label) = &typed
                            && !has.contains(label)
                        {
                            outcome = Some(Outcome::Add(label.clone()));
                        }
                    }

                    // 2. The hint line: what `↩` will do.
                    hint_line(ui, picker, typed.as_deref(), has, board, inner);

                    // 3. The group head.
                    let shown: Vec<&String> = board
                        .iter()
                        .filter(|name| typed.as_deref().is_none_or(|text| name.contains(text)))
                        .collect();
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("ON THIS BOARD")
                                .font(theme::ui_label())
                                .color(p.ink_muted),
                        );
                        ui.label(
                            egui::RichText::new(shown.len().to_string())
                                .font(theme::meta())
                                .color(p.ink_muted),
                        );
                    });

                    // 4. The labels, wrapping, scrolling after five rows.
                    let mut pill_ids = Vec::new();
                    if board.is_empty() {
                        ui.label(
                            egui::RichText::new("No labels on this board yet. Type one above.")
                                .font(theme::ui_small())
                                .color(p.ink_muted),
                        );
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("label_picker_list")
                            .max_height(LIST_MAX_H)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::splat(4.0);
                                    for name in &shown {
                                        let flashing =
                                            picker.flash.as_ref().is_some_and(|(label, at)| {
                                                label == *name && now - at < FLASH_SECONDS
                                            });
                                        if has.contains(name) {
                                            on_pill(ui, name, flashing);
                                        } else {
                                            let pill =
                                                widgets::label_pill(ui, name, colors.color(name))
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .on_hover_text(format!("Add {name}"));
                                            focus_edge(ui, &pill);
                                            pill_ids.push(pill.id);
                                            if pill.clicked() {
                                                outcome = Some(Outcome::Add((*name).clone()));
                                                picker.focus_field = true;
                                            }
                                        }
                                    }
                                });
                            });
                    }
                    // `↓` leaves the field for the first label.
                    if field.has_focus()
                        && ui.input(|i| i.key_pressed(Key::ArrowDown))
                        && let Some(first) = pill_ids.first()
                    {
                        ui.memory_mut(|m| m.request_focus(*first));
                    }
                    // Typing on a focused pill goes back to the field.
                    let on_pill =
                        ui.memory(|m| m.focused().is_some_and(|id| pill_ids.contains(&id)));
                    if on_pill {
                        let text: String = ui.input(|i| {
                            i.events
                                .iter()
                                .filter_map(|event| match event {
                                    egui::Event::Text(text) => Some(text.as_str()),
                                    _ => None,
                                })
                                .collect()
                        });
                        if !text.is_empty() {
                            picker.text.push_str(&text);
                            picker.focus_field = true;
                        }
                    }

                    // 5. Key hints.
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                        widgets::hint(ui, "↩", "add");
                        widgets::hint(ui, "↓", "pick");
                        widgets::hint(ui, "esc", "close");
                    });
                });
        });
    picker.size = area.response.rect.size();
    if picker
        .flash
        .as_ref()
        .is_some_and(|(_, at)| now - at < FLASH_SECONDS)
    {
        ctx.request_repaint();
    }

    // A press outside the popup and its trigger closes it.
    let pressed_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer.interact_pos().is_some_and(|pos| {
                !area.response.rect.contains(pos) && !trigger.is_some_and(|rect| rect.contains(pos))
            })
    });
    if pressed_outside {
        return Some(Outcome::Close);
    }
    outcome
}

/// The hint line under the field, in `meta`, with the label in `ink`.
fn hint_line(
    ui: &mut egui::Ui,
    picker: &LabelPicker,
    typed: Option<&str>,
    has: &[String],
    board: &[String],
    width: f32,
) {
    let p = theme::palette(ui);
    let meta = theme::meta();
    let strong = FontId::new(11.0, theme::ui_strong().family);
    let plain = |color: Color32| TextFormat {
        font_id: meta.clone(),
        color,
        ..Default::default()
    };
    let bold = |color: Color32| TextFormat {
        font_id: strong.clone(),
        color,
        ..Default::default()
    };
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    match (&picker.note, typed) {
        (Note::Failed(message), _) => job.append(message, 0.0, plain(p.danger)),
        (_, Some(label)) if has.iter().any(|name| name == label) => {
            job.append(label, 0.0, bold(p.ink));
            job.append(" is already on this issue", 0.0, plain(p.ink_muted));
        }
        (_, Some(label)) => {
            job.append("↩ adds ", 0.0, plain(p.ink_muted));
            job.append(label, 0.0, bold(p.ink));
            if !board.iter().any(|name| name == label) {
                job.append(" · new on this board", 0.0, plain(p.ink_muted));
            }
        }
        (Note::Added { label, file }, None) => {
            job.append("Added ", 0.0, plain(p.ink_muted));
            job.append(label, 0.0, bold(p.ink));
            job.append(&format!(" to {file}"), 0.0, plain(p.ink_muted));
        }
        (Note::None, None) => {
            job.append(
                "Type a new label, or pick one below",
                0.0,
                plain(p.ink_muted),
            );
        }
    }
    ui.label(job);
}

/// A label the issue already has: a `status-done` check, a `line` edge on
/// `surface`, `ink-muted` text, not clickable. Just added, its edge flashes
/// `status-done`.
fn on_pill(ui: &mut egui::Ui, name: &str, flashing: bool) {
    let p = theme::palette(ui);
    let text = ui.painter().layout_no_wrap(
        name.to_string(),
        FontId::new(11.0, theme::ui_button().family),
        p.ink_muted,
    );
    let check = 10.0;
    let size = Vec2::new(6.0 + check + 4.0 + text.size().x + 7.0, 18.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS_PILL),
        p.surface,
        Stroke::new(1.0, if flashing { p.status_done } else { p.line }),
        StrokeKind::Inside,
    );
    let icon = Rect::from_min_size(
        Pos2::new(rect.min.x + 6.0, rect.center().y - check / 2.0),
        Vec2::splat(check),
    );
    theme::paint_icon(painter, Icon::Check, icon, p.status_done);
    painter.galley(
        Pos2::new(icon.max.x + 4.0, rect.center().y - text.size().y / 2.0),
        text,
        p.ink_muted,
    );
}

/// The 2px `focus` ring around a pill reached with the keyboard.
fn focus_edge(ui: &egui::Ui, response: &Response) {
    if response.has_focus() {
        let p = theme::palette(ui);
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            CornerRadius::same(theme::RADIUS_PILL),
            Stroke::new(2.0, p.accent),
            StrokeKind::Outside,
        );
    }
}
