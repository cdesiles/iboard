# ReviewButton

The entry point to ask an agent (OpenCode, Claude Code or a custom command) to challenge an issue.

- Always the rightmost header control on Reader and Editor; on the Board it appears in the selected card's hover row.
- Outlined in `review` (violet), the only control that uses this hue. Violet reads as "assistant", not as a warning, and stays clearly apart from the blue `accent` and the red Delete. Label "Refine", icon a speech bubble with a question mark, shortcut `⌘R`: refining is the default, reviewing comes second (`⌘⇧R`).
- Running: a review runs in the background, so the board and reader stay usable; the label reads "Reviewing…" and a click (or `⌘⇧R`) cancels it. Once it lands, a "Review ready" button beside it opens the ReviewPanel; a failure shows in the error strip and the last review stays. Disabled when no agent is configured, with a tooltip saying how to configure one.
- In the Editor it refines or reviews the current draft, not the saved file.
- It is a split button: the main part opens the RefinePanel, the caret opens a menu with "Refine with a challenger" (`⌘R`), "Review this issue" (`⌘⇧R`), "Deep review", a "Runs with" line naming the tool and model (`opencode · default`), and "AI settings…" (`⌘,`), which opens ReviewSettings.
