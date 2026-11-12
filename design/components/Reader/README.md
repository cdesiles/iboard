# Reader

The rendered markdown of one issue, laid out as a **sheet**: a `surface` page with a `line-soft` edge, `reader-max` (624pt) wide, centred, that always reaches down to the status bar. A short issue no longer floats as a small box in an empty window. The page is two blocks: the **sheet** holds what is in the file, and a separate **Not written yet** block docked at the bottom holds what is missing. Suggestions never mix with the issue text.

## Layout, top to bottom

1. **Labels and title**: the labels from the title's `[Brackets]` as a row of Label pills, then the first `#` heading without them in `read-h1` (20/26 SemiBold). Clicking a label goes back to the board filtered by it. There is no head strip above them any more: the column is in the breadcrumb (with its status icon), and the path, "modified 15d ago" and the word count or checklist progress are in the status bar. Each is shown once.
2. **Body** in `read-body` 15/24, in a `read-measure` 560pt column: about 75 characters per line (measured 70 to 79 on prose). The sheet is that column plus 32pt padding each side. `##` is `read-h2` 16/22 SemiBold with 24pt above and 6 below; `###` and deeper are `read-h3` 14/20 SemiBold with 16 above and 4 below, so sections read as sections without rivalling the title. Links in `accent`, inline code in JetBrains Mono 13 on `surface-sunken`, `- [ ]` rendered as live checkboxes: ticking one writes the file (the status bar's progress follows). In Doing and Done they stay live too, since ticking is not editing the text. List items sit 4pt apart, GitHub's `.25em`, and a bullet, a number or a task box centres on its item's first line.
3. **The rest of the sheet** stays empty. In Todo and Ready, its foot reads "Double-click below the text to keep writing" in `ink-faint` (the `E` cap shows while ⌘ is held), and a double-click there opens the Editor with the cursor at the end of the file.

Under the sheet, 12pt apart, comes the **Not written yet** block, on Todo issues only and only when standard sections are missing (see below). Together the two blocks fill the window minus the header, the status bar and the 16pt gutters: the sheet takes the spare height, so the block sits at the bottom of the window. On a longer issue the page scrolls and the block stays docked at the bottom edge (sticky), always one glance away.

## Missing sections

iboard checks the issue for the standard headings `## Context`, `## Acceptance criteria`, `## Out of scope` and `## Open questions`. The list is configurable: `[reader] sections = [...]`, and an empty list turns this off.

- **The block**: its own `surface` card with a `line-soft` edge and `radius-md`, 12×16 padding. Its head row has "Not written yet" in `ui-label` `ink-muted` on the left and the Review button on the right.
- **No section written yet** (a title and a paragraph): one **ghost tile** per missing section, in a grid of two across. A ghost tile has a dashed `line-strong` edge, the heading in `ui-strong` `ink-muted` with a `+`, and one short `ui-small` hint saying what belongs there:
  - Context: "Why now, and what to know first?"
  - Acceptance criteria: "What must be true when done?"
  - Out of scope: "What will this deliberately not do?"
  - Open questions: "What do we still need to decide?"
- On hover, a ghost tile turns solid `accent` on `accent-soft`. A click opens the Editor with `## <Section>` and an empty line (`- [ ] ` for Acceptance criteria) appended, with the cursor ready.
- The Review button in the block head reads "Draft with Review ⌘⇧R", using the reserved `review` hue because it calls the agent. It asks Review to propose those sections, and they land in the Editor unsaved, as Apply to draft already does.
- **Some sections written**: the block shrinks to one 40pt line: "Not written yet" followed by `+ Out of scope` and `+ Open questions` chips.
- **Ready, Doing and Done issues**, or nothing missing: no block at all, and the sheet takes the whole height. Only Todo issues are still being written.

## Behaviour

- The whole body is still one edit target, in Todo and Ready: hovering shows a `line-strong` outline (fading in over `motion-hover`) and the hint "Double-click to edit" (with its `E` cap while ⌘ is held); a double-click opens the Editor at the clicked line, as `E` does at the top. A single click stays free for the checkboxes, links and labels.
- Doing and Done open read-only: no outline, no hint, no double-click to edit.
- `esc` or `⌘[` returns to the board with the same card selected.

The consumer provides the parsed markdown and the list of missing sections; the column goes to the breadcrumb and the path, modified time and checkbox counts to the status bar. Emits EditStarted, CheckboxToggled, SectionAddRequested, ReviewRequested.
