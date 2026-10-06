# ColumnFocus

One column expanded to the whole window, to read more of each issue and sort them differently. You open it from the column title and close it the same way.

## Opening and closing

- **Column titles are buttons.** On hover the title gets a `surface-hover` fill and a small expand icon in `ink-muted`. The tooltip says "Show Todo full screen ⌘1". `⌘1` to `⌘4` open Todo, Ready, Doing and Done.
- **Closing:** click the active column tab again, press the bordered **Board esc** button at the right of the focus bar, or press `esc`. If a filter is on, the first `esc` clears the filter, as on the board, and the second one goes back. The board comes back with the same card selected.
- Opening an issue from the focused column and pressing `esc` in the Reader returns to the focused column, not the board.

## Layout

- The **header bar** stays the same. The breadcrumb reads `iboard / Todo`, and the filter, Refresh and New issue keep working.
- A **focus bar** under it, 12×16 padding, with a `line` rule:
  - **Column tabs** on the left: all four columns in a segmented track. Each tab has its status dot, its name in `ui-label` and its count. The active tab sits on `surface` with a 2pt underline in its status colour. Clicking another tab switches column without going back to the board.
  - **Sort** select, a **Group by label** checkbox, then the **Board esc** button.
- **Cards** flow in a grid of 300pt-minimum columns, 12pt apart: four across on a 14" MacBook Pro and three on a smaller window.
- The **status bar** says what is shown ("todo/ · 6 issues · sorted by recently modified") and the keys: `↩` open, `⌘1–4` column, `esc` board.

## Wide cards

The same IssueCard with more room (padding 12×14, 6pt gaps):

- Labels, title, then the summary clamped at **6 lines** instead of 3.
- A **facts row** in `meta`: each standard section that exists, with a `status-done` check (Context), the checklist count (`Criteria 2/5`), and missing sections as "Out of scope —" in `ink-muted`. A bare issue shows "No sections yet".
- The meta row always shows the modified date (`today 18:02`, `yesterday`, `2 Oct`), not only in Done.
- Hover actions, selection, drag and DropZones behave as on the board. While dragging, the column tabs are drop targets, so a card can move to another column without leaving the view.

## Ordering

Each column opens in its own default, chosen by what people look for there. The choice is remembered per column for the session.

| Column | Default sort | Why |
| --- | --- | --- |
| Todo | Recently modified | Fresh ideas first |
| Ready | Board order (`001-` priority) | The queue order is the point |
| Doing | Recently modified | What moved last |
| Done | Recently finished | What just shipped |

The other options are Board order, Recently modified, Recently created, Title A to Z, and Most to do (open checklist items, descending). Every sort only changes the view and never renames a file. Reordering by drag is offered only in Board order, where it renumbers files exactly as on the board. In other sorts the drop line does not appear.

**Group by label** splits the grid into sections, one per label, ordered by label count. Each section has its label tag and count as a heading with a `line` rule. An issue with two labels appears under both, and unlabelled issues come last.

The consumer provides the column's issues with their parsed sections, checkbox counts and timestamps. Emits ColumnFocused, ColumnSortChanged, ColumnGroupingToggled, ColumnUnfocused.
