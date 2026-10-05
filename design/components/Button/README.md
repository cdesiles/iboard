# Button

Buttons are 24pt-tall verbs with a `line-strong` edge, so they stay visible while taking little room.

- Secondary (default): `surface` fill, `line-strong` border, `ink` label. Use for Refresh, Discard, Back.
- Primary: `accent` fill, `on-accent` label. One per screen at most: Save in the editor.
- Danger outline: `surface` fill, `danger` border and label, `danger-soft` on hover. For Delete in the Reader and Editor header: trash icon, "Delete", key `Del`. It always opens the ConfirmDialog, so the filled Danger button is kept for that dialog's final "Delete".
- Danger: `danger` fill, `on-danger` label. Only as the confirm button inside ConfirmDialog.
- Back: a secondary button with a leading chevron, "Board" and its key `esc` in `ink-muted`. Always bordered; navigation that people use constantly must not hide.
- Ghost: no border, `ink-muted` label. Only for icon buttons inside a row of other buttons (dialog close, column `+`), never for navigation.
- A key hint sits after the label in mono 11pt, in the label colour (`Edit E`, `Delete Del`).
- Icon-only buttons are 24×24 and need an accessible name and a tooltip with the shortcut.
- Label in `ui-strong` at 12pt; icon 14pt line, 6pt gap.
- Disabled keeps the shape but drops to `line` and `ink-faint`; never hide a button that will become available.

The consumer provides the label, an optional icon and the shortcut for the tooltip.
