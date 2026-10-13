# Column

One column per folder: `todo/`, `ready/`, `doing/`, `done/`. Equal widths between `column-min` and `column-max`, `space-5` apart.

- Header: 8pt status dot, uppercase title in `ui-label`, count in `meta` right-aligned, and a 2pt rule in the status color. Color plus word, never color alone.
- The title is a button: on hover it gets a `surface-hover` fill and an expand icon, and a click (or `⌘1` to `⌘4`) opens that column full screen (see ColumnFocus). A double-click on the header or the column's empty space does the same.
- The Todo header alone ends with a ghost `+` icon button that opens the QuickAdd composer (same as `⌘N`). Ready, Doing and Done have none: new issues always start in Todo.
- Hovering the Done header swaps its count for its controls: an `archive` icon button that moves every issue it lists (after the filter) to `.archive/`.
- Cards stack with `space-3` gaps. Todo and Ready follow the file names (Ready's `001-` prefix is its priority); Doing and Done put the most recently edited first.
- Dragging a card to a position in Todo or Ready renumbers that column's files (`001-`, `002-`, ...) so the file order matches the board. A 2pt `accent` line shows where the card will land.
- Empty column: a dashed `line-strong` well on `surface-sunken` saying "No issues in doing/". It doubles as the drop target when dragging.
- Done cards keep full contrast. Do not grey out finished work; the column already says it is done.
