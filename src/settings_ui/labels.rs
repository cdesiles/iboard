//! Settings › Labels: label colours, renames and new labels.

use egui::{Align, CornerRadius, FontId, Frame, Layout, Stroke, Vec2};

use super::Dialog;
use crate::labels::{self};
use crate::theme::{self, Icon};
use crate::widgets::{self, ButtonKind, field_label, help};

/// Tallest the label list grows before it scrolls.
const LABEL_LIST_MAX_H: f32 = 240.0;

/// The row whose name is being edited, the name typed so far, and whether its
/// field takes focus on the next frame.
pub(super) struct Rename {
    index: usize,
    text: String,
    focus: bool,
}

/// What the rows asked for during a frame, applied once they are all drawn.
#[derive(Default)]
struct RowEdits {
    remove: Option<usize>,
    start_rename: Option<Rename>,
    commit_rename: bool,
}

/// A small mono `#rrggbb` field, edged in `danger` while the value is not a
/// colour.
fn hex_field(ui: &mut egui::Ui, hex: &mut String, hint: &str) -> egui::Response {
    let p = theme::palette(ui);
    let invalid = !hex.trim().is_empty() && labels::parse_hex(hex).is_none();
    let response = widgets::text_entry(
        ui,
        widgets::field_edit(ui, hex, hint, FontId::monospace(12.0))
            .char_limit(7)
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

/// A label's row: its chip, or the field renaming it, then its colour, a way
/// back to the preset and a trash to remove it.
fn label_row(
    ui: &mut egui::Ui,
    index: usize,
    name: &str,
    hex: &mut String,
    rename: Option<&mut Rename>,
    edits: &mut RowEdits,
) {
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        if let Some(rename) = rename {
            let field = widgets::text_entry(
                ui,
                egui::TextEdit::singleline(&mut rename.text)
                    .hint_text("name")
                    .font(FontId::monospace(12.0))
                    .margin(Vec2::new(theme::SPACE_2, 4.0))
                    .char_limit(labels::MAX_LABEL_CHARS)
                    .desired_width(140.0),
            );
            if std::mem::take(&mut rename.focus) {
                field.request_focus();
            }
            if field.lost_focus() {
                edits.commit_rename = true;
            }
        } else {
            let pill = widgets::label_pill(ui, name, labels::parse_hex(hex))
                .on_hover_text("Double-click to rename");
            if pill.double_clicked() {
                edits.start_rename = Some(Rename {
                    index,
                    text: name.to_string(),
                    focus: true,
                });
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
                edits.remove = Some(index);
            }
            if let Some(preset) = labels::preset_hex(name)
                && !hex.eq_ignore_ascii_case(preset)
                && widgets::button(ui, ButtonKind::Ghost, None, "Reset", None)
                    .on_hover_text(format!("Back to {preset}"))
                    .clicked()
            {
                *hex = preset.to_string();
            }
            hex_field(ui, hex, "no colour");
        });
    });
}

