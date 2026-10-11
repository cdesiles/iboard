//! Labels on an issue: the LabelPicker, and adding, removing and renaming them.

use super::*;

impl IboardApp {
    /// Opens the LabelPicker on `path`, or closes it when it is already open
    /// there (the trigger again).
    pub(super) fn toggle_label_picker(&mut self, path: PathBuf) {
        if self
            .label_picker
            .as_ref()
            .is_some_and(|picker| picker.path == path)
        {
            self.label_picker = None;
        } else {
            self.label_picker = Some(LabelPicker::new(path));
        }
    }

    /// Draws the open LabelPicker and writes what it adds. It closes when its
    /// issue is gone or no longer on screen (the Editor edits labels in the
    /// title instead).
    pub(super) fn label_picker_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let on_screen = self.label_picker.as_ref().is_some_and(|picker| {
            self.board.find(&picker.path).is_some()
                && match &self.screen {
                    Screen::Board => true,
                    Screen::Detail(path) => {
                        *path == picker.path && self.detail_mode == DetailMode::Preview
                    }
                }
        });
        if !on_screen {
            self.label_picker = None;
        }
        label_picker::set_open(
            &ctx,
            self.label_picker
                .as_ref()
                .map(|picker| picker.path.as_path()),
        );
        let Some(picker) = self.label_picker.as_mut() else {
            return;
        };
        let has = self
            .board
            .find(&picker.path)
            .map(|issue| issue.labels.clone())
            .unwrap_or_default();
        let board = filter::top_labels(
            self.board.columns.iter().flat_map(|column| &column.issues),
            usize::MAX,
        );
        match label_picker::show(ui, picker, &has, &board, &self.labels) {
            Some(label_picker::Outcome::Close) => self.label_picker = None,
            Some(label_picker::Outcome::Add(label)) => {
                self.add_label(&label, ctx.input(|i| i.time));
            }
            Some(label_picker::Outcome::Remove(label)) => {
                self.remove_label(&label, ctx.input(|i| i.time));
            }
            None => {}
        }
    }

    /// Writes `label` into the picker's issue, then reloads so the pill shows
    /// on the card and in the Reader.
    pub(super) fn add_label(&mut self, label: &str, now: f64) {
        let Some(path) = self.label_picker.as_ref().map(|picker| picker.path.clone()) else {
            return;
        };
        let shown = path
            .strip_prefix(&self.board.root)
            .unwrap_or(&path)
            .display()
            .to_string();
        match labels::add_label_to_file(&path, label) {
            Ok(_) => {
                self.refresh();
                if let Some(picker) = self.label_picker.as_mut() {
                    picker.added(label.to_string(), shown, now);
                }
            }
            Err(message) => {
                if let Some(picker) = self.label_picker.as_mut() {
                    picker.failed(message);
                }
            }
        }
    }

    /// Takes `label` off the picker's issue, then reloads so the pill goes
    /// from the card and the Reader.
    pub(super) fn remove_label(&mut self, label: &str, now: f64) {
        let Some(path) = self.label_picker.as_ref().map(|picker| picker.path.clone()) else {
            return;
        };
        let shown = path
            .strip_prefix(&self.board.root)
            .unwrap_or(&path)
            .display()
            .to_string();
        match labels::remove_label_from_file(&path, label) {
            Ok(_) => {
                self.refresh();
                if let Some(picker) = self.label_picker.as_mut() {
                    picker.removed(label.to_string(), shown, now);
                }
            }
            Err(message) => {
                if let Some(picker) = self.label_picker.as_mut() {
                    picker.failed(message);
                }
            }
        }
    }

    /// Rewrites every issue that carries a renamed label, in the order the
    /// renames were made so chained renames resolve.
    pub(super) fn rename_labels(&mut self, renames: &[(String, String)]) {
        if renames.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = self
            .board
            .columns
            .iter()
            .flat_map(|column| column.issues.iter().map(|issue| issue.path.clone()))
            .collect();
        let mut changed = false;
        for path in paths {
            for (from, to) in renames {
                match labels::rename_label_to_file(&path, from, to) {
                    Ok(did) => changed |= did,
                    Err(message) => {
                        self.error = Some(message);
                        return;
                    }
                }
            }
        }
        if changed {
            self.refresh();
        }
    }
}

/// The issue whose LabelPicker is open, for its card.
pub(super) fn label_picker_open(ctx: &egui::Context) -> Option<PathBuf> {
    label_picker::open_path(ctx)
}

/// Where a LabelPicker trigger sits, so the popup opens under it.
pub(super) fn set_label_anchor(ctx: &egui::Context, path: &Path, rect: Rect) {
    label_picker::set_anchor(ctx, path, rect);
}
