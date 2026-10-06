# ConfirmDialog

A small modal, centred in the window over a dimmed board, that asks before an action that cannot be undone from iboard. Today that is only Delete.

- **It can be turned off.** Settings › General › "Ask before deleting an issue" is on by default. When it is off, Delete skips this dialog and moves the file to the Trash at once, with `⌘Z` undo in the status bar (see GeneralSettings).

- **Replaces the header-bar confirmation.** A question at the top of the window is easy to miss and hard to read; this one sits where the eye already is and blocks everything else until answered.
- **Layout:** 360pt wide on `surface`, `line-strong` edge, `radius-md`, `shadow-popover`, 20pt padding. The window behind it is covered by `scrim`.
- **Content, top to bottom:** a question title in `ui-title` ("Delete this issue?"), the subject in a `surface-sunken` box (issue title in `ui-strong`, file path in `meta`), one sentence on the consequence in `ui-small` `ink-muted` ("The file is moved to the macOS Trash. You can restore it from there."), then the buttons.
- **Buttons:** Cancel (secondary) then Delete (filled `danger`, `on-danger` label, trash icon). The verb on the button repeats the action; never "OK" or "Yes".
- **Keys:** `↩` confirms Delete, `esc` cancels. Focus starts on Delete so `Del` then `↩` is a fast two-key delete, and the dialog itself is the safety step. Clicking the scrim cancels.
- **Opened by:** the card's trash button, `Del` on the selected card, a drop on the Delete zone, and the Editor's trash.
- After deleting, the card leaves its column and the status bar says "Deleted sidebar-collapse-toggle.md".
- Archive never asks: it only moves the file to `.archive/`.

The consumer provides the title, the subject (issue title and path), the consequence sentence and the confirm handler. Emits DeleteConfirmRequested, IssueDeleted, DeleteCancelled.
