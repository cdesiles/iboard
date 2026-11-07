//! The RefinePanel's keys: choosing, moving between choices and the note,
//! and sending.

use super::{Key, RefineAction, Refinement, note_id};

pub(super) const NUMBERS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

/// The panel's keys, before its widgets read the input: `1`–`9` pick or
/// unpick a choice, `↑` `↓` move between them and `space` picks, `tab` goes
/// to the note and `⇧tab` back. `⌘↩` sends from anywhere.
pub(super) fn refine_keys(ui: &mut egui::Ui, refine: &mut Refinement) -> Option<RefineAction> {
    if refine.is_running() {
        return None;
    }
    let in_note = ui.memory(|m| m.has_focus(note_id()));
    let elsewhere = !in_note && ui.memory(|m| m.focused().is_some());
    let count = refine.options().len();
    let none = egui::Modifiers::NONE;
    let (mut to_note, mut from_note) = (false, false);
    let send = ui.input_mut(|i| {
        if i.consume_key(egui::Modifiers::COMMAND, Key::Enter) {
            return true;
        }
        if in_note {
            from_note = i.consume_key(egui::Modifiers::SHIFT, Key::Tab);
            i.consume_key(none, Key::Tab);
            return false;
        }
        if elsewhere {
            return false;
        }
        to_note = i.consume_key(none, Key::Tab);
        if count == 0 {
            return false;
        }
        let last = count - 1;
        if i.consume_key(none, Key::ArrowDown) {
            refine.cursor = Some(refine.cursor.map_or(0, |at| (at + 1).min(last)));
        }
        if i.consume_key(none, Key::ArrowUp) {
            refine.cursor = Some(refine.cursor.map_or(0, |at| at.saturating_sub(1)));
        }
        if let Some(at) = refine.cursor.filter(|_| i.consume_key(none, Key::Space)) {
            refine.toggle_choice(at.min(last));
        }
        if let Some(at) = NUMBERS[..count.min(NUMBERS.len())]
            .iter()
            .position(|key| i.consume_key(none, *key))
        {
            refine.toggle_choice(at);
        }
        false
    });
    if to_note {
        ui.memory_mut(|m| m.request_focus(note_id()));
    }
    if from_note {
        ui.memory_mut(|m| m.surrender_focus(note_id()));
    }
    send.then_some(RefineAction::Send)
}
