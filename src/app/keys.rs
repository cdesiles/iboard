//! What each frame does before drawing: keys, shortcuts and background work.
//! The shortcuts are a keymap: the input and where the keyboard is give a
//! list of [`Command`]s, applied in order.

use super::editor::EditorState;
use super::{
    Confirm, DetailMode, IboardApp, Instant, Job, Key, LeaveChoice, PathBuf, RESCAN_EVERY, Screen,
    Vec2, job, quitting, scan, settings_ui, widgets,
};

/// How far two fingers swipe right, in points, to go back.
const SWIPE_BACK: f32 = 120.0;

/// Where the keyboard is this frame, for the keymap.
struct KeyContext {
    /// A text field has focus.
    typing: bool,
    /// The delete confirmation is up.
    confirming: bool,
    place: Place,
    /// The open issue's text can be edited.
    editable: bool,
    /// The open issue is in Todo, so it can be refined and reviewed.
    refinable: bool,
    /// The review panel shows a proposal to apply.
    proposal: bool,
}

/// The screen the keys act on.
enum Place {
    /// The board, and the card selected on it with whether it is done.
    Board(Option<(PathBuf, bool)>),
    /// The issue open in the reader or the editor.
    Issue(PathBuf, DetailMode),
    /// The Diff screen, or an issue no longer on the board.
    Elsewhere,
}

impl Place {
    fn open(&self) -> Option<&PathBuf> {
        match self {
            Place::Issue(path, _) => Some(path),
            _ => None,
        }
    }

    /// What `⌘U`, `⌘K` and `Del` act on: the card selected on the board, or
    /// the issue the reader shows.
    fn target(&self) -> Option<&PathBuf> {
        match self {
            Place::Board(selected) => selected.as_ref().map(|(path, _)| path),
            Place::Issue(path, DetailMode::Preview) => Some(path),
            _ => None,
        }
    }
}

/// One thing a key asked for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Compose,
    FocusFilter,
    Refresh,
    Undo,
    Settings,
    /// `⌘R` on a Todo issue: refine it with the challenger.
    Refine(PathBuf),
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
    /// `⌃⇥` / `⌃⇧⇥` on the board: the next or the previous project.
    SwitchProject(bool),
    /// `⌘I` on the board: Project stats.
    Stats,
    /// `⌘U`: Update with AI on the open issue or the selected card, or cancel it.
    Update(PathBuf),
    /// `⌘↩` with a review's proposal shown: apply it.
    ApplyReview,
    /// `⌘K`: Ask AI on the open issue or the selected card, or stop the ask.
    Ask(PathBuf),
}

/// The shortcuts that fire for `input` where the keyboard is.
fn keymap(input: &egui::InputState, cx: &KeyContext) -> Vec<Command> {
    let pressed = |key: Key| input.key_pressed(key);
    let plain = input.modifiers.is_none();
    let mut out = command_keys(input, cx);
    if let Place::Board(selected) = &cx.place
        && !cx.typing
        && !cx.confirming
    {
        out.extend(board_keys(input, selected.clone()));
    }
    let ctrl = input.modifiers.ctrl && !input.modifiers.mac_cmd && !input.modifiers.alt;
    if matches!(cx.place, Place::Board(_)) && !cx.confirming && ctrl && pressed(Key::Tab) {
        out.push(Command::SwitchProject(!input.modifiers.shift));
    }
    if let Place::Issue(path, DetailMode::Preview) = &cx.place
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
        out.extend(cx.place.target().cloned().map(Command::Delete));
    }
    out
}

/// The `⌘` shortcuts, in the order they run: `⇧` only tells `⌘R` from `⌘⇧R`.
fn command_keys(input: &egui::InputState, cx: &KeyContext) -> Vec<Command> {
    if !input.modifiers.command {
        return Vec::new();
    }
    let board = matches!(cx.place, Place::Board(_));
    let shift = input.modifiers.shift;
    let target = cx.place.target().filter(|_| !cx.typing);
    let open = cx.place.open();
    let refine = open.filter(|_| cx.refinable).cloned();
    [
        (Key::N, Some(Command::Compose)),
        (Key::F, board.then_some(Command::FocusFilter)),
        (Key::I, (board && !cx.typing).then_some(Command::Stats)),
        (
            Key::Enter,
            (cx.proposal && !cx.typing).then_some(Command::ApplyReview),
        ),
        (Key::U, target.cloned().map(Command::Update)),
        (Key::K, target.cloned().map(Command::Ask)),
        (
            Key::R,
            (!shift).then(|| refine.map_or(Command::Refresh, Command::Refine)),
        ),
        // In a text field `⌘Z` stays the field's own undo.
        (Key::Z, (board && !cx.typing).then_some(Command::Undo)),
        (Key::Comma, Some(Command::Settings)),
        (
            Key::R,
            open.filter(|_| shift).cloned().map(Command::ToggleReview),
        ),
        (Key::OpenBracket, (!board).then_some(Command::Back)),
        (Key::S, Some(Command::Save)),
    ]
    .into_iter()
    .filter(|(key, _)| input.key_pressed(*key))
    .filter_map(|(_, command)| command)
    .collect()
}

