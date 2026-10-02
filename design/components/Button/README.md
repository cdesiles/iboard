# Button

Buttons are 24pt-tall verbs with a `line-strong` edge, so they stay visible while taking little room.

- Secondary (default): `surface` fill, `line-strong` border, `ink` label. Use for Refresh, Discard, Back.
- Primary: `accent` fill, `on-accent` label. One per screen at most: Save in the editor.
- Ghost: no border, `ink-muted` label. Only for navigation (back to Board) and icon buttons in a row of other buttons.
- Icon-only buttons are 24×24 and need an accessible name and a tooltip with the shortcut.
- Label in `ui-strong` at 12pt; icon 14pt line, 6pt gap.
- Disabled keeps the shape but drops to `line` and `ink-faint`; never hide a button that will become available.

The consumer provides the label, an optional icon and the shortcut for the tooltip.
