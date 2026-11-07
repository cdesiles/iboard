# PlanPanel

The Plan panel turns an open conversation with the agent into tickets. **Tools › Plan…** opens it over the board as a window, not a new screen: the board stays behind it and the plan is scratch, not a record.

## Anatomy

1. **Head.** "Plan" in `ui-title`, the **kind chip** (`Spike`, `Epic`, `Bug review`, `Tech debt review`), then `tool · model` in `meta` `ink-muted`. **Close** sits on the right. Under it, one `meta` line says what the current kind is for.
2. **Transcript.** Each turn starts with its speaker in `meta` (`You` in `ink-faint`, `Agent` in `review`). The author's words are body text; the agent's turn is rendered as markdown. The transcript scrolls on its own.
3. **Progress.** While the agent reads or writes, "Thinking…" and its current step, in `review`. A failure takes its place, in `danger`. The outcome of filing tickets follows, in `ink-muted`.
4. **Composer.** A multiline field with the hint "Tell the agent what to think about, or ask the next question". Under it: **Send** (`⌘↩`) and **Turn into tickets** on the right, a quiet note on the left. While a turn runs, **Cancel** replaces both.

## States

| State | What shows |
| --- | --- |
| Fresh | An empty transcript, "What should we plan?", and the composer |
| Talking | The conversation; Send is enabled only with text in the field |
| Waiting | "Thinking…" and the agent's latest step; Cancel and a busy mark |
| Failed | The agent's error in `danger`; the conversation stays editable |
| Ready to file | **Turn into tickets**, enabled once the agent has answered |
| Filed | "Filed N tickets in Todo." and the first new card selected |
| Reopened | "A plan is waiting here", the message count and the topic, then **Continue this plan** and **Start fresh** |

## Kind

The first topic infers the kind: a reported failure is a Bug review; words such as debt, refactor or cleanup make a Tech debt review; investigate, research or spike make a Spike; anything else is an Epic. The chip overrides it and the choice survives later turns. The kind switches the sections of the one prompt the agent follows:

- **Spike**: Question, Context, What to investigate, What we would do next.
- **Epic**: Problem, Outcome, Context, Acceptance criteria, Out of scope.
- **Bug review**: Symptom, Expected, Actual, Steps to reproduce, Acceptance criteria.
- **Tech debt review**: Debt, Where it costs, Context, Scope, Acceptance criteria.

## Tickets

Only **Turn into tickets** asks for the `=== TICKET ===` blocks. The agent answers with 2 to 6 `# [label] Title` tickets whose bodies carry the reasoning from the conversation. iboard parses them and writes them into Todo, where they become ordinary issues. Nothing is filed before the button is pressed.

## Scratch

The session id, transcript and kind are kept as JSON in the cache folder, next to `iboard.sock`, keyed by the project path: `plan-<project>-<hash>.json`. Closing iboard ends the conversation but leaves the file; reopening offers to continue it or start fresh and never resumes on its own.

## Tokens used

`bg`, `surface`, `surface-hover`, `surface-sunken`, `line`, `line-strong`, `line-soft`, `ink`, `ink-muted`, `ink-faint`, `accent`, `accent-soft`, `review`, `review-soft`, `danger`, `radius-sm`, `radius-md`, `radius-pill`, `space-2`, `space-3`, `space-4`, `space-5`, `control-h`, and the `ui-title`, `ui-strong`, `ui-body` and `meta` styles. Agent markdown uses 13pt body text with `ui-title` headings and 12pt monospace.

Emits PlanOpened, PlanContinued, PlanStartedFresh, PlanTurnSent, PlanTicketsRequested, PlanTicketsFiled, PlanClosed.
