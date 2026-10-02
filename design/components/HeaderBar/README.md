# HeaderBar

The one top bar: 40pt tall, `bg` ground, a `line` rule below. It holds the breadcrumb on the left and the screen's controls on the right.

- Board: project name in `ui-title`, then the roadmap path in `meta` `ink-muted`. Right side: filter, Auto, Refresh, then New ticket (primary, `⌘N`). The rightmost slot always holds the screen's main action: New ticket on the Board, Review on the Reader and Editor.
- Reader and Editor: breadcrumb `project / Column / Ticket title`. Every segment but the last is a link; the last is `ui-strong` and truncates with an ellipsis. Right side: Refresh, then the Review slot.
- Keep the bar the same height on every screen so nothing jumps when you open a ticket.
- The ticket count moves to the status bar; it does not need its own line.
