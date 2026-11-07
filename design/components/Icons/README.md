# Icons

The line icons iboard draws on the 24pt grid: 14pt in buttons and card actions, 18pt in the Delete and Archive drop zones, always `currentColor` with round caps and joins.

- **Stroke:** 1.6pt in buttons and drop zones, 1.5pt in the 22×22 card actions.
- **Shape language:** rounded corners and round ends; nothing meets at a hard square corner unless the object is square (archive box).
- **Trash is iboard's own.** A soft bin: a 15-wide lid, a flat-topped knob with rounded corners instead of a square tab, a body that tapers 0.65pt per side, and two long ribs. It replaces both earlier bins (Lucide's straight-sided `trash-2` in the header Delete button, and the rib-less tapered bin elsewhere), so every delete now uses the same glyph.
- **Release is a flag.** A banner on a pole, notched once at its free end: Done's release control. It is not the label tag, which always carries its small plus.
- **Bell is the top bar's one notification mark.** A dome with a flared foot and the clapper under it; `ink-muted` at rest, unchanged under the pointer. It opens the notification list, never the first row.
- **Agents is a pair of people.** Lucide's `users`: one head and shoulders, another head and a shoulder behind them. It opens Doing's build-agent count, never the count itself.
- **Colour follows the control:** `ink-muted` at rest, `danger` on the hovered Delete icon button (header) and ConfirmDialog, the Delete zone and a card's Delete on hover, `on-danger` on the filled ConfirmDialog button.

```html
<svg class="ib-ico" viewBox="0 0 24 24" aria-hidden="true"><path d="M4.5 7h15"/><path d="M9.4 7V4.9a.9.9 0 0 1 .9-.9h3.4a.9.9 0 0 1 .9.9V7"/><path d="M6.25 7l.65 14h10.2l.65-14"/><path d="M10 9.6v8.3M14 9.6v8.3"/></svg>
<svg class="ib-ico" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 22V3h14l-4 5 4 5H5"/></svg>
```

The consumer provides the size class (`ib-ico`, `ib-ico--lg`) and the colour through `color`.
