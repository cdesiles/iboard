//! The keymap: the input and where the keyboard is give a list of
//! [`Command`]s, and the open levels give the one `esc` closes.

use super::{DetailMode, Key, PathBuf, Screen, Stride};

/// Where the keyboard is this frame, for the keymap.
pub(super) struct KeyContext {
    /// A text field has focus.
    pub(super) typing: bool,
    pub(super) place: Place,
    /// The open issue's text can be edited.
    pub(super) editable: bool,
    /// The open issue or the selected card is in Todo, so it can be refined
    /// and reviewed.
    pub(super) refinable: bool,
    /// The review panel shows a proposal to apply.
    pub(super) proposal: bool,
    /// Settings › General's Auto: the board reloads itself.
    pub(super) auto: bool,
    /// A menu is open and takes the plain keys.
    pub(super) popup: bool,
}

/// The screen the keys act on.
pub(super) enum Place {
    /// The board, and the card selected on it with whether it is done.
    Board(Option<(PathBuf, bool)>),
    /// The issue open in the reader or the editor.
    Issue(PathBuf, DetailMode),
    /// The Diff screen, or an issue no longer on the board.
    Elsewhere,
}

impl Place {
    pub(super) fn open(&self) -> Option<&PathBuf> {
        match self {
            Place::Issue(path, _) => Some(path),
            _ => None,
        }
    }

    /// What `Del` acts on: the card selected on the board, or the issue the
    /// reader shows.
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
pub(super) enum Command {
    Compose,
    FocusFilter,
    Refresh,
    Undo,
    Settings,
    /// `⌘R` on a Todo issue: refine it with the challenger.
    Refine(PathBuf),
    /// `⌘⇧R`: review the open issue or the selected Todo card, or cancel the
    /// review running.
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
    /// `↑ ↓`, `fn ↑ ↓`, `space`, `⇧space` on the Reader's page, down when true.
    Scroll(Stride, bool),
}

/// The shortcuts that fire for `input` where the keyboard is.
pub(super) fn keymap(input: &egui::InputState, cx: &KeyContext) -> Vec<Command> {
    let pressed = |key: Key| input.key_pressed(key);
    let plain = input.modifiers.is_none();
    let mut out = command_keys(input, cx);
    if let Place::Board(selected) = &cx.place
        && !cx.typing
    {
        out.extend(board_keys(input, selected.clone()));
    }
    let ctrl = input.modifiers.ctrl && !input.modifiers.mac_cmd && !input.modifiers.alt;
    if matches!(cx.place, Place::Board(_)) && ctrl && pressed(Key::Tab) {
        out.push(Command::SwitchProject(!input.modifiers.shift));
    }
    if let Place::Issue(path, DetailMode::Preview) = &cx.place
        && !cx.typing
        && !cx.popup
    {
        if plain && pressed(Key::L) {
            out.push(Command::Labels(path.clone()));
        }
        if plain && cx.editable && (pressed(Key::E) || pressed(Key::Enter)) {
            out.push(Command::Edit(path.clone()));
        }
        out.extend(scroll_keys(input));
    }
    // `Del` (or Backspace) asks to delete the selected card, or the issue the
    // reader shows. In the editor the key belongs to the text.
    let delete = plain && (pressed(Key::Delete) || pressed(Key::Backspace));
    if delete && !cx.typing {
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
    // The Editor's text keeps the keyboard: these keys still reach the issue.
    let target = match &cx.place {
        Place::Issue(path, DetailMode::Edit) => Some(path),
        place => place.target().filter(|_| !cx.typing),
    };
    let open = cx.place.open();
    let refine = open.filter(|_| cx.refinable).cloned();
    let review = match &cx.place {
        Place::Board(Some((path, _))) if cx.refinable && !cx.typing => Some(path),
        place => place.open(),
    };
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
            refine
                .map(Command::Refine)
                .or((!cx.auto).then_some(Command::Refresh))
                .filter(|_| !shift),
        ),
        // In a text field `⌘Z` stays the field's own undo.
        (Key::Z, (board && !cx.typing).then_some(Command::Undo)),
        (Key::Comma, Some(Command::Settings)),
        (
            Key::R,
            review.filter(|_| shift).cloned().map(Command::ToggleReview),
        ),
        (Key::OpenBracket, (!board).then_some(Command::Back)),
        (Key::S, Some(Command::Save)),
    ]
    .into_iter()
    .filter(|(key, _)| input.key_pressed(*key))
    .filter_map(|(_, command)| command)
    .collect()
}