/// On the board, the arrow keys select a card; `↩` opens it, `⌘↩` moves it
/// to the next column, `E` edits, `A` archives a done one, `L` labels it.
fn board_keys(input: &egui::InputState, selected: Option<(PathBuf, bool)>) -> Vec<Command> {
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
    let Some((path, done)) = selected else {
        return out;
    };
    let command_enter = input.modifiers.command && input.key_pressed(Key::Enter);
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
    /// The work each frame does before drawing: what runs in the background,
    /// menu requests, keys and shortcuts, the autosave and the rescan. Tests
    /// drive it with no window.
    pub(super) fn frame_logic(&mut self, ctx: &egui::Context) {
        // A project switch sets the title; apply it now that we have a context.
        if let Some(title) = self.pending_title.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        self.forget_undo_on_action(ctx);
        self.poll_jobs(ctx);
        self.window_requests(ctx);
        let confirming = self.confirm_keys(ctx);
        if self.settings.is_none() {
            self.watch_config();
        }
        let welcoming = self.welcoming();
        if !self.modal_keys(ctx, confirming) && !welcoming {
            self.screen_keys(ctx, confirming);
        }
        self.autosave();
        if !welcoming {
            self.rescan(ctx);
        }
        self.remember_splits(ctx);
    }

    /// Adopts the background work's answers as soon as they land. Each poll
    /// says how soon to look again while its work runs: the frame wakes up at
    /// the soonest.
    fn poll_jobs(&mut self, ctx: &egui::Context) {
        let wake = [
            self.poll_review(),
            self.poll_refinement(),
            self.poll_split(),
            self.poll_namings(ctx),
            self.poll_draft(),
            self.poll_design_draft(),
            self.poll_enrichment(),
            self.poll_build(),
            self.poll_asks(),
            self.poll_stats(),
        ];
        if let Some(after) = wake.into_iter().flatten().min() {
            ctx.request_repaint_after(after);
        }
    }

    /// The dialog up owns the keyboard: Settings, which `⌘,` closes as it
    /// opened it, Groups, the project menu, Project stats, Ask AI or
    /// NewIssueModal. True when one does.
    fn modal_keys(&mut self, ctx: &egui::Context, confirming: bool) -> bool {
        if self.settings.is_some() {
            if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Comma))
                && self
                    .settings
                    .as_mut()
                    .is_some_and(|dialog| dialog.close() == settings_ui::Action::Close)
            {
                self.settings = None;
            }
            return true;
        }
        if self.groups_dialog.is_some() {
            return true;
        }
        if self.project_menu.is_some() {
            self.project_menu_keys(ctx);
        } else if self.session.stats.is_some() {
            self.stats_keys(ctx);
        } else if self.session.ask_draft.is_some() {
            self.ask_keys(ctx);
        } else if self.composer_open() && !confirming {
            self.new_issue_keys(ctx);
            self.paste_images(ctx);
        } else {
            return false;
        }
        true
    }

    /// The keys of the screen on show, while no dialog owns the keyboard:
    /// `esc`, a paste, the Diff's own keys, the shortcuts, the swipe back and
    /// the double-click that edits.
    fn screen_keys(&mut self, ctx: &egui::Context, confirming: bool) {
        if !confirming {
            self.escape_keys(ctx);
            self.paste_images(ctx);
        }
        if matches!(self.session.screen, Screen::Diff) && !confirming {
            self.diff_keys(ctx);
        }
        let cx = self.key_context(ctx, confirming || self.refinement_shown());
        let mut commands = ctx.input(|i| keymap(i, &cx));
        if !cx.confirming && !matches!(cx.place, Place::Board(_)) && self.swiped_back(ctx) {
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
        // A double-click on the page (not the header) edits, as `E` does.
        if cx.editable
            && !self.writing_note()
            && let Place::Issue(path, DetailMode::Preview) = cx.place
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

    /// The window's own requests: a quit, Settings or Project Stats picked
    /// from the macOS menu, a later `iboard`'s board, and closing the window,
    /// one more way of leaving the editor.
    fn window_requests(&mut self, ctx: &egui::Context) {
        self.answer_handovers(ctx);
        // A quit from the menu arrives as a flag rather than a window close,
        // so it can run the save prompt before we exit.
        #[cfg(target_os = "macos")]
        if iboard::menu::take_quit_request() {
            self.request_quit();
        }
        if quitting::requested() {
            self.save_pending();
            self.quit(ctx);
        }
        #[cfg(target_os = "macos")]
        if iboard::menu::take_settings_request() && self.settings.is_none() {
            self.open_settings(self.settings_tab);
        }
        #[cfg(target_os = "macos")]
        if iboard::menu::take_stats_request() {
            self.open_stats();
        }
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.closing {
            self.request_quit();
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
        let Some(confirm) = self.session.confirm.clone() else {
            return false;
        };
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.session.confirm = None;
        } else if widgets::enter_confirms(ctx) && ctx.input(|i| i.key_pressed(Key::Enter)) {
            match confirm {
                Confirm::Delete(paths) => self.delete_issues(&paths),
                Confirm::Quit(_) => self.quit_anyway(),
            }
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
            self.close_review();
        } else if self.refinement_shown() {
            self.escape_refinement(ctx);
        } else {
            match self.session.screen.clone() {
                Screen::Diff if self.session.diff.as_ref().is_some_and(|v| v.find.is_some()) => {
                    if let Some(view) = self.session.diff.as_mut() {
                        view.find = None;
                    }
                }
                Screen::Diff => self.close_diff(),
                _ if self.session.leave_prompt.is_some() => {
                    self.answer_leave(LeaveChoice::KeepEditing)
                }
                Screen::Board if !self.filter_bar.filter.is_empty() => {
                    self.filter_bar.filter.clear()
                }
                Screen::Board => {
                    self.session.selected = None;
                    self.session.picked.clear();
                }
                Screen::Detail(_, DetailMode::Edit) => self.show_preview(),
                Screen::Detail(_, DetailMode::Preview) => self.back_to_board(),
            }
        }
    }

    fn key_context(&self, ctx: &egui::Context, confirming: bool) -> KeyContext {
        let place = match &self.session.screen {
            Screen::Board => Place::Board(self.selected_issue().map(|path| {
                let done = self
                    .board
                    .find(&path)
                    .is_some_and(|issue| issue.done_at.is_some());
                (path, done)
            })),
            Screen::Detail(path, mode) if self.board.find(path).is_some() => {
                Place::Issue(path.clone(), *mode)
            }
            _ => Place::Elsewhere,
        };
        let open = place.open();
        KeyContext {
            typing: ctx.memory(|m| m.focused().is_some()),
            confirming,
            editable: open.is_some_and(|path| self.can_edit(path)),
            refinable: open.is_some_and(|path| self.is_todo_issue(path)),
            proposal: self.proposal_shown() && !self.refinement_shown(),
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
            Command::Delete(path) => self.request_delete(path),
            Command::SwitchProject(forward) => self.switch_project(forward),
            Command::Stats => self.open_stats(),
            Command::Update(path) => self.toggle_update(&path),
            Command::ApplyReview => self.apply_review_proposal(),
            Command::Ask(path) => self.toggle_ask(&path),
        }
    }

    /// Debounced autosave, while Auto is on.
    fn autosave(&mut self) {
        let due = self.prefs.auto
            && self
                .session
                .editor
                .as_ref()
                .is_some_and(EditorState::autosave_due);
        if due {
            self.flush_autosave(false);
        }
    }

    /// Auto, or Play watching Ready, reads the board again every
    /// [`RESCAN_EVERY`], off the UI thread, and takes it unless the board
    /// changed meanwhile.
    pub(super) fn rescan(&mut self, ctx: &egui::Context) {
        if self.prefs.auto || self.watching_ready() {
            if self.session.scan.is_none() && self.last_refresh.elapsed() >= RESCAN_EVERY {
                self.last_refresh = Instant::now();
                let source = self.source();
                let rescan = Job::spawn(move |_| scan(&source));
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
    }
}
