# Skeleton

Loading blocks shaped like what is coming, for anything that takes more than a moment: stats gathered from git history, charts, lists, and the RefinePanel while an agent answers. The block sits exactly where the content will land, at its final size, so nothing moves when the data arrives.

## Blocks

| Block | While loading | Loaded |
| --- | --- | --- |
| **Stat tile** (`ib-statcard`) | Label shown for real (`ui-label`), a 22pt value bar, an 8pt meta bar | Value in 24/30 600 tabular figures, meta in `meta` |
| **Bar chart** (`ib-skel-bars`) | Grid lines and baseline in `line` drawn at once; 12 bars of mixed heights in `surface-sunken`; axis labels as short bars | Bars in the series colour, axis labels in `meta` |
| **Line chart** (`ib-skel-line`) | Same grid; one soft 4pt curve in `surface-sunken` | The line in the series colour |
| **List** (`ib-skel-rows`) | 5 rows of 28pt: dot, name bar, count bar, `line` dividers | Label pills and counts |
| **Text** (`ib-skel`) | 8pt bars (12pt for a title), widths 60–98% so it reads as prose | Text |

- Everything that is already known shows for real: card labels, axes, grid lines, a column's name. Only the data waits.
- Placeholder colour is `surface-sunken` on `surface`; no borders, no icons, no fake numbers.
- Placeholder heights are fixed by the block, never by a guess at the data.

## Motion

All placeholders on screen pulse together, opacity 1 → 0.5 → 1 over 1.6s, the same rhythm as the Thinking mark. No shimmer or sweep: a sweep draws the eye across the whole window, a shared pulse just says "not yet". Reduced motion: no pulse.

## Timing

- **0–300ms**: nothing changes. Fast results never flash a placeholder.
- **From 300ms**: the skeleton.
- **From 2s**: the block head shows the small Thinking i and an elapsed clock in `meta` ("3s"), right-aligned.
- **Each block on its own**: blocks fill in as their data arrives, in any order. One slow stat never holds the others back.
- **Failure**: the block keeps its size, says what failed in `danger` ("Couldn't read git history for done/"), and offers Retry. Its neighbours are unaffected.
- **Refresh** (`⌘R`) keeps the old numbers visible with a 60% opacity until the new ones land, instead of going back to placeholders.

## Accessibility

A loading block carries `aria-busy="true"`; the page announces "Loading stats" once, not each block. Placeholders are hidden from screen readers.

The consumer provides each block's label and a future per block. In egui, `theme::skeleton_fill` returns the pulsing placeholder colour to paint with.