/// The keys that scroll the Reader's page by a line or by a page.
fn scroll_keys(input: &egui::InputState) -> Vec<Command> {
    let plain = input.modifiers.is_none();
    let shift = input.modifiers.shift_only();
    [
        (plain, Key::ArrowUp, Stride::Line, false),
        (plain, Key::ArrowDown, Stride::Line, true),
        (plain, Key::PageUp, Stride::Page, false),
        (plain, Key::PageDown, Stride::Page, true),
        (plain, Key::Space, Stride::Page, true),
        (shift, Key::Space, Stride::Page, false),
    ]
    .into_iter()
    .filter(|(held, key, ..)| *held && input.key_pressed(*key))
    .map(|(_, _, stride, down)| Command::Scroll(stride, down))
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

/// What is open when `esc` is pressed, for [`escape_step`].
pub(super) struct Ladder<'a> {
    /// `esc` came with neither `⌘` nor `⌃`: only then does it close the
    /// LabelPicker or the filter bar.
    pub(super) plain: bool,
    pub(super) label_picker: bool,
    pub(super) filter_bar: bool,
    /// A note is being written.
    pub(super) note: bool,
    pub(super) review: bool,
    pub(super) refinement: bool,
    /// The Diff's find field.
    pub(super) find: bool,
    pub(super) leave_prompt: bool,
    /// The board's filter holds some text.
    pub(super) filter: bool,
    pub(super) screen: &'a Screen,
}

/// The level `esc` closes, one at a time.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Escape {
    LabelPicker,
    FilterBar,
    Note,
    Review,
    Refinement,
    Find,
    Diff,
    LeavePrompt,
    Filter,
    Selection,
    Preview,
    Board,
}

