# Column

One column per folder: `todo/`, `ready/`, `doing/`, `done/`. Equal widths between `column-min` and `column-max`, `space-5` apart.

- **Header**, 24pt: a 14pt status icon (`status-icon`), the title in title case in `ui-strong` `ink` ("Todo"), and the count right-aligned in `meta` (Inter) `ink-muted`. No rule under it. The count is the only issue count on the board; the status bar does not repeat a total.
- **Status icon**, in the status colour, a different shape per column so colour is never the only signal (`theme::status_icon`):
  - Todo: an empty circle, r 5.25, 1.5pt stroke (`status-todo`).
  - Ready: a dotted circle, twelve 1.75pt dots on r 5.25 (`status-ready`).
  - Doing: the empty circle with its right half filled inside, r 3.25 (`status-doing`).
  - Done: a filled circle r 6 with a 1.5pt check in `surface` (`status-done`).
- The Column card shows the title case header next to today's uppercase `ui-label` one for comparison; title case is the proposal.
- Hovering a header swaps its count for its column actions, right-aligned ghost icon buttons that fade in over `motion-hover`: Todo's `+`, Ready's `play`, Doing's agents or Done's `archive`. At rest only the count shows.
- A column holding picked cards (see IssueCard) shows a `close` icon button right after its title, "Clear selection"; `esc` does the same.
- Todo's `+` column action opens the NewIssueModal (same as `⌘N`). Ready, Doing and Done have none: new issues always start in Todo.
- Ready's `play` column action hands its issues (after the filter) to the AI build agent (Settings › AI build), one at a time from top to bottom: each moves to the next column, then the agent implements it from the project folder, following the project's own instructions. The status bar says "Building 001-x.md (1/3) · <the agent's step>" in `accent`. While it runs, `play` becomes a `stop` square that stays in Ready's header even at rest; stopping leaves the issue where it is. A failing agent stops the run and names the issue in the error strip. Without a build agent set up, `play` opens Settings › AI build.
- Doing's hovered header shows its build agents as a stepper: `−`, "2 agents" in `meta` `ink-muted`, `+`. From 0 to 16, 1 by default. Each agent takes the next Ready issue of the run, so 2 agents build 2 issues at once; adding one mid-run starts it at once, removing one lets the agents at work finish. 0 stops the AI build; `play` then starts it again with one agent. A card's `stop` stops only its agent.
- Done's controls end with an `archive` icon button that moves every issue it lists (after the filter) to `.archive/`.
- Cards stack with `space-3` gaps. Todo and Ready follow the file names (Ready's `001-` prefix is its priority); Doing and Done put the most recently edited first.
- Dragging a card to a position in Todo or Ready renumbers that column's files (`001-`, `002-`, ...) so the file order matches the board. A 2pt `accent` line shows where the card will land.
- **Empty column**: a dashed 1pt `line` well, no fill, `radius-md` corners like a card, 12pt padding, saying "No issues in doing/" centred in 12pt `ink-faint`. It doubles as the drop target: while a card is over it, solid `accent` on `accent-soft`. In egui, paint the dashes along a rounded path (`theme::radius::MD`); dashing a plain rect gives square corners.
- Done cards keep full contrast. Do not grey out finished work; the column already says it is done.
