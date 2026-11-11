//! Settings › Labels: label colours, renames and new labels.

use std::collections::{BTreeSet, HashMap};

use egui::{CornerRadius, FontId, Frame, Stroke, Vec2};

use super::Dialog;
use crate::domains::preferences::labels::{self, LabelColor, LabelStyle, Rgb};
use crate::kernel::labels::{Hue, MAX_LABEL_CHARS, normalize};
use crate::theme::{self, Icon};
use crate::widgets::{self, ButtonKind, field_label, help};

/// Tallest the label list grows before it scrolls.
const LABEL_LIST_MAX_H: f32 = 240.0;

/// Width of a row's `#rrggbb` field.
const HEX_W: f32 = 80.0;

/// So that "no colour" fits its field in mono.
const HEX_INSET: f32 = 6.0;

const STAND_OUT: &str = "Stand out";

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
    reset: Option<usize>,
    toggle_stand_out: Option<usize>,
}

/// One label, as its row draws it.
struct Row<'a> {
    index: usize,
    name: &'a str,
    hex: &'a mut String,
    /// A preset left with no colour.
    colourless: bool,
    style: LabelStyle,
    rename: Option<&'a mut Rename>,
}

/// A small mono `#rrggbb` field, edged in `danger` while the value is not a
/// colour.
fn hex_field(ui: &mut egui::Ui, hex: &mut String, hint: &str) -> egui::Response {
    let p = theme::palette(ui);
    let invalid = !hex.trim().is_empty() && labels::parse_hex(hex).is_none();
    let font = FontId::monospace(12.0);
    let response = widgets::text_entry(
        ui,
        widgets::field_edit(ui, hex, hint, font.clone())
            .hint_text(egui::RichText::new(hint).font(font).color(p.ink_faint))
            .margin(Vec2::new(HEX_INSET, 4.0))
            .char_limit(7)
            .desired_width(HEX_W),
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
    normalize(typed.trim().trim_matches(['[', ']']))
}

/// The hue a row's empty colour stands for, unless it was removed.
fn kept_preset(name: &str, colourless: &BTreeSet<String>) -> Option<Hue> {
    labels::preset_hue(name).filter(|_| !colourless.contains(name))
}

/// Labels with a colour, then those without, each by name.
pub(super) fn sort_rows(rows: &mut [(String, String)], colourless: &BTreeSet<String>) {
    rows.sort_by_cached_key(|(name, hex)| {
        let bare = hex.trim().is_empty() && kept_preset(name, colourless).is_none();
        (bare, name.clone())
    });
}

/// The colour `name`'s pill shows for `hex`: `preset` when empty, its last
/// valid one while `hex` is not a colour.
fn chip_colour(
    last: &mut HashMap<String, LabelColor>,
    preset: Option<Hue>,
    name: &str,
    hex: &str,
) -> Option<LabelColor> {
    let preset = preset.map(LabelColor::Preset);
    if hex.trim().is_empty() {
        last.remove(name);
        return preset;
    }
    let Some(colour) = labels::parse_hex(hex).map(LabelColor::Own) else {
        return last.get(name).copied().or(preset);
    };
    if last.get(name) != Some(&colour) {
        last.insert(name.to_string(), colour);
    }
    Some(colour)
}

/// A label's row: its pill, or the field renaming it, then on the right
/// whether it stands out, its colour, a way back to the preset and a trash to
/// remove it, drawn in the order `tab` walks them.
fn label_row(ui: &mut egui::Ui, row: Row, edits: &mut RowEdits) {
    let Row {
        index,
        name,
        hex,
        colourless,
        style,
        rename,
    } = row;
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        if let Some(rename) = rename {
            let field = widgets::text_entry(
                ui,
                widgets::field_edit(ui, &mut rename.text, "name", theme::ui_body())
                    .char_limit(MAX_LABEL_CHARS)
                    .desired_width(140.0),
            );
            if std::mem::take(&mut rename.focus) {
                field.request_focus();
            }
            if field.lost_focus() {
                edits.commit_rename = true;
            }
        } else {
            let pill = widgets::unfocusable_label_pill(ui, name, style)
                .on_hover_text("Double-click to rename");
            if pill.double_clicked() {
                edits.start_rename = Some(Rename {
                    index,
                    text: name.to_string(),
                    focus: true,
                });
            }
        }
        let gap = ui.spacing().item_spacing.x;
        let hue = labels::preset_hue(name);
        let back_to = hue.filter(|_| colourless || !hex.trim().is_empty());
        let reset_w = back_to.map_or(0.0, |_| {
            widgets::text_button_width(ui, ButtonKind::Ghost, "Reset") + gap
        });
        let stand_out_w = widgets::quiet_checkbox_width(ui, STAND_OUT) + gap;
        ui.add_space(
            (ui.available_width() - stand_out_w - HEX_W - gap - reset_w - widgets::CARD_ACTION)
                .max(0.0),
        );
        let mut stand_out = style.stand_out;
        if widgets::quiet_checkbox(ui, &mut stand_out, STAND_OUT).changed() {
            edits.toggle_stand_out = Some(index);
        }
        let hint = if hue.is_some() && !colourless {
            "preset"
        } else {
            "no colour"
        };
        hex_field(ui, hex, hint);
        if let Some(hue) = back_to
            && widgets::button(ui, ButtonKind::Ghost, None, "Reset", None)
                .on_hover_text(format!("Back to the preset {}", hue.name()))
                .clicked()
        {
            edits.reset = Some(index);
        }
        // Every label can go, the presets too.
        if widgets::icon_button(
            ui,
            Icon::Trash,
            egui::Id::unique(("label-remove", index)),
            "Remove this label",
            widgets::Tone::Danger,
        )
        .clicked()
        {
            edits.remove = Some(index);
        }
    });
}

