# TicketCard

A ticket on the board: the file's first heading as title, its first paragraph as summary, its file name as meta.

- `surface` fill, `line` border, `radius-md`, padding 10×12, `space-2` between rows.
- Title `ui-strong` `ink`, one or two lines. Summary `ui-small` `ink-muted`, clamped to three lines. Meta `meta` `ink-muted`.
- Hover: border to `line-strong`. Selected (keyboard or single click): `accent` 2pt edge on `accent-soft`.
- Click or `↩` opens the Reader. A pill in the meta row shows Unsaved or, later, a pending review.
- The meta row ends with ghost icon buttons: a trash on every card, and an archive on Done cards. They are their own click targets, so pressing one never opens the ticket; `⌘`-free `Del` on the selected card is a shortcut for the trash.
- No drop shadow, no colored side stripe, no avatar or tags until the files carry that data.
