# Reader

The rendered markdown of one issue, laid out as a **sheet**: a `surface` page up to `reader-max` (720pt) wide, centred, that always reaches down to the status bar. A short issue no longer floats as a small box in an empty window. The page is two blocks: the **sheet** holds what is in the file, and a separate **Not written yet** block docked at the bottom holds what is missing. Suggestions never mix with the issue text.

## Layout, top to bottom

1. **Head**, a `meta` strip with a `line` rule under it:
   - On the left, the column as a status mark: an 8pt dot in the column's status colour and the word in `ui-label`, then the file path, which truncates with an ellipsis when space runs out.
   - On the right, "modified 1h ago" and either the word count, or checklist progress when the issue has checkboxes: a 48pt `status-done` meter with "2/5 done".
2. **Labels and title**: the labels from the title's `[Brackets]` as a row of Label tags, then the first `#` heading without them in `read-h1`. Clicking a label goes back to the board filtered by it.
3. **Body** in `read-body`, at most 68ch per line. Links in `accent`, code on `surface-sunken`, `- [ ]` rendered as read-only checkboxes.
4. **The rest of the sheet** stays empty and clickable. At its foot, `ink-faint` reads "Click below the text to keep writing E". A click there opens the Editor with the cursor at the end of the file.

Under the sheet, 12pt apart, comes the **Not written yet** block, only when standard sections are missing (see below). Together the two blocks fill the window minus the header, the status bar and the 16pt gutters: the sheet takes the spare height, so the block sits at the bottom of the window. On a longer issue the page scrolls and the block stays docked at the bottom edge (sticky), always one glance away.

## Missing sections

iboard checks the issue for the standard headings `## Context`, `## Acceptance criteria`, `## Out of scope` and `## Open questions`. The list is configurable: `[reader] sections = [...]`, and an empty list turns this off.

- **The block**: its own `surface` card with a `line` edge and `radius-md`, 12×16 padding. Its head row has "Not written yet" in `ui-label` `ink-muted` on the left and the Review button on the right.
- **No section written yet** (a title and a paragraph): one **ghost tile** per missing section, in a grid of up to four across (two on a narrow window). A ghost tile has a dashed `line-strong` edge, the heading in `ui-strong` `ink-muted` with a `+`, and one short `ui-small` hint saying what belongs there:
  - Context: "Why now, and what to know first?"
  - Acceptance criteria: "What must be true when done?"
  - Out of scope: "What will this deliberately not do?"
  - Open questions: "What do we still need to decide?"
- On hover, a ghost tile turns solid `accent` on `accent-soft`. A click opens the Editor with `## <Section>` and an empty line (`- [ ] ` for Acceptance criteria) appended, with the cursor ready.
- The Review button in the block head reads "Draft with Review ⌘⇧R", using the reserved `review` hue because it calls the agent. It asks Review to propose those sections, and they land in the Editor unsaved, as Apply to draft already does.
- **Some sections written**: the block shrinks to one 40pt line: "Not written yet" followed by `+ Out of scope` and `+ Open questions` chips.
- **Done issues**, or nothing missing: no block at all, and the sheet takes the whole height.

## Behaviour

- The whole body is still one edit target: hovering shows the `line-strong` outline and the hint "Click to edit E"; clicking opens the Editor at the clicked line.
- `esc` or `⌘[` returns to the board with the same card selected.

The consumer provides the parsed markdown, the column, the path, the modified time, the checkbox counts and the list of missing sections. Emits EditStarted, SectionAddRequested, ReviewRequested.
