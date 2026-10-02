# Reader

The rendered markdown of one ticket, on a `surface` page up to `reader-max` wide, centred in the window.

- Title is the file's first `#` heading in `read-h1`; the rest is `read-body` at 68ch max.
- The whole description is one edit target. Hover shows a `line-strong` outline and the hint "Click to edit E" in the top-right corner; click enters the Editor with the cursor at the clicked line.
- Links and code use `accent` and `surface-sunken`. Render checkboxes (`- [ ]`) as real checkboxes but read-only.
- `esc` or `⌘[` returns to the board with the same card selected.
