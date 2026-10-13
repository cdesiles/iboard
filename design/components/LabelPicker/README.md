# LabelPicker

A small popover that adds labels to one issue without opening the Editor. It opens under its trigger, the card's tag-plus action or the Reader's **Add label** button (which turns `Open` while the picker shows), lists the labels already on the board, most used first, and stays open for the next label. Done issues have no trigger: their labels are settled.

## Anatomy

A popover frame: 264pt wide, `surface` on a `line-strong` edge, `radius-md`, `shadow-popover`, 8pt inside, rows 6pt apart. Right-aligned with its trigger and 4pt below it (`space-2`); flipped above when it would leave the window, centred when the trigger has scrolled away.

1. **Field.** A mono 12pt text field, "Add a label" as its placeholder, focused on open.
2. **Hint line.** `meta` in `ink-muted`, the label itself in `ink` at the `ui-strong` weight. It says what `↩` will do.
3. **Group head.** "ON THIS BOARD" in `ui-label`, `ink-muted`, then the count of labels shown in `meta`.
4. **Labels.** The board's labels as 18pt Label pills, 4pt apart, wrapping, scrolling after five rows (106pt). Typing filters them.
5. **Key hints.** `↩ add`, `↓ pick`, `esc close`, 12pt apart (`space-4`).

## States

| State | Hint line | Labels |
| --- | --- | --- |
| Empty | "Type a new label, or pick one below" | every label on the board |
| Typing a label the board has | "↩ adds **bug**" | narrowed to the names containing the text |
| Typing a new label | "↩ adds **perf** · new on this board" | narrowed; none may match |
| Typing a label the issue has | "**bug** is already on this issue" | narrowed |
| Just added | "Added **ux** to todo/ai-assist.md" | the label turns on, its edge flashes |
| Just removed | "Removed **ux** from todo/ai-assist.md" | the label turns off |
| Write failed | the error, in `danger` | unchanged |
| No labels on the board | "Type a new label, or pick one below" | "No labels on this board yet. Type one above." in `ui-small` |

A label **off** the issue is the ordinary Label pill: its colour as a soft tint, or `ink-muted` on `surface-sunken` without one. Clicking it adds it ("Add bug" tooltip). A label **on** the issue is drawn on `surface` with a `line` edge, a `status-done` check and its name in `ink-muted`; clicking it takes it off ("Remove bug"). Just added or removed, its edge is `status-done` for 1.2s.

## Tokens used

`surface`, `surface-sunken`, `line`, `line-strong`, `ink`, `ink-muted`, `danger`, `status-done`, `accent` (focus ring), `radius-md`, `radius-pill`, `space-2`, `space-3`, `space-4`, `shadow-popover`, and the label preset colours of Settings › Labels.

## Keyboard and accessibility

- `↩` in the field adds the typed label (lowercased, spaces as hyphens, at most 25 characters) and keeps the cursor in the field for the next one.
- `↓` leaves the field for the first pill; `tab` and `⇧tab` move between pills, `space` or `↩` toggles the focused one. A focused pill takes the 2pt `accent` ring outside its edge.
- Typing while a pill has focus goes back to the field with the typed text.
- `esc`, or a click outside the popover and its trigger, closes it and drops the field's text.
- Each pill is a button whose label is its name; the tooltip says what a click does ("Add bug", "Remove bug").

Adding or removing rewrites the title's `[label]` prefix in the file (see Label), nothing else. Emits LabelPickerOpened, LabelAdded, LabelRemoved, LabelPickerClosed.
