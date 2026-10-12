//! What each frame does before drawing: keys, shortcuts and background work.
//! The shortcuts are a keymap: the input and where the keyboard is give a
//! list of [`Command`]s, applied in order.

use super::*;

/// How far two fingers swipe right, in points, to go back.
const SWIPE_BACK: f32 = 120.0;

/// Where the keyboard is this frame, for the keymap.
struct KeyContext {
    /// A text field has focus.
    typing: bool,
    /// The delete confirmation is up.
    confirming: bool,
    board: bool,
    /// The board shows one column full screen (ColumnFocus).
    focused: bool,
    /// The issue open in the reader or the editor.
    open: Option<PathBuf>,
    editing: bool,
    composer: bool,
    /// The card selected on the board, and whether it is done.
    selected: Option<(PathBuf, bool)>,
    /// The open issue's text can be edited.
    editable: bool,
}

/// One thing a key asked for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Compose,
    FocusFilter,
    /// `⌘1`–`⌘4`: show that column full screen, or leave it.
    ToggleColumn(usize),
    Refresh,
    Undo,
    Settings,
    /// `⌘⇧R`: review the open issue, or cancel the review running.
    ToggleReview(PathBuf),
    /// `⌘[`: one level back.
    Back,
    Save,
    Step(Key),
    Open(PathBuf),
    NextColumn,
    Edit(PathBuf),
    Archive(PathBuf),
    Labels(PathBuf),
    Delete(PathBuf),
}

/// The shortcuts that fire for `input` where the keyboard is.
fn keymap(input: &egui::InputState, cx: &KeyContext) -> Vec<Command> {
    let pressed = |key: Key| input.key_pressed(key);
    let command = |key: Key| input.modifiers.command && pressed(key);
    let plain = input.modifiers.is_none();
    let mut out = Vec::new();
    if command(Key::N) {
        out.push(Command::Compose);
    }
    if cx.board && command(Key::F) {
        out.push(Command::FocusFilter);
    }
    if cx.board && !cx.typing {
        for (at, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4]
            .into_iter()
            .enumerate()
        {
            if command(key) {
                out.push(Command::ToggleColumn(at));
            }
        }
    }
    if command(Key::R) && !input.modifiers.shift {
        out.push(Command::Refresh);
    }
    // In a text field `⌘Z` stays the field's own undo.
    if cx.board && !cx.typing && command(Key::Z) {
        out.push(Command::Undo);
    }
    if command(Key::Comma) {
        out.push(Command::Settings);
    }
    if command(Key::R)
        && input.modifiers.shift
        && let Some(path) = &cx.open
    {
        out.push(Command::ToggleReview(path.clone()));
    }
    if command(Key::OpenBracket) && (!cx.board || cx.focused) {
        out.push(Command::Back);
    }
    if command(Key::S) {
        out.push(Command::Save);
    }
    if cx.board && !cx.typing && !cx.confirming {
        out.extend(board_keys(input, cx));
    }
    let reading = cx.open.as_ref().filter(|_| !cx.editing);
    if let Some(path) = reading
        && !cx.typing
        && !cx.confirming
        && plain
    {
        if pressed(Key::L) {
            out.push(Command::Labels(path.clone()));
        }
        if cx.editable && pressed(Key::E) {
            out.push(Command::Edit(path.clone()));
        }
    }
    // `Del` (or Backspace) asks to delete the selected card, or the issue the
    // reader shows. In the editor the key belongs to the text.
    let delete = plain && (pressed(Key::Delete) || pressed(Key::Backspace));
    if delete && !cx.typing && !cx.confirming {
        let target = if cx.board {
            cx.selected.as_ref().map(|(path, _)| path)
        } else {
            reading
        };
        out.extend(target.map(|path| Command::Delete(path.clone())));
    }
    out
}

/// On the board, the arrow keys select a card; `↩` opens it, `⌘↩` moves it
/// to the next column, `E` edits, `A` archives a done one, `L` labels it.
fn board_keys(input: &egui::InputState, cx: &KeyContext) -> Vec<Command> {
    let plain = input.modifiers.is_none();
    let mut out: Vec<Command> = [
        Key::ArrowUp,
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
    ]
    .into_iter()
    .filter(|key| plain && input.key_pressed(*key))
    .map(Command::Step)
    .collect();
    let Some((path, done)) = cx.selected.clone() else {
        return out;
    };
    let command_enter = !cx.composer && input.modifiers.command && input.key_pressed(Key::Enter);
    out.extend(if plain && input.key_pressed(Key::Enter) {
        Some(Command::Open(path))
    } else if command_enter {
        Some(Command::NextColumn)
    } else if plain && input.key_pressed(Key::E) {
        Some(Command::Edit(path))
    } else if done && plain && input.key_pressed(Key::A) {
        Some(Command::Archive(path))
    } else if plain && input.key_pressed(Key::L) {
        Some(Command::Labels(path))
    } else {
        None
    });
    out
}

