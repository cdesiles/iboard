//! Settings › Labels: label colours, renames and new labels.

use egui::{Align, CornerRadius, FontId, Frame, Layout, Margin, RichText, Stroke, Vec2};

use super::Dialog;
use crate::labels::{self};
use crate::theme::{self, Icon};
use crate::widgets::{self, ButtonKind, field_label, help};

/// Tallest the label list grows before it scrolls.
const LABEL_LIST_MAX_H: f32 = 240.0;

/// A small mono `#rrggbb` field, edged in `danger` while the value is not a
/// colour.
fn hex_field(ui: &mut egui::Ui, hex: &mut String, hint: &str) -> egui::Response {
    let p = theme::palette(ui);
    let invalid = !hex.trim().is_empty() && labels::parse_hex(hex).is_none();
    let response = widgets::text_entry(
        ui,
        egui::TextEdit::singleline(hex)
            .hint_text(RichText::new(hint).color(p.ink_faint))
            .font(FontId::monospace(12.0))
            .char_limit(7)
            .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
            .min_size(Vec2::new(0.0, theme::CONTROL_H))
            .desired_width(80.0),
    );
    if invalid {
        ui.painter().rect_stroke(
            response.rect,
            CornerRadius::same(theme::RADIUS_SM),
            Stroke::new(1.0, p.danger),
            egui::StrokeKind::Inside,
        );
    }
    response
}

/// The label a name typed in Settings › Labels stands for, the same way a
/// title's `[Tech debt]` gives `tech-debt`.
fn new_label_name(typed: &str) -> Option<String> {
    labels::normalize(typed.trim().trim_matches(['[', ']']))
}

