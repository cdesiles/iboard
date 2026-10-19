# HeaderBar

The one top bar: 40pt tall, `bg` ground, a `line` rule below. It holds the breadcrumb on the left and the screen's controls on the right.

- Board: project name in `ui-title`, then the issues path in `meta` `ink-muted`. Right side: FilterField (220pt, 480pt while in use, growing to the left), Auto, Refresh, then New issue (primary, `⌘N`). The rightmost slot always holds the screen's main action: New issue on the Board, Review on the Reader and Editor.
- Reader and Editor: a **Board** back button first, then a 16pt `line-strong` divider, then the breadcrumb `project / Column / Issue title`. The back button is a normal secondary button, never ghost: `surface` fill, `line-strong` edge, `ink` chevron and label, and its key `esc` in `ink-muted` (it is the most used control on these screens, so it must read as a button at a glance). In the Editor, `esc` asks first when there are unsaved changes.
- Breadcrumb links are `ink-muted`, 24pt tall hit areas, with a `surface-hover` fill and an underline on hover; Every segment but the last is a link; the last is the issue title without its `[label]` prefix, `ui-strong` in `ink`, and truncates with an ellipsis. Right side, left to right: Delete (not on Done issues; danger outline, `Del` on the Reader; no key hint in the Editor, where `Del` edits text), Refresh, Edit (`E`; Save and Discard replace it in the Editor), then Refine (`⌘R`, with the reviews in its menu, `⌘⇧R`) as the split button at the edge. Every button there is labelled, so none is an icon alone.
- Keep the bar the same height on every screen so nothing jumps when you open an issue.
- The issue count moves to the status bar; it does not need its own line.
