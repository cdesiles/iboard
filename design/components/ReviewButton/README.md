# ReviewButton

The entry point to ask an agent (OpenCode, Claude Code or a custom command) to challenge an issue.

- Always the rightmost header control on Reader and Editor; on the Board it appears in the selected card's hover row.
- Outlined in `review` (violet), the only control that uses this hue. Violet reads as "assistant", not as a warning, and stays clearly apart from the blue `accent` and the red Delete. Label "Review", icon a speech bubble with a question mark, shortcut `⌘⇧R`.
- Running: label "Reviewing…", `esc` cancels. Disabled when no agent is configured, with a tooltip saying how to configure one.
- In the Editor it reviews the current draft, not the saved file.
- It is a split button: the caret opens a menu with "Review this issue", "Deep review", "Refine with a challenger" (which opens the RefinePanel), a "Runs with" line naming the tool and model (`opencode · default`), and "AI settings…" (`⌘,`), which opens ReviewSettings.