impl Dialog {
    /// Settings › Labels: each label's chip, its `#rrggbb` colour, a way back
    /// to the preset and a trash to remove it; then a row to add one.
    pub(super) fn labels_body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        field_label(ui, "Label colours");
        let mut remove = None;
        let mut rows = std::mem::take(&mut self.label_rows);
        let renaming = self.renaming;
        let mut start_rename: Option<(usize, String)> = None;
        let mut commit_rename = false;
        let rows_height = (rows.len() as f32 * 32.0).min(LABEL_LIST_MAX_H);
        if rows.is_empty() {
            help(ui, "No labels yet. Add one below.", p.ink_muted);
        } else {
            Frame::new()
                .stroke(Stroke::new(1.0, p.line_strong))
                .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                .inner_margin(Margin::symmetric(
                    theme::SPACE_4 as i8,
                    theme::SPACE_2 as i8,
                ))
                .show(ui, |ui| {
                    theme::scroll_area(
                        ui,
                        egui::ScrollArea::vertical()
                            .id_salt("label-rows")
                            .max_height(rows_height)
                            .auto_shrink([false, true]),
                        true,
                        |ui| {
                            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                            for (index, (name, hex)) in rows.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.set_min_height(28.0);
                                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                                    if renaming == Some(index) {
                                        let field = widgets::text_entry(
                                            ui,
                                            egui::TextEdit::singleline(&mut self.rename_text)
                                                .hint_text("name")
                                                .font(FontId::monospace(12.0))
                                                .margin(Margin::symmetric(theme::SPACE_2 as i8, 4))
                                                .char_limit(labels::MAX_LABEL_CHARS)
                                                .desired_width(140.0),
                                        );
                                        if std::mem::take(&mut self.rename_focus) {
                                            field.request_focus();
                                        }
                                        if field.lost_focus() {
                                            commit_rename = true;
                                        }
                                    } else {
                                        let pill = widgets::label_pill(
                                            ui,
                                            name.as_str(),
                                            labels::parse_hex(hex),
                                        )
                                        .on_hover_text("Double-click to rename");
                                        if pill.double_clicked() {
                                            start_rename = Some((index, name.clone()));
                                        }
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        // Every label can go, the presets too.
                                        if widgets::icon_button(
                                            ui,
                                            Icon::Trash,
                                            egui::Id::new(("label-remove", index)),
                                            "Remove this label",
                                            widgets::Tone::Danger,
                                        )
                                        .clicked()
                                        {
                                            remove = Some(index);
                                        }
                                        if let Some(preset) = labels::preset_hex(name)
                                            && !hex.eq_ignore_ascii_case(preset)
                                            && widgets::button(
                                                ui,
                                                ButtonKind::Ghost,
                                                None,
                                                "Reset",
                                                None,
                                            )
                                            .on_hover_text(format!("Back to {preset}"))
                                            .clicked()
                                        {
                                            *hex = preset.to_string();
                                        }
                                        hex_field(ui, hex, "no colour");
                                    });
                                });
                            }
                        },
                    );
                });
        }
        self.label_rows = rows;
        if let Some(index) = remove {
            self.label_rows.remove(index);
        }
        if let Some((index, text)) = start_rename {
            self.renaming = Some(index);
            self.rename_text = text;
            self.rename_focus = true;
        }
        if commit_rename {
            self.commit_rename();
        }

        ui.add_space(theme::SPACE_5);
        field_label(ui, "Add a label");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            widgets::text_entry(
                ui,
                egui::TextEdit::singleline(&mut self.new_label)
                    .hint_text(RichText::new("name").color(p.ink_faint))
                    .font(FontId::monospace(12.0))
                    .margin(Margin::symmetric(theme::SPACE_3 as i8, 4))
                    .min_size(Vec2::new(0.0, theme::CONTROL_H))
                    .desired_width(140.0),
            );
            hex_field(ui, &mut self.new_hex, "#rrggbb");
            let name = new_label_name(&self.new_label).unwrap_or_default();
            if !name.is_empty() {
                widgets::label_pill(ui, &name, labels::parse_hex(&self.new_hex));
            }
            if widgets::button(ui, ButtonKind::Secondary, Some(Icon::Plus), "Add", None).clicked() {
                self.label_error = if name.is_empty() {
                    Some("Name the label first.".to_string())
                } else if labels::parse_hex(&self.new_hex).is_none() {
                    Some("The colour must be #rrggbb, for example #1a6e42.".to_string())
                } else {
                    match self.label_rows.iter_mut().find(|(known, _)| *known == name) {
                        Some(row) => row.1 = self.new_hex.trim().to_string(),
                        None => {
                            self.label_rows
                                .push((name, self.new_hex.trim().to_string()));
                            self.label_rows.sort_by(|a, b| a.0.cmp(&b.0));
                        }
                    }
                    self.new_label.clear();
                    self.new_hex.clear();
                    None
                };
            }
        });
        ui.add_space(theme::SPACE_2);
        match &self.label_error {
            Some(error) => help(ui, error, p.danger),
            None => help(
                ui,
                "A label is the [word] at the start of an issue's title. Only changes to iboard's presets are written to [labels].",
                p.ink_muted,
            ),
        }
        if let Some(error) = &self.save_error {
            ui.add_space(theme::SPACE_5);
            help(ui, error, p.danger);
        }
    }

    /// Commits a double-click name edit: normalizes the typed name, updates
    /// the row and remembers the rename so Save can rewrite the issues.
    fn commit_rename(&mut self) {
        let Some(index) = self.renaming.take() else {
            return;
        };
        let Some(old) = self.label_rows.get(index).map(|(name, _)| name.clone()) else {
            self.rename_text.clear();
            return;
        };
        let Some(new) = labels::normalize(&self.rename_text) else {
            self.rename_text.clear();
            return;
        };
        self.rename_text.clear();
        if new == old {
            return;
        }
        if let Some(row) = self.label_rows.get_mut(index) {
            row.0 = new.clone();
        }
        if let Some(rename) = self.renames.iter_mut().find(|(from, _)| *from == old) {
            rename.1 = new;
        } else {
            self.renames.push((old, new));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_label_is_named_like_the_labels_in_titles() {
        assert_eq!(new_label_name("Tech debt").as_deref(), Some("tech-debt"));
        assert_eq!(new_label_name(" [Bug] ").as_deref(), Some("bug"));
        assert_eq!(new_label_name("[ ]"), None);
    }
}
