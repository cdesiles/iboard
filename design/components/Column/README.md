# Column

One column per folder: `todo/`, `ready/`, `doing/`, `done/`. Equal widths between `column-min` and `column-max`, `space-5` apart.

- Header: 8pt status dot, uppercase title in `ui-label`, count in `meta` right-aligned, and a 2pt rule in the status color. Color plus word, never color alone.
- The title is a button: on hover it gets a `surface-hover` fill, and a click (or `⌘1` to `⌘4`) opens that column full screen (see ColumnFocus). A double-click on the header or the column's empty space does the same.
- Hovering a header swaps its count for its column actions, right-aligned icon buttons: `expand`, which also opens the column full screen, Todo's `+` and Ready's `play` at the far right. At rest only the count shows.
- A column holding picked cards (see IssueCard) shows a `close` icon button right after its title, "Clear selection"; `esc` does the same.
- Todo's `+` column action opens the QuickAdd composer (same as `⌘N`). Ready, Doing and Done have none: new issues always start in Todo.
- Ready's `play` column action hands its issues (after the filter) to the AI build agent (Settings › AI build), one at a time from top to bottom: each moves to the next column, then the agent implements it from the project folder, following the project's own instructions. The status bar says "Building 001-x.md (1/3) · <the agent's step>" in `accent`. While it runs, `play` becomes a `stop` square that stays in Ready's header even at rest; stopping leaves the issue where it is. A failing agent stops the run and names the issue in the error strip. Without a build agent set up, `play` opens Settings › AI build.
- Doing's header always shows its build agents as a stepper: `−`, "2 agents" in `meta` `ink-muted`, `+`. From 0 to 16, 1 by default. Each agent takes the next Ready issue of the run, so 2 agents build 2 issues at once; adding one mid-run starts it at once, removing one lets the agents at work finish. 0 stops the AI build; `play` then starts it again with one agent. A card's `stop` stops only its agent.
- Done's controls end with an `archive` icon button, right of `expand`, that moves every issue it lists (after the filter) to `.archive/`.
- Cards stack with `space-3` gaps. Todo and Ready follow the file names (Ready's `001-` prefix is its priority); Doing and Done put the most recently edited first.
- Dragging a card to a position in Todo or Ready renumbers that column's files (`001-`, `002-`, ...) so the file order matches the board. A 2pt `accent` line shows where the card will land.
- Empty column: a dashed `line-strong` well on `surface-sunken` saying "No issues in doing/". It doubles as the drop target when dragging.
- Done cards keep full contrast. Do not grey out finished work; the column already says it is done.
