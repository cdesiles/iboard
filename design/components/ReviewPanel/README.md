# ReviewPanel

The result of a review: a 360pt panel beside the Reader or Editor page with three findings in a fixed order.

- Header on `review-soft` names the file and the agent. Findings: Relevance, Size, Clarity, each with a verdict pill (Pass, Challenge, Block) and one or two sentences, then concrete items (a proposed split, the questions an implementer would ask).
- When the ticket is only a title, the panel instead proposes the missing sections (Context, Acceptance criteria, Out of scope, Open questions).
- Apply to draft opens the Editor with the proposal merged into the draft, unsaved. The agent never writes the file.
- Ask again sends a follow-up ("make it smaller", "assume CLI only"). Dismiss closes the panel; the last review stays reachable from the card's pill.
