# IssueCard

An issue on the board: its labels, the file's first heading as title, its first paragraph as summary, its file name as meta.

- `surface` fill, `line` border, `radius-md`, padding 10×12, `space-2` between rows.
- Title `ui-strong` `ink`, one or two lines. Summary `ui-small` `ink-muted`, clamped to three lines. Meta `meta` `ink-muted`.
- Hover: border to `line-strong`. Selected (keyboard or single click): `accent` 2pt edge on `accent-soft`.
- A click selects the card, and only it; it never opens it. `↩`, a double click or **Expand** opens the Reader.
- **Expand** sits in the card's top right corner, 6pt in, over the card's fill, and shows on hover like the other actions: "Expand  ↩", with the outward arrows icon. With a multi-selection only the card with focus offers it, and `↩` opens the first picked card in board order.
- Shift-click picks the card into a multi-selection without opening it; shift-clicking a picked card takes it out. Picked cards share the selected look. An action on a picked card (Delete, Archive, Add label) applies to every picked card, and dragging one carries them all, landing together in board order. Delete asks once for all of them. A pill in the meta row shows Unsaved, or the last review's verdict, which a click reopens.
- Meta row, left to right in `meta` `ink-muted`: file name (takes the remaining width, truncates with an ellipsis), optional pill, date (`YYYY-MM-DD`, Done only), then the action slot.
- **Actions appear on hover only.** Left to right: Add label (it opens the LabelPicker under it), Review on the selected Todo card once an agent is set up, then Delete. Done cards offer Show the diff and Archive instead: done issues are archived, never deleted, and keep the labels they have. They are hidden at rest and shown when the pointer is over the card, on the selected card, and when the card has keyboard focus. Their slot is always reserved, 70pt, so the file name and date never move when they appear.
- Each action is a 22×22 icon button with a 14pt line icon at 1.5pt stroke, `ink-muted`, no border. On its own hover it gets `surface-hover` and a `line-strong` edge; Delete turns `danger` only then. Review uses `review` instead. Tooltips name the key: "Add label  L", "Review  ⌘⇧R", "Show the diff", "Archive  A", "Delete  Del".
- Actions are their own click targets, so pressing one never opens the issue. Delete always goes through ConfirmDialog; Archive acts at once (it only moves the file to `.archive/`). Show the diff opens the Diff screen on every commit that closed the issue (`closes <file>.md`), or says in the status bar that none did.
- Dragging a card opens the DropZones dock as a second way to archive or delete. The card being dragged stays in place at 40% opacity; a lifted copy follows the pointer.
- **Labels** sit on their own row above the title, read from the `[Brackets]` at the start of the title (see Label). The title shows without them. A card with no label has no label row, so it is not taller. Clicking a label filters the board by it and never opens the issue.
- No drop shadow, no colored side stripe, no avatar.
