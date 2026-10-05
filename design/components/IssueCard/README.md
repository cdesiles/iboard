# IssueCard

An issue on the board: the file's first heading as title, its first paragraph as summary, its file name as meta.

- `surface` fill, `line` border, `radius-md`, padding 10×12, `space-2` between rows.
- Title `ui-strong` `ink`, one or two lines. Summary `ui-small` `ink-muted`, clamped to three lines. Meta `meta` `ink-muted`.
- **Label.** An issue whose `# H1` starts with `[name]` (`# [Discovery] Hey look!`) shows an 18pt chip at the right of the title's first line; the title drops the token and wraps in what is left, so a long title never shifts the chip. The chip is the label's preset colour as a soft tint behind the lowercased name (see the palette below); a colour too faint on the tint falls back to `ink`, and an unknown label is neutral (`surface-sunken` on `ink-muted`). One label per issue; a bracket elsewhere in the heading is not a label, and the raw file is never rewritten for it.
- Hover: border to `line-strong`. Selected (keyboard or single click): `accent` 2pt edge on `accent-soft`.
- Click or `↩` opens the Reader. A pill in the meta row shows Unsaved or, later, a pending review.
- Meta row, left to right in `meta` `ink-muted`: file name (takes the remaining width, truncates with an ellipsis), optional pill, date (`YYYY-MM-DD`, Done only), then the action slot.
- **Actions appear on hover only.** Trash on every card, Archive before it on Done cards. They are hidden at rest and shown when the pointer is over the card, on the selected card, and when the card has keyboard focus. Their 46pt slot is always reserved, so the file name and date never move when they appear.
- Each action is a 22×22 icon button with a 14pt line icon at 1.5pt stroke, `ink-muted`, no border. On its own hover it gets `surface-hover` and a `line-strong` edge; Delete turns `danger` only then. Tooltips name the key: "Archive  A", "Delete  Del".
- Actions are their own click targets, so pressing one never opens the issue. Delete always goes through ConfirmDialog; Archive acts at once (it only moves the file to `.archive/`).
- Dragging a card opens the DropZones dock as a second way to archive or delete. The card being dragged stays in place at 40% opacity; a lifted copy follows the pointer.
- No drop shadow, no colored side stripe, no avatar. The only tag is the one label the file itself carries in its `# H1`.

## Labels

The label is a user preference, not project data. iboard ships a preset, and the `[labels]` table of `~/.config/iboard/iboard.toml` overrides or adds a `name = "#rrggbb"` mapping (a per-project `iboard.toml` may override a name later):

| Label | Colour |
| --- | --- |
| `discovery` | `#0b6076` |
| `bug` | `#b12f22` |
| `feature` | `#1a6e42` |
| `chore` | `#5b6472` |
| `question` | `#8a5006` |