impl Dialog {
    /// Settings › Labels: each label's chip, its `#rrggbb` colour, a way back
    /// to the preset and a trash to remove it; then a row to add one.
    pub(super) fn labels_body(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        field_label(ui, "Label colours");
        let mut rows = std::mem::take(&mut self.label_rows);
        let mut edits = RowEdits::default();
        let rows_height = (rows.len() as f32 * 32.0).min(LABEL_LIST_MAX_H);
        if rows.is_empty() {
            help(ui, "No labels yet. Add one below.", p.ink_muted);
        } else {
            Frame::new()
                .stroke(Stroke::new(1.0, p.line_strong))
                .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                .show(ui, |ui| {
                    theme::scroll_area(
                        ui,
                        egui::ScrollArea::vertical()
                            .id_salt("label-rows")
                            .content_margin(theme::scroll_margin(theme::SPACE_4, theme::SPACE_2))
                            .max_height(rows_height + 2.0 * theme::SPACE_2)
                            .auto_shrink([false, true]),
                        true,
                        |ui| {
                            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                            for (index, (name, hex)) in rows.iter_mut().enumerate() {
                                let rename =
                                    self.rename.as_mut().filter(|rename| rename.index == index);
                                label_row(ui, index, name, hex, rename, &mut edits);
                            }
                        },
                    );
                });
        }
        self.label_rows = rows;
        if let Some(index) = edits.remove {
            self.label_rows.remove(index);
        }
        if let Some(rename) = edits.start_rename {
            self.rename = Some(rename);
        }
        if edits.commit_rename {
            self.commit_rename();
        }

        ui.add_space(theme::SPACE_5);
        field_label(ui, "Add a label");
        self.add_row(ui);
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

    /// The row adding a label: its name, its colour, the chip they make and
    /// Add.
    fn add_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            widgets::text_entry(
                ui,
                widgets::field_edit(ui, &mut self.new_label, "name", FontId::monospace(12.0))
                    .desired_width(140.0),
            );
            hex_field(ui, &mut self.new_hex, "#rrggbb");
            if let Some(name) = new_label_name(&self.new_label) {
                widgets::label_pill(ui, &name, labels::parse_hex(&self.new_hex));
            }
            if widgets::button(ui, ButtonKind::Secondary, Some(Icon::Plus), "Add", None).clicked() {
                self.label_error = self.add_label().err();
            }
        });
    }

    /// Adds the label typed in the add row, or recolours it when it exists,
    /// and empties the row. Fails, keeping the row, when the name is empty or
    /// the colour is not `#rrggbb`.
    fn add_label(&mut self) -> Result<(), String> {
        let Some(name) = new_label_name(&self.new_label) else {
            return Err("Name the label first.".to_string());
        };
        if labels::parse_hex(&self.new_hex).is_none() {
            return Err("The colour must be #rrggbb, for example #1a6e42.".to_string());
        }
        let hex = self.new_hex.trim().to_string();
        match self.label_rows.iter_mut().find(|(known, _)| *known == name) {
            Some(row) => row.1 = hex,
            None => {
                self.label_rows.push((name, hex));
                self.label_rows.sort_by(|a, b| a.0.cmp(&b.0));
            }
        }
        self.new_label.clear();
        self.new_hex.clear();
        Ok(())
    }

    /// Commits a double-click name edit: normalizes the typed name, updates
    /// the row and remembers the rename so Save can rewrite the issues.
    fn commit_rename(&mut self) {
        let Some(Rename { index, text, .. }) = self.rename.take() else {
            return;
        };
        let Some(old) = self.label_rows.get(index).map(|(name, _)| name.clone()) else {
            return;
        };
        let Some(new) = labels::normalize(&text) else {
            return;
        };
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
    use crate::settings_ui::Tab;
    use iboard::preferences::Preferences;

    #[test]
    fn a_new_label_is_named_like_the_labels_in_titles() {
        assert_eq!(new_label_name("Tech debt").as_deref(), Some("tech-debt"));
        assert_eq!(new_label_name(" [Bug] ").as_deref(), Some("bug"));
        assert_eq!(new_label_name("[ ]"), None);
    }

    #[test]
    fn adding_a_label_needs_a_name_and_a_colour_and_recolours_a_known_one() {
        let mut dialog = Dialog::new(&Preferences::default(), Tab::Labels);
        dialog.label_rows = vec![("ops".to_string(), String::new())];
        dialog.new_hex = "#1a6e42".to_string();
        assert_eq!(dialog.add_label(), Err("Name the label first.".to_string()));
        dialog.new_label = "[Tech debt]".to_string();
        dialog.new_hex = "green".to_string();
        assert!(dialog.add_label().is_err());
        assert_eq!(
            dialog.new_label, "[Tech debt]",
            "a refused label stays typed"
        );

        dialog.new_hex = " #1a6e42 ".to_string();
        assert_eq!(dialog.add_label(), Ok(()));
        dialog.new_label = "Ops".to_string();
        dialog.new_hex = "#000000".to_string();
        assert_eq!(dialog.add_label(), Ok(()));
        assert_eq!(
            dialog.label_rows,
            [
                ("ops".to_string(), "#000000".to_string()),
                ("tech-debt".to_string(), "#1a6e42".to_string()),
            ]
        );
        assert!(dialog.new_label.is_empty() && dialog.new_hex.is_empty());
    }
}
