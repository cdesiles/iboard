# Icons

The line icons iboard draws on the 24pt grid: 14pt in buttons and card actions, 18pt in the Delete and Archive drop zones, always `currentColor` with round caps and joins.

- **Stroke:** 1.6pt in buttons and drop zones, 1.5pt in the 22×22 card actions.
- **Shape language:** rounded corners and round ends; nothing meets at a hard square corner unless the object is square (archive box).
- **Trash is iboard's own.** A soft bin: a 15-wide lid, a round knob (a 2.5 radius half circle) instead of a square tab, a body that tapers 1pt per side down to 3pt rounded bottom corners, and two short ribs. It replaces both earlier bins (Lucide's straight-sided `trash-2` in the header Delete button, and the rib-less tapered bin elsewhere), so every delete now uses the same glyph.
- **Release is a flag.** A banner on a pole, notched once at its free end: Done's release control. It is not the label tag, which always carries its small plus.
- **Agents is a pair of people.** Lucide's `users`: one head and shoulders, another head and a shoulder behind them. It opens Doing's build-agent count, never the count itself.
- **Colour follows the control:** `ink-muted` at rest, `danger` on the hovered Delete icon button (header) and ConfirmDialog, the Delete zone and a card's Delete on hover, `on-danger` on the filled ConfirmDialog button.

```html
<svg class="ib-ico" viewBox="0 0 24 24" aria-hidden="true"><path d="M4.5 7h15"/><path d="M9.5 7a2.5 2.5 0 0 1 5 0"/><path d="M6.5 7l.9 11.1a3 3 0 0 0 3 2.9h3.2a3 3 0 0 0 3-2.9L17.5 7"/><path d="M10 11v5.5M14 11v5.5"/></svg>
<svg class="ib-ico" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 22V3h14l-4 5 4 5H5"/></svg>
```

The consumer provides the size class (`ib-ico`, `ib-ico--lg`) and the colour through `color`.
