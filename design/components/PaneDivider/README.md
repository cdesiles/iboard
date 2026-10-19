# PaneDivider

The resize handle between two panes side by side (Reader and Agent log, Reader and Review panel, the two panes of RefinePanel). The whole gutter between the panes is the handle, so there is nothing to aim at, and it never shares pixels with a pane's scrollbar.

## Why this shape

The previous handle was a 8×60 pill on a gutter framed by two rules. Next to the left pane's scrollbar thumb it read as a second scrollbar, the two rules added chrome to a window that should stay quiet, and the pill was the only grabbable part.

- **The gutter is the target.** The 16pt (`space-5`) gutter that already separates panes takes the pointer over its full height: a 16pt strip is easier to hit than any pill, and costs no extra space.
- **Dots, not a pill.** At rest the only mark is a 2×3 dot grip in `ink-faint`, vertically centred. Dots read as "grab here" and cannot be mistaken for the Scrollbar's pill thumb.
- **Separate zones.** The pane's scroll strip (`scroll-hit`, 12pt) stays inside the pane's edge; the divider starts outside it. Pointing at the thumb scrolls, pointing at the gutter resizes, and the cursor says which.

## States

| State | Look |
| --- | --- |
| Rest | No line. Dot grip in `ink-faint` on `bg`. |
| Pointer over | After 150ms (so passing through doesn't flash), a 2pt `line-strong` rule runs the full height down the gutter's centre; the grip's dots turn `ink-muted` and sit on a gap in the rule. Cursor: col-resize. |
| Dragging | Rule and dots in `accent`. A `meta` readout above the grip shows both pane widths ("612 · 404"). The cursor keeps col-resize even when it leaves the gutter. |
| Keyboard focus | `tab` reaches the divider after the left pane's content: 2pt `focus` ring around the grip, `accent` rule, readout shown. |
| Inline | Two panes inside one card (RefinePanel): the 1pt `line` divider is the handle, with a 9pt hit area centred on it (4pt each side). On hover it thickens to 2pt `line-strong` and the grip sits on `surface`. The left pane's scroll strip steps 4pt in so the two never overlap. |
| Too narrow | When the window can't fit both minimums, the panes stack (as RefinePanel does under 820pt) and there is no divider. |

## Behaviour

- Each pane keeps at least `column-min` (280pt); a drag stops there rather than collapsing a pane. Pane content never reflows mid-drag beyond the width change itself.
- Within 8pt of the default split, the drag snaps to it.
- Keys when focused: `← →` move 16pt, `⇧← ⇧→` move 80pt, `↩` resets to the default. A double-click on the gutter resets too.
- The split is remembered per pair of panes (for example Reader + Agent log) in iboard's view state, not in any issue file.
- Width changes are instant; only the hover rule fades in (120ms after the 150ms delay), and not at all with reduced motion.

| Command (user) | Event | Next state |
| --- | --- | --- |
| Drag the gutter, `← →` on the focused divider | PaneSplitChanged | Same screen, panes resized; split remembered |
| Double-click, `↩` on the focused divider | PaneSplitReset | Default split |

The consumer provides the two panes, their minimum widths and the default split. In egui, `theme::pane_divider` in `theme.rs` paints and handles it for a gutter `Rect`.
