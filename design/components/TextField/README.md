# TextField

The filter input: 24pt tall, `line-strong` border, `ink` text, `ink-faint` placeholder.

- Placeholder says what it filters: "Filter issues". It matches title, summary and file name.
- Show the `⌘F` hint inside the right edge while empty; hide it once there is text.
- `esc` clears the filter; the board shows "No issues match "review"" per empty column.
- In the board header this field is the FilterField: it widens to 480pt on focus and offers keyword suggestions. See FilterField.
- Focused: the one border turns `focus` and a 3px `focus-halo` glows around it. No outline ring, so the field never shows two borders. The same rule holds for every text entry: textareas, the QuickAdd composer, the Editor, and the FilterField while open (once you leave it with a filter on, it keeps the `accent` border without the halo).