impl Dialog {
    /// Settings › Labels: each label's pill, whether it stands out, its
    /// `#rrggbb` colour, a way back to the preset and a trash to remove it; then
    /// a row to add one.
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
                .stroke(Stroke::new(1.0, p.line_soft))
                .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                .show(ui, |ui| {
                    theme::scroll_area(
                        ui,
                        egui::ScrollArea::vertical()
                            .id_salt("label-rows")
                            .content_margin(theme::scroll_margin(theme::SPACE_4, theme::SPACE_2))
                            .max_height(rows_height + 2.0 * theme::SPACE_2)
                            .auto_shrink([false, true]),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                            for (index, (name, hex)) in rows.iter_mut().enumerate() {
                                let rename =
                                    self.rename.as_mut().filter(|rename| rename.index == index);
                                let preset = kept_preset(name, &self.colourless);
                                let color = chip_colour(&mut self.last_colours, preset, name, hex);
                                let row = Row {
                                    index,
                                    name,
                                    hex,
                                    colourless: self.colourless.contains(name),
                                    style: LabelStyle {
                                        color,
                                        stand_out: self.stand_out.contains(name),
                                    },
                                    rename,
                                };
                                label_row(ui, row, &mut edits);
                            }
                        },
                    );
                });
        }
        self.label_rows = rows;
        if let Some(index) = edits.toggle_stand_out
            && let Some((name, _)) = self.label_rows.get(index)
            && !self.stand_out.remove(name)
        {
            self.stand_out.insert(name.clone());
        }
        if let Some(index) = edits.reset
            && let Some((name, hex)) = self.label_rows.get_mut(index)
        {
            hex.clear();
            self.colourless.remove(name);
        }
        if edits.commit_rename {
            if edits.remove.is_some_and(|index| {
                self.rename
                    .as_ref()
                    .is_some_and(|rename| rename.index == index)
            }) {
                self.rename = None;
            }
            self.commit_rename(p);
        }
        if let Some(rename) = edits.start_rename {
            self.rename = Some(rename);
        }
        if let Some(index) = edits.remove {
            let (name, _) = self.label_rows.remove(index);
            self.stand_out.remove(&name);
            self.colourless.remove(&name);
            match self.rename.as_mut() {
                Some(rename) if rename.index == index => self.rename = None,
                Some(rename) if rename.index > index => rename.index -= 1,
                _ => {}
            }
        }

        ui.add_space(theme::SPACE_5);
        field_label(ui, "Add a label");
        self.add_row(ui);
        ui.add_space(theme::SPACE_2);
        match &self.label_error {
            Some(error) => help(ui, error, p.danger),
            None => help(
                ui,
                "A label is the [word] at the start of an issue's title. Only changes to iboard's presets are written to [labels]. Stand out tints the pill; keep it for what must catch the eye.",
                p.ink_muted,
            ),
        }
        if let Some(error) = &self.save_error {
            ui.add_space(theme::SPACE_5);
            help(ui, error, p.danger);
        }
    }

    /// The row adding a label: its name, its colour, the pill they make and
    /// Add.
    fn add_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            widgets::text_entry(
                ui,
                widgets::field_edit(ui, &mut self.new_label, "name", theme::ui_body())
                    .desired_width(140.0),
            );
            hex_field(ui, &mut self.new_hex, "#rrggbb");
            if let Some(name) = new_label_name(&self.new_label) {
                let style = LabelStyle {
                    color: labels::parse_hex(&self.new_hex).map(LabelColor::Own),
                    stand_out: self.stand_out.contains(&name),
                };
                widgets::unfocusable_label_pill(ui, &name, style);
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
        self.colourless.remove(&name);
        match self.label_rows.iter_mut().find(|(known, _)| *known == name) {
            Some(row) => row.1 = hex,
            None => self.label_rows.push((name, hex)),
        }
        sort_rows(&mut self.label_rows, &self.colourless);
        self.new_label.clear();
        self.new_hex.clear();
        Ok(())
    }

    /// Commits a double-click name edit: normalizes the typed name, updates
    /// the row and remembers the rename so Save can rewrite the issues. Its
    /// colour and whether it stands out move along; a preset's colour, which
    /// only its name could follow, moves as this theme's value.
    fn commit_rename(&mut self, p: &theme::Palette) {
        let Some(Rename { index, text, .. }) = self.rename.take() else {
            return;
        };
        let Some(old) = self.label_rows.get(index).map(|(name, _)| name.clone()) else {
            return;
        };
        let Some(new) = normalize(&text) else {
            return;
        };
        if new == old {
            return;
        }
        let kept = kept_preset(&old, &self.colourless);
        self.colourless.remove(&old);
        if self.stand_out.remove(&old) {
            self.stand_out.insert(new.clone());
        }
        if let Some(row) = self.label_rows.get_mut(index) {
            row.0 = new.clone();
            if let Some(hue) = kept.filter(|_| labels::parse_hex(&row.1).is_none()) {
                let colour = theme::label_color(LabelColor::Preset(hue), p);
                row.1 = labels::to_hex(Rgb(colour.r(), colour.g(), colour.b()));
            }
        }
        if let Some(colour) = self.last_colours.remove(&old) {
            self.last_colours.insert(new.clone(), colour);
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
    use crate::settings_ui::{Catalog, Tab};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use iboard::preferences::Preferences;
    use iboard::settings::UserConfig;
    use std::path::Path;

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

    #[test]
    fn coloured_labels_sort_before_the_others() {
        let dialog = Dialog::new(&Preferences::default(), Tab::Labels);
        let mut dialog = Dialog {
            label_rows: vec![("zeta".to_string(), "#000000".to_string())],
            ..dialog
        }
        .with_used_labels(["alpha".to_string()]);
        let names = |dialog: &Dialog| {
            dialog
                .label_rows
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&dialog), ["zeta", "alpha"]);

        dialog.new_label = "beta".to_string();
        dialog.new_hex = "#111111".to_string();
        assert_eq!(dialog.add_label(), Ok(()));
        assert_eq!(names(&dialog), ["beta", "zeta", "alpha"]);
    }

    #[test]
    fn an_invalid_hex_keeps_the_last_valid_colour() {
        let mut last = HashMap::new();
        let black = Some(LabelColor::Own(Rgb(0, 0, 0)));
        let red = Some(Hue::Red);
        let mut colour = |preset, name, hex| chip_colour(&mut last, preset, name, hex);
        assert_eq!(colour(None, "ops", "#00"), None, "none to keep");
        assert_eq!(
            colour(red, "bug", "#00"),
            Some(LabelColor::Preset(Hue::Red)),
            "the preset's"
        );
        assert_eq!(colour(red, "bug", "#000000"), black);
        assert_eq!(colour(red, "bug", "#00000"), black);
        assert_eq!(colour(None, "ops", "#00000"), None, "per label");
        assert_eq!(colour(None, "bug", ""), None, "no colour");
    }

    fn opened(config: &str, used: &[&str]) -> (Preferences, Dialog) {
        let mut config = UserConfig::parse(config, Path::new("iboard.toml"));
        let prefs = Preferences::load(&mut config);
        let dialog = Dialog::new(&prefs, Tab::Labels)
            .with_used_labels(used.iter().map(|name| name.to_string()));
        (prefs, dialog)
    }

    #[test]
    fn saving_untouched_rows_writes_nothing() {
        let config = "[labels]\nbug = \"#ff0000\"\nchore = \"\"\nspike = \"#123456\"\n\
                      [label_style]\nstand_out = [\"spike\", \"gone\"]\n";
        for used in [&[][..], &["chore", "ops"][..]] {
            let (prefs, dialog) = opened(config, used);
            assert_eq!(dialog.labels(), prefs.labels);
            let saved = Preferences {
                labels: dialog.labels(),
                ..prefs.clone()
            };
            assert!(saved.sections_since(&prefs).unwrap().is_empty());
        }
    }

    #[test]
    fn a_removed_preset_stays_colourless_until_reset() {
        let (prefs, mut dialog) = opened("[labels]\nbug = \"\"\n", &["bug"]);
        assert_eq!(dialog.labels().style("bug").color, None);
        let row = dialog.label_rows.iter().position(|(name, _)| name == "bug");
        assert_eq!(row, Some(dialog.label_rows.len() - 1), "sorted last");
        assert_eq!(dialog.labels(), prefs.labels);

        dialog.colourless.remove("bug");
        assert_eq!(
            dialog.labels().style("bug").color,
            Some(LabelColor::Preset(Hue::Red))
        );
    }

    #[test]
    fn renaming_a_preset_moves_its_colour_and_stand_out_along() {
        let (_, mut dialog) = opened("", &[]);
        let index = dialog
            .label_rows
            .iter()
            .position(|(name, _)| name == "bug")
            .unwrap();
        dialog.rename = Some(Rename {
            index,
            text: "Defect".to_string(),
            focus: false,
        });
        dialog.commit_rename(&theme::LIGHT);
        let labels = dialog.labels();
        assert_eq!(
            labels.style("defect"),
            LabelStyle {
                color: Some(LabelColor::Own(Rgb(0xbc, 0x32, 0x28))),
                stand_out: true,
            }
        );
        assert_eq!(labels.style("bug"), LabelStyle::default());
    }

    fn labels_harness(rows: &[(&str, &str)]) -> Harness<'static, (bool, Dialog, Catalog)> {
        let mut dialog = Dialog::new(&Preferences::default(), Tab::Labels);
        dialog.label_rows = rows
            .iter()
            .map(|(name, hex)| (name.to_string(), hex.to_string()))
            .collect();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1280.0, 800.0))
            .build_ui_state(
                |ui, (themed, dialog, catalog): &mut (bool, Dialog, Catalog)| {
                    if !*themed {
                        theme::install(ui.ctx());
                        *themed = true;
                        return;
                    }
                    dialog.show(ui, catalog, "Saved to ~/x.toml");
                },
                (false, dialog, Catalog::default()),
            );
        harness.run_steps(2);
        harness
    }

    #[test]
    fn tab_walks_a_row_from_its_colour_to_its_trash() {
        let mut harness = labels_harness(&[("bug", "#000000"), ("ops", "#111111")]);
        let row_y = harness.get_by_label("bug").rect().center().y;
        let mut walked = Vec::new();
        for _ in 0..40 {
            let focused = harness.ctx.memory(|m| m.focused());
            if let Some(rect) = focused.and_then(|id| harness.ctx.read_response(id)) {
                let rect = rect.rect;
                if rect.y_range().contains(row_y) && !walked.contains(&rect) {
                    walked.push(rect);
                }
            }
            harness.key_press(egui::Key::Tab);
            harness.run_steps(2);
        }
        assert_eq!(
            walked.len(),
            4,
            "Stand out, hex, Reset, trash, never the pill"
        );
        assert!(walked.windows(2).all(|pair| pair[0].max.x < pair[1].min.x));
        let trashes = harness
            .get_all_by_label("Remove this label")
            .map(|node| node.rect().max.x)
            .collect::<Vec<_>>();
        assert_eq!(trashes[0], trashes[1], "with or without a Reset");
        assert_eq!(trashes[0], walked[3].max.x);
    }

    fn state<'a>(harness: &'a Harness<'static, (bool, Dialog, Catalog)>) -> &'a Dialog {
        &harness.state().1
    }

    #[test]
    fn stand_out_ticks_for_a_row_and_is_saved() {
        let mut harness = labels_harness(&[("bug", ""), ("ops", "#111111")]);
        assert!(state(&harness).labels().style("bug").stand_out);
        let boxes: Vec<_> = harness.get_all_by_label(STAND_OUT).collect();
        assert_eq!(boxes.len(), 2);
        boxes[1].click();
        boxes[0].click();
        harness.run_steps(2);
        let labels = state(&harness).labels();
        assert!(labels.style("ops").stand_out, "ticked");
        assert!(!labels.style("bug").stand_out, "unticked");
        assert!(state(&harness).is_dirty());
    }

    #[test]
    fn a_preset_shows_preset_until_its_colour_changes_and_reset_clears_it() {
        let mut harness = labels_harness(&[("bug", ""), ("ops", "")]);
        assert!(harness.query_by_label("Reset").is_none(), "nothing changed");
        harness.state_mut().1.label_rows[0].1 = "#ff0000".to_string();
        harness.run_steps(2);
        assert_eq!(
            state(&harness).labels().style("bug").color,
            Some(LabelColor::Own(Rgb(255, 0, 0)))
        );
        harness.get_by_label("Reset").click();
        harness.run_steps(2);
        let dialog = state(&harness);
        assert_eq!(dialog.label_rows[0].1, "", "back to following the palette");
        assert_eq!(
            dialog.labels().style("bug").color,
            Some(LabelColor::Preset(Hue::Red))
        );
    }

    #[test]
    fn removing_a_label_drops_its_colour_and_stand_out() {
        let mut harness = labels_harness(&[("bug", "")]);
        harness.get_by_label("Remove this label").click();
        harness.run_steps(2);
        let labels = state(&harness).labels();
        assert_eq!(labels.style("bug"), LabelStyle::default());
        assert_eq!(labels::table(&labels.entries())["bug"], "");
    }

    #[test]
    fn removing_a_row_while_another_is_renamed_renames_the_right_one() {
        let rows = [
            ("alpha", "#000000"),
            ("beta", "#111111"),
            ("gamma", "#222222"),
        ];
        let mut harness = labels_harness(&rows);
        harness.state_mut().1.rename = Some(Rename {
            index: 1,
            text: "renamed".to_string(),
            focus: true,
        });
        harness.run_steps(2);
        harness
            .get_all_by_label("Remove this label")
            .next()
            .unwrap()
            .click();
        harness.run_steps(2);
        let names: Vec<_> = state(&harness)
            .label_rows
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names, ["renamed", "gamma"]);
        assert_eq!(
            state(&harness).renames,
            [("beta".to_string(), "renamed".to_string())]
        );
    }
}