impl IboardApp {
    /// The work each frame does before drawing: keys and shortcuts, menu
    /// requests, and what runs in the background. Tests drive it with no
    /// window.
    pub(super) fn frame_logic(&mut self, ctx: &egui::Context) {
        // A project switch sets the title; apply it now that we have a context.
        if let Some(title) = self.pending_title.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        // Adopt an agent's answer as soon as it lands.
        self.poll_review();
        self.poll_split();
        self.poll_naming(ctx);
        self.poll_design_draft(ctx);
        self.poll_enrichment(ctx);
        self.window_requests(ctx);

        let confirming = self.confirm_keys(ctx);
        // The settings dialog owns the keyboard while it is up; `⌘,` closes
        // it as it opened it.
        if self.settings.is_some() {
            if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Comma)) {
                self.settings = None;
            }
            self.background(ctx);
            return;
        }
        if !confirming {
            self.escape_keys(ctx);
        }
        let cx = self.key_context(ctx, confirming);
        let mut commands = ctx.input(|i| keymap(i, &cx));
        if !confirming && (!cx.board || cx.focused) && self.swiped_back(ctx) {
            commands.push(Command::Back);
        }
        for command in commands {
            self.run(command);
        }
        // Saved some other way while the prompt was up: carry on leaving.
        if let Some(to) = self.session.leave_prompt
            && !self.has_unsaved_edits()
        {
            self.go(to);
        }
        // A double-click on the page (not the header) edits, as `E` does.
        if cx.editable
            && !cx.editing
            && !self.writing_note()
            && let Some(path) = cx.open
            && ctx.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|pos| self.reader.preview_rect.contains(pos))
            })
        {
            self.start_editing(&path);
        }
        self.background(ctx);
    }

    /// A two-finger swipe to the right on the trackpad, as a browser's
    /// back. It reaches egui as horizontal scrolling: added up while it lasts,
    /// it goes back once past [`SWIPE_BACK`], then waits for the fingers to
    /// lift so the momentum after it never goes back twice.
    fn swiped_back(&mut self, ctx: &egui::Context) -> bool {
        let delta = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta,
                        ..
                    } => Some(*delta),
                    _ => None,
                })
                .fold(Vec2::ZERO, |sum, delta| sum + delta)
        });
        if delta.x == 0.0 && delta.y == 0.0 {
            self.session.swipe = Some(0.0);
            return false;
        }
        let Some(swipe) = self.session.swipe.as_mut() else {
            return false;
        };
        if delta.x.abs() < 2.0 * delta.y.abs() {
            *swipe = 0.0;
            return false;
        }
        *swipe += delta.x;
        if *swipe < SWIPE_BACK {
            return false;
        }
        self.session.swipe = None;
        true
    }

    /// The window's own requests: a quit or Settings picked from the macOS
    /// menu, and closing the window, one more way of leaving the editor.
    fn window_requests(&mut self, ctx: &egui::Context) {
        // A quit from the menu arrives as a flag rather than a window close,
        // so it can run the save prompt before we exit.
        #[cfg(target_os = "macos")]
        if iboard::menu::take_quit_request() {
            self.leave(Leave::Quit);
        }
        #[cfg(target_os = "macos")]
        if iboard::menu::take_settings_request() && self.settings.is_none() {
            self.open_settings(self.settings_tab);
        }
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.closing {
            self.leave(Leave::Quit);
            if !self.closing {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        if std::mem::take(&mut self.send_close) {
            self.quit(ctx);
        }
    }

    /// The delete confirmation owns Escape and Enter while it is up. True
    /// while it is.
    fn confirm_keys(&mut self, ctx: &egui::Context) -> bool {
        let Some(Confirm::Delete(path)) = self.session.confirm.clone() else {
            return false;
        };
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.session.confirm = None;
        } else if ctx.input(|i| i.key_pressed(Key::Enter)) {
            self.delete_issue(&path);
        }
        true
    }

    /// `esc` steps back one level: the LabelPicker closes first (dropping
    /// what its field holds), then the filter takes it, then a note being
    /// written, a review (cancelled, or its panel closed), and the screens.
    fn escape_keys(&mut self, ctx: &egui::Context) {
        let take = || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        if self.session.label_picker.is_some() && take() {
            self.session.label_picker = None;
        }
        if matches!(self.session.screen, Screen::Board) && self.filter_bar.open && take() {
            self.filter_bar.escape(ctx);
        }
        if !ctx.input(|i| i.key_pressed(Key::Escape)) {
            return;
        }
        if let Some(notes) = self
            .session
            .notes
            .as_mut()
            .filter(|notes| notes.draft.is_some())
        {
            notes.draft = None;
        } else if self.session.review.is_some() {
            self.cancel_review();
        } else {
            match self.session.screen.clone() {
                _ if self.session.leave_prompt.is_some() => {
                    self.answer_leave(LeaveChoice::KeepEditing)
                }
                Screen::Board if self.session.composer.is_some() => self.composer_escape(),
                Screen::Board if !self.filter_bar.filter.is_empty() => {
                    self.filter_bar.filter.clear()
                }
                Screen::Board if self.column_focus.column.is_some() => self.unfocus_column(),
                Screen::Board => self.session.selected = None,
                Screen::Detail(_) if self.session.detail_mode == DetailMode::Edit => {
                    self.show_preview()
                }
                Screen::Detail(_) => self.back_to_board(),
            }
        }
    }

    fn key_context(&self, ctx: &egui::Context, confirming: bool) -> KeyContext {
        let open = match &self.session.screen {
            Screen::Detail(path) if self.board.find(path).is_some() => Some(path.clone()),
            _ => None,
        };
        KeyContext {
            typing: ctx.memory(|m| m.focused().is_some()),
            confirming,
            board: matches!(self.session.screen, Screen::Board),
            focused: self.column_focus.column.is_some(),
            editable: open.as_deref().is_some_and(|path| self.can_edit(path)),
            open,
            editing: self.session.detail_mode == DetailMode::Edit,
            composer: self.session.composer.is_some(),
            selected: self.selected_issue().map(|path| {
                let done = self
                    .board
                    .find(&path)
                    .is_some_and(|issue| issue.done_at.is_some());
                (path, done)
            }),
        }
    }

    fn run(&mut self, command: Command) {
        match command {
            Command::Compose => self.open_composer(),
            Command::FocusFilter => self.filter_bar.focus = true,
            Command::ToggleColumn(at) => {
                if let Some(dir) = self.board.columns.get(at).map(|c| c.dir.clone()) {
                    if self.column_focus.column.as_ref() == Some(&dir) {
                        self.unfocus_column();
                    } else {
                        self.focus_column(dir);
                    }
                }
            }
            Command::Refresh => self.refresh(),
            Command::Undo => self.undo_last(),
            Command::Settings => self.open_settings(self.settings_tab),
            Command::ToggleReview(path) => {
                if self.review_is_running(&path) {
                    self.cancel_review();
                } else {
                    let draft = self.session.detail_mode == DetailMode::Edit;
                    self.start_review(&path, draft);
                }
            }
            Command::Back if self.session.detail_mode == DetailMode::Edit => self.show_preview(),
            Command::Back if matches!(self.session.screen, Screen::Board) => self.unfocus_column(),
            Command::Back => self.back_to_board(),
            // `⌘S` is the header's Save: it forces a save (overwriting a
            // conflict) and, when it went through, shows the issue again. On
            // the board it adds the composer's issue, as `↩` does.
            Command::Save => {
                if matches!(self.session.screen, Screen::Board) && self.session.composer.is_some() {
                    self.submit_composer(false);
                } else if self.session.detail_mode == DetailMode::Edit {
                    self.save_and_preview();
                } else {
                    self.flush_autosave(true);
                }
            }
            Command::Step(key) => self.move_selection(key),
            Command::Open(path) => self.open_issue(path),
            Command::NextColumn => self.move_selection_to_next(),
            Command::Edit(path) => self.start_editing(&path),
            Command::Archive(path) => self.archive_issue(&path),
            Command::Labels(path) => self.toggle_label_picker(path),
            Command::Delete(path) => self.request_delete(path),
        }
    }

    /// The work each frame does whatever the keyboard said: autosave, the
    /// periodic refresh, and waking up for a running review.
    pub(super) fn background(&mut self, ctx: &egui::Context) {
        // Debounced autosave, while Auto is on.
        let due = self.auto
            && self
                .session
                .editor
                .as_ref()
                .map(|e| {
                    e.is_dirty()
                        && e.conflict.is_none()
                        && e.last_edit
                            .map(|t| t.elapsed() >= AUTOSAVE_AFTER)
                            .unwrap_or(false)
                })
                .unwrap_or(false);
        if due {
            self.flush_autosave(false);
        }

        // Auto reads the board again every RESCAN_EVERY, off the UI thread.
        if self.auto {
            if self.session.scan.is_none() && self.last_refresh.elapsed() >= RESCAN_EVERY {
                self.last_refresh = Instant::now();
                let located = self.located.clone();
                let rescan = Job::spawn(move |_| scan(&located));
                self.session.scan = Some((rescan, self.board_generation));
            }
            ctx.request_repaint_after(RESCAN_EVERY);
        }
        if let Some((rescan, generation)) = &self.session.scan {
            match rescan.answer() {
                Some(scanned) => {
                    let current = *generation == self.board_generation;
                    self.session.scan = None;
                    if current {
                        self.adopt(scanned);
                    }
                }
                None => ctx.request_repaint_after(job::POLL_EVERY),
            }
        }
        // Poll a running review a few times a second.
        if self.session.review.as_ref().is_some_and(|review| {
            matches!(
                review.status,
                ReviewStatus::Running { .. } | ReviewStatus::Splitting { .. }
            )
        }) {
            ctx.request_repaint_after(job::POLL_EVERY);
        }
    }
}
