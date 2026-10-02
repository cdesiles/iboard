# ReviewButton

The future entry point to ask an agent (OpenCode or a Claude agent) to challenge a ticket. Reserved now so the layout does not move later.

- Always the rightmost header control on Reader and Editor; on the Board it appears in the selected card's hover row.
- Outlined in `review`, the only control that uses this hue. Label "Review", icon a speech bubble with a question mark, shortcut `⌘⇧R`.
- Running: label "Reviewing…", `esc` cancels. Disabled when no agent is configured, with a tooltip saying how to configure one.
- In the Editor it reviews the current draft, not the saved file.
