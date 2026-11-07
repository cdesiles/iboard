# Kbd

A key cap: `meta-mono` (JetBrains Mono Medium 11/14) in `ink-muted` on a `surface-sunken` fill, `radius-sm`, padding 1×4. No edge: a key cap is a hint, not a control.

- Use the macOS glyphs: ⌘ ⌥ ⇧ ⌃ ↩ ⌫ ⇥ and the arrows, and the word `esc`. JetBrains Mono draws all of them; never add a symbol it lacks.
- **Hidden at rest.** Holding ⌘ alone for `hint-hold` 0.5s slides hints out over `motion-hint` 120ms: beside buttons (plain mono text after the label, no cap), in the filter field, in modal and popup footers, and in the status bar (at most three). The **Shortcuts** card lists the whole screen's keys over it. Letting go of ⌘, or pressing any key with it, hides them in one frame.
- **Always shown**: in tooltips, in menus (right-aligned, `ink-faint`, no cap fill), and on the RefinePanel's numbered choices, whose number is their name.
- Every preview shows the rest state; the "Board, holding ⌘" card shows the revealed one.
