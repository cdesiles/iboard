# RefinePanel

Backlog refinement: a conversation with an agent whose sole role is to be a product challenger, until the issue is a clear spec worth building, or plainly not worth it.

- Opened from the ReviewButton's menu, "Refine with a challenger", on a Todo issue. It is a popin over the Reader or Editor page: a modal of the popover frame, up to 720pt wide and 80% of the window high, on the `scrim`. Opening a review closes it, and the other way round.
- Header on `review-soft`: "Refining x.md", and the agent with `challenger`.
- The challenger speaks first. It asks whether a feature really adds value and for whom, whether a bug is worth fixing, what a smaller way would be, what is out of scope and how done is known: at most three points a turn, and it says when the issue should be dropped.
- Each turn sits under its speaker: "Challenger" in `review`, rendered as markdown, "You" in `ink-muted`. While the challenger thinks, the elapsed time and its step show.
- The reply field below takes the author's answer; Send (`⌘↩`) hands it, the issue as it reads now (the Editor draft when open) and the conversation so far to the agent.
- Once the need is settled the challenger proposes the refined issue. Apply to draft opens the Editor with it, unsaved, and tucks the popin away so the draft can be read. The agent never writes the file.
- Hide, or a click on the scrim, tucks the popin away too, even while the challenger thinks: a Review-kind pill, "Back to the refinement" ("Refining…" while a turn runs), floats at the foot of the window and brings it back, as "Refine with a challenger" does.
- Stop (`esc`) halts a turn and gives the last answer back to edit. `esc` with no turn running closes the popin; Dismiss too. The conversation is not kept.
