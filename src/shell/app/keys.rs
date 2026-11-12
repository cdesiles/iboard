//! The keys of the screen on show: `esc`, the shortcuts and the Diff's own
//! keys. The [`keymap`] says what they ask for; this applies it.

use super::reader::Stride;
use super::{DetailMode, IboardApp, Key, LeaveChoice, PathBuf, Screen};

mod keymap;

use keymap::{Command, Escape, KeyContext, Ladder, Place, escape_step, keymap};

impl IboardApp {
    /// The keys of the screen on show, while no dialog owns the keyboard:
    /// `esc`, a paste, the Diff's own keys, the shortcuts, the swipe back and
    /// the double-click that edits.
    pub(super) fn screen_keys(&mut self, ctx: &egui::Context) {
        self.project_trigger_keys(ctx);
        self.escape_keys(ctx);
        self.paste_images(ctx);
        if matches!(self.session.screen, Screen::Diff) {
            self.diff_keys(ctx);
        }
        let cx = self.key_context(ctx);
        let mut commands = ctx.input(|i| keymap(i, &cx));
        if !matches!(cx.place, Place::Board(_)) && self.swiped_back(ctx) {
            commands.push(Command::Back);
        }
        for command in commands {
            self.run(command);
        }
        // Saved some other way while the prompt was up: carry on leaving.
        if let Some(to) = self.session.leave_prompt.clone()
            && !self.has_unsaved_edits()
        {
            self.go(to);
        }
        // A double-click on the sheet, both its clicks, edits at that line.
        if cx.editable
            && !self.writing_note()
            && !self.reader.stray_double_click(ctx)
            && let Place::Issue(path, DetailMode::Preview) = cx.place
            && let Some(pos) = ctx.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
                    .then(|| i.pointer.interact_pos())
                    .flatten()
            })
            && self.on_the_sheet(ctx, pos)
        {
            self.edit_at_line(&path, self.reader.line_at(pos.y));
        }
    }

    /// The Diff screen's own keys: `⌘F` finds, and the moves between files,
    /// hunks and lines while the find field does not have the keyboard.
    fn diff_keys(&mut self, ctx: &egui::Context) {
        let Some(view) = self.session.diff.as_mut() else {
            return;
        };
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::F)) {
            view.open_find();
        }
        if ctx.memory(|m| m.focused().is_none()) {
            view.keys(ctx);
        }
    }

    /// `esc` steps back the one level [`escape_step`] picks. An open menu
    /// closes on `esc` by itself, and the screen stays.
    pub(super) fn escape_keys(&mut self, ctx: &egui::Context) {
        if egui::Popup::is_any_open(ctx) || !ctx.input(|i| i.key_pressed(Key::Escape)) {
            return;
        }
        let step = escape_step(&Ladder {
            plain: ctx.input(|i| i.modifiers.matches_logically(egui::Modifiers::NONE)),
            label_picker: self.session.label_picker.is_some(),
            filter_bar: self.filter_bar.open,
            note: (self.session.notes.as_ref()).is_some_and(|notes| notes.draft.is_some()),
            review: self.session.review.is_some(),
            refinement: self.refinement_shown(),
            find: (self.session.diff.as_ref()).is_some_and(|view| view.find.is_some()),
            leave_prompt: self.session.leave_prompt.is_some(),
            filter: !self.filter_bar.filter.is_empty(),
            screen: &self.session.screen,
        });
        if matches!(step, Escape::LabelPicker | Escape::FilterBar) {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        }
        match step {
            Escape::LabelPicker => self.session.label_picker = None,
            Escape::FilterBar => self.filter_bar.escape(ctx),
            Escape::Note => {
                if let Some(notes) = self.session.notes.as_mut() {
                    notes.draft = None;
                }
            }
            Escape::Review => self.close_review(),
            Escape::Refinement => self.escape_refinement(ctx),
            Escape::Find => {
                if let Some(view) = self.session.diff.as_mut() {
                    view.find = None;
                }
            }
            Escape::Diff => self.close_diff(),
            Escape::LeavePrompt => self.answer_leave(LeaveChoice::KeepEditing),
            Escape::Filter => self.filter_bar.filter.clear(),
            Escape::Selection => {
                self.session.selected = None;
                self.session.picked.clear();
            }
            Escape::Preview => self.show_preview(),
            Escape::Board => self.back_to_board(),
        }
    }

    fn key_context(&self, ctx: &egui::Context) -> KeyContext {
        let place = match &self.session.screen {
            Screen::Board => Place::Board(self.selected_issue().map(|path| {
                let done = self.is_done_issue(&path);
                (path, done)
            })),
            Screen::Detail(path, mode) if self.board.find(path).is_some() => {
                Place::Issue(path.clone(), *mode)
            }
            _ => Place::Elsewhere,
        };
        let open = place.open();
        let selected = match &place {
            Place::Board(selected) => selected.as_ref().map(|(path, _)| path),
            _ => None,
        };
        KeyContext {
            typing: ctx.memory(|m| m.focused().is_some()),
            editable: open.is_some_and(|path| self.can_edit(path)),
            refinable: open
                .or(selected)
                .is_some_and(|path| self.is_todo_issue(path)),
            proposal: self.proposal_shown() && !self.refinement_shown(),
            auto: self.prefs.auto,
            popup: egui::Popup::is_any_open(ctx),
            place,
        }
    }

    fn run(&mut self, command: Command) {
        match command {
            Command::Compose => self.open_composer(),
            Command::FocusFilter => self.filter_bar.focus = true,
            Command::Refresh => self.refresh(),
            Command::Undo => {
                if !self.restore_discarded() {
                    self.undo_last();
                }
            }
            Command::Settings => self.open_settings(self.settings_tab),
            Command::Refine(path) => self.start_refinement(&path),
            Command::ToggleReview(path) => {
                if self.review_is_running(&path) {
                    self.cancel_review_run(&path);
                } else {
                    let draft = self.session.editing().is_some();
                    self.start_review(&path, draft);
                }
            }
            Command::Back if self.session.editing().is_some() => self.show_preview(),
            Command::Back if matches!(self.session.screen, Screen::Diff) => self.close_diff(),
            Command::Back => self.back_to_board(),
            // `⌘S` is the header's Save: it forces a save (overwriting a
            // conflict) and, when it went through, shows the issue again.
            Command::Save => {
                if self.session.editing().is_some() {
                    self.save_and_preview();
                } else {
                    self.flush_autosave(true);
                }
            }
            Command::Step(key) => self.move_selection(key),
            Command::Open(path) => {
                let first = self.selection_of(&path).into_iter().next();
                self.open_issue(first.unwrap_or(path));
            }
            Command::NextColumn => self.move_selection_to_next(),
            Command::Edit(path) => self.start_editing(&path),
            Command::Archive(path) => self.archive_issue(&path),
            Command::Labels(path) => self.toggle_label_picker(path),
            Command::MoveTo => self.session.open_move_menu = true,
            Command::Delete(path) => self.request_delete(path),
            Command::SwitchProject(forward) => self.switch_project(forward),
            Command::Stats => self.open_stats(),
            Command::ApplyReview => self.apply_review_proposal(),
            Command::Ask(path) => self.toggle_ask(&path),
            Command::Scroll(stride, down) => self.reader.scroll = Some((stride, down)),
        }
    }
}
