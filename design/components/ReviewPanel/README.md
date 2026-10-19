# ReviewPanel

The result of a review: a 360pt panel beside the Reader or Editor page with three findings in a fixed order.

- Header on `review-soft` names the file and the agent. Findings: Relevance, Size, Clarity, each with a verdict pill (Pass, Challenge, Block) and one or two sentences, then concrete items (a proposed split, the questions an implementer would ask).
- When the issue is only a title, the panel instead proposes the missing sections (Context, Acceptance criteria, Out of scope, Open questions).
- Apply to draft opens the Editor with the proposal merged into the draft, unsaved. The agent never writes the file.
- Ask again sends a follow-up ("make it smaller", "assume CLI only"). Dismiss closes the panel; the last review stays reachable from the card's pill.
- The header also says how far the review went: `quick` (the issue alone) or `deep` (it read the repository). While it runs, the body shows the elapsed time and the agent's current step.
- OpenCode and Claude Code review in plan mode, unable to change the repository; the body then opens on a muted `meta` note saying so. A tool that rejects the flag reviews without it, and without the note.
- A Size finding that is not a Pass offers "Split into tickets…". The body then lists the proposed tickets, each with a checkbox and an editable title. Create N tickets writes them in the original's place and archives it, Back to review returns to the findings, and `⌘Z` on the board takes the split back.
- The last review is kept beside the issue, so the card's pill survives a restart. The pill turns muted once the issue has changed since.