/// The top level open, which `esc` closes: the LabelPicker, the filter bar, a
/// note being written, a review, the refinement, then the screens.
pub(super) fn escape_step(open: &Ladder) -> Escape {
    let board = matches!(open.screen, Screen::Board);
    let diff = matches!(open.screen, Screen::Diff);
    let editing = matches!(open.screen, Screen::Detail(_, DetailMode::Edit));
    [
        (open.plain && open.label_picker, Escape::LabelPicker),
        (open.plain && board && open.filter_bar, Escape::FilterBar),
        (open.note, Escape::Note),
        (open.review, Escape::Review),
        (open.refinement, Escape::Refinement),
        (diff && open.find, Escape::Find),
        (diff, Escape::Diff),
        (open.leave_prompt, Escape::LeavePrompt),
        (board && open.filter, Escape::Filter),
        (board, Escape::Selection),
        (editing, Escape::Preview),
    ]
    .into_iter()
    .find_map(|(up, level)| up.then_some(level))
    .unwrap_or(Escape::Board)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Modifiers;

    fn input(modifiers: Modifiers, key: Key) -> egui::InputState {
        let raw = egui::RawInput {
            events: vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ],
            ..Default::default()
        };
        egui::InputState::default().begin_pass(raw, false, 1.0, egui::InputOptions::default())
    }

    fn at(place: Place) -> KeyContext {
        KeyContext {
            typing: false,
            place,
            editable: true,
            refinable: false,
            proposal: false,
            auto: false,
            popup: false,
        }
    }

    fn card(done: bool) -> Place {
        Place::Board(Some((PathBuf::from("a.md"), done)))
    }

    fn reading() -> Place {
        Place::Issue(PathBuf::from("a.md"), DetailMode::Preview)
    }

    fn keys(modifiers: Modifiers, key: Key, cx: &KeyContext) -> Vec<Command> {
        keymap(&input(modifiers, key), cx)
    }

    fn path() -> PathBuf {
        PathBuf::from("a.md")
    }

    #[test]
    fn enter_opens_the_selected_card_and_command_enter_moves_it_on() {
        let cx = at(card(false));
        assert_eq!(
            keys(Modifiers::NONE, Key::Enter, &cx),
            [Command::Open(path())]
        );
        assert_eq!(
            keys(Modifiers::COMMAND, Key::Enter, &cx),
            [Command::NextColumn]
        );
    }

    #[test]
    fn only_a_done_card_archives() {
        assert!(keys(Modifiers::NONE, Key::A, &at(card(false))).is_empty());
        assert_eq!(
            keys(Modifiers::NONE, Key::A, &at(card(true))),
            [Command::Archive(path())]
        );
    }

    #[test]
    fn a_focused_field_keeps_the_plain_keys() {
        let mut cx = at(card(false));
        cx.typing = true;
        assert!(keys(Modifiers::NONE, Key::ArrowDown, &cx).is_empty());
        assert!(keys(Modifiers::NONE, Key::Backspace, &cx).is_empty());
        assert_eq!(keys(Modifiers::COMMAND, Key::N, &cx), [Command::Compose]);
    }

    #[test]
    fn delete_asks_for_the_selected_card_or_the_issue_read() {
        assert_eq!(
            keys(Modifiers::NONE, Key::Delete, &at(card(false))),
            [Command::Delete(path())]
        );
        assert_eq!(
            keys(Modifiers::NONE, Key::Backspace, &at(reading())),
            [Command::Delete(path())]
        );
        let editing = at(Place::Issue(path(), DetailMode::Edit));
        assert!(keys(Modifiers::NONE, Key::Delete, &editing).is_empty());
    }

    #[test]
    fn command_r_refines_a_todo_issue_or_refreshes_unless_auto() {
        let mut cx = at(reading());
        assert_eq!(keys(Modifiers::COMMAND, Key::R, &cx), [Command::Refresh]);
        cx.refinable = true;
        assert_eq!(
            keys(Modifiers::COMMAND, Key::R, &cx),
            [Command::Refine(path())]
        );
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        assert_eq!(keys(shift, Key::R, &cx), [Command::ToggleReview(path())]);
        let mut auto = at(Place::Board(None));
        auto.auto = true;
        assert!(keys(Modifiers::COMMAND, Key::R, &auto).is_empty());
    }

    #[test]
    fn control_tab_switches_projects_from_the_board_only() {
        let board = at(Place::Board(None));
        assert_eq!(
            keys(Modifiers::CTRL, Key::Tab, &board),
            [Command::SwitchProject(true)]
        );
        assert_eq!(
            keys(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab, &board),
            [Command::SwitchProject(false)]
        );
        assert!(keys(Modifiers::CTRL, Key::Tab, &at(reading())).is_empty());
    }

    #[test]
    fn the_reader_scrolls_edits_and_labels() {
        let cx = at(reading());
        assert_eq!(
            keys(Modifiers::NONE, Key::Space, &cx),
            [Command::Scroll(Stride::Page, true)]
        );
        assert_eq!(
            keys(Modifiers::SHIFT, Key::Space, &cx),
            [Command::Scroll(Stride::Page, false)]
        );
        assert_eq!(
            keys(Modifiers::NONE, Key::ArrowUp, &cx),
            [Command::Scroll(Stride::Line, false)]
        );
        assert_eq!(keys(Modifiers::NONE, Key::E, &cx), [Command::Edit(path())]);
        assert_eq!(
            keys(Modifiers::NONE, Key::L, &cx),
            [Command::Labels(path())]
        );
        let mut menu = at(reading());
        menu.popup = true;
        assert!(keys(Modifiers::NONE, Key::Space, &menu).is_empty());
    }

    fn ladder(screen: &Screen) -> Ladder<'_> {
        Ladder {
            plain: true,
            label_picker: false,
            filter_bar: false,
            note: false,
            review: false,
            refinement: false,
            find: false,
            leave_prompt: false,
            filter: false,
            screen,
        }
    }

    #[test]
    fn esc_closes_the_label_picker_before_the_filter_bar() {
        let board = Screen::Board;
        let open = Ladder {
            label_picker: true,
            filter_bar: true,
            note: true,
            ..ladder(&board)
        };
        assert_eq!(escape_step(&open), Escape::LabelPicker);
        let filtering = Ladder {
            label_picker: false,
            ..open
        };
        assert_eq!(escape_step(&filtering), Escape::FilterBar);
    }

    #[test]
    fn esc_with_command_skips_the_picker_and_the_filter_bar() {
        let board = Screen::Board;
        let open = Ladder {
            plain: false,
            label_picker: true,
            filter_bar: true,
            note: true,
            ..ladder(&board)
        };
        assert_eq!(escape_step(&open), Escape::Note);
    }

    #[test]
    fn esc_closes_a_review_before_the_refinement() {
        let screen = Screen::Detail(path(), DetailMode::Preview);
        let open = Ladder {
            review: true,
            refinement: true,
            ..ladder(&screen)
        };
        assert_eq!(escape_step(&open), Escape::Review);
        let refining = Ladder {
            review: false,
            ..open
        };
        assert_eq!(escape_step(&refining), Escape::Refinement);
    }

    #[test]
    fn esc_closes_the_find_field_then_the_diff_before_the_leave_prompt() {
        let diff = Screen::Diff;
        let open = Ladder {
            find: true,
            leave_prompt: true,
            ..ladder(&diff)
        };
        assert_eq!(escape_step(&open), Escape::Find);
        let found = Ladder {
            find: false,
            ..open
        };
        assert_eq!(escape_step(&found), Escape::Diff);
    }

    #[test]
    fn esc_on_the_board_clears_the_filter_then_the_selection() {
        let board = Screen::Board;
        let filtered = Ladder {
            filter: true,
            ..ladder(&board)
        };
        assert_eq!(escape_step(&filtered), Escape::Filter);
        assert_eq!(escape_step(&ladder(&board)), Escape::Selection);
    }

    #[test]
    fn esc_in_an_issue_steps_from_the_editor_to_the_reader_to_the_board() {
        let edit = Screen::Detail(path(), DetailMode::Edit);
        let prompted = Ladder {
            leave_prompt: true,
            ..ladder(&edit)
        };
        assert_eq!(escape_step(&prompted), Escape::LeavePrompt);
        assert_eq!(escape_step(&ladder(&edit)), Escape::Preview);
        let read = Screen::Detail(path(), DetailMode::Preview);
        assert_eq!(escape_step(&ladder(&read)), Escape::Board);
    }
}
