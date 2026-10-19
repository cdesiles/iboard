# RefinePanel

The dialog where a refinement agent (the Challenger) pushes back on an issue and offers numbered ways to resolve it. It replaces the single scrolling column of the first version, where the TL;DR, three paragraphs, four long choices and an answer box all shared one scroll, the choices sat below the fold, and a choice only differed from plain text by a small boxed number and a pale fill when picked.

## What changed, and why

| Before | After | Why |
| --- | --- | --- |
| Reasoning and choices in one scroll; choices pushed below the fold | Two panes: reasoning on the left, choices on the right (`bg` well, `line` divider) | You can read the argument and look at the options at the same time; the choices never scroll away |
| Each choice is one long sentence | A **title** (`ui-strong` 600) and a **detail** (`ui-small`, `ink-muted`) | The four options can be compared by scanning four short titles |
| Choices look like list text; selected = pale blue fill only | Each choice is a bordered card with a `line-strong` edge, its key in a Kbd and a radio dot; selected = `accent` 2px edge, `accent-soft` fill, filled radio, key in `accent` | Reads as clickable; selection never relies on fill colour alone |
| TL;DR in a filled violet box, bold 17pt | `ui-title` 15/22 with a 3px `review` rule on the left | Still the first thing read, but lighter; violet stays the "agent is talking" hint |
| Prose 15pt across the full 1100pt width | `read-body` 14/22 capped at 68ch | Long lines were the main reading cost |
| One free-text box, unrelated to the choices | The box follows the selection: "Add a note to choice N (optional)" once one is picked, "Or answer in your own words" otherwise | A choice and a nuance can go in the same answer |
| "Hide", "Dismiss", a quiet "Send ⌘↩" | "End refining" (ghost), "Hide `esc`", and a filled `review` **Send** that names what goes out: "Send choice 1", "Send answer"; disabled with nothing to send | Dismiss vs Hide was unclear; the main action now looks main and says what it does |
| Header shows tool · model · role in one violet run | Path in mono, role as a `review` pill, tool and model in `meta` `ink-muted` on the right | The file and the role read first; the plumbing reads last |

## Layout

- A `surface` panel, `line-strong` edge, `radius-md`, `shadow-popover`, up to 1080pt wide, centred over the Reader like the screenshot's version.
- **Head** (36pt, `review-soft`): "Refining `todo/002-toolbar.md`", the role pill (Challenger), and `tool · model` at the right, truncating first.
- **Why pane**: TL;DR, then the agent's paragraphs. Inline code on `surface-sunken`. Scrolls on its own past 520pt.
- **Pick pane** (320 to 420pt): the question head in `ui-label` ("How should we resolve it?", count in `meta`), the choice cards 8pt apart, then the note field.
- **Foot**: key hints on the left, then End refining, Hide, Send.
- **Narrow** (panel under 820pt, or inside a 720pt Reader): one column. The reasoning folds under the TL;DR behind "Show the reasoning (N paragraphs)", so the choices sit right under the summary.

## Choice titles

The agent is asked to return each choice as `{ title, detail }`, title at most 6 words, starting with a verb. If it returns a plain sentence, iboard splits it at the first `:` or ` — ` (title before, detail after); with neither, the whole sentence is the title and there is no detail. The full text is what gets sent back, never the shortened title alone.

## Keys

- `1`–`4` (up to `9`) pick a choice while the note is not focused; the same key again unpicks it. `↑ ↓` move between choices, `space` picks.
- `tab` goes to the note, `⇧tab` back to the choices. Number keys type digits inside the note.
- `⌘↩` sends from anywhere in the panel. `esc` hides the panel (the session is kept and the Review button shows it is waiting); End refining closes the session.

## Behaviour

- Send is disabled until a choice is picked or the note has text. Its label says what goes out: "Send choice 2", "Send choice 2 with note", "Send answer".
- What is sent: the choice's full text, then the note, if any.
- The panes keep a 320px floor, so the panel holds its shape while the agent answers. Meanwhile the pick pane is replaced by a spinner and "Challenger is thinking…", the elapsed clock pinned at its right end, and `esc` cancels; the next round replaces the why pane and the choices, and the previous round folds into a one-line "Round 1: you chose Split into four tickets" above the TL;DR.

| Command (user) | Event | Next state |
| --- | --- | --- |
| Click a choice, `1`–`9` | RefineChoicePicked | Choice selected; note label and Send label follow it |
| Same key again | RefineChoiceCleared | No choice; note reads "Or answer in your own words" |
| Type in the note | RefineNoteChanged | Send enabled |
| Send (`⌘↩`) | RefineAnswerSent → RefineRoundReceived | Next round, or the refined draft in the Editor, unsaved |
| Hide (`esc`) | RefinePanelHidden | Reader; Review button shows the waiting session |
| End refining | RefineSessionEnded | Reader; nothing written |

The consumer provides the agent's round (role, TL;DR, paragraphs, choices) and the tool and model names. The agent never writes the file.

## Kept from the first version

- Opened from the ReviewButton (its main part, its menu's "Refine with a challenger", or `⌘R`) on a Todo issue, as a modal over the Reader or Editor page on the `scrim`. Opening a review closes it, and the other way round.
- The challenger is asked for the issue as it reads now (the Editor draft when open) and the conversation so far.
- Once the need is settled the challenger proposes the refined issue: Apply to draft opens the Editor with it, unsaved, and hides the panel.
- Hidden, or after a click on the scrim, a Review-kind pill floats at the foot of the window, "Back to the refinement" ("Refining…" while a turn runs), and brings the panel back.
