# NewIssueModal

Where a new issue gets written: a large modal over the board, with your notes on the left and the issue file, exactly as it will be written, on the right. Draft with Review fills that file in place, section by section, so you can read every word as it arrives.

## Why it replaces the inline composer

- The QuickAdd composer lives in the Todo column: 280 to 420pt wide, capped at 8 lines. That's fine for one sentence. It falls apart once you talk at length to the agent: your notes scroll inside a small box, and what the agent writes is cut short with "…", so you can't see what will land in the file.
- "The file is the truth" should hold *before* the file exists. The right pane shows the exact markdown that `↩` will write, at reading size, never truncated.
- **The one-sentence path doesn't get slower.** `⌘N`, type, `↩` writes the file and closes at once, and the title lands in the background, so tickets can be written one after another. The only cost is a 160ms fade.
- **There is still one composer.** Keeping the inline composer and adding an "expand" button would mean two places with the same states, and you'd have to decide how long your note will be before you start typing.

## Layout (Modal XL)

- **Head**: "New issue", then `in todo/` in `meta`, then the title-engine tag (`on-device`, `review agent`, `auto title off`). Clicking the tag opens Settings › Titles. Then the close button.
- **Notes pane** (left, default 40%, at least 320pt): the label "Your notes" in `ui-label` with a word count in `meta`, then one text area that fills the pane's height, in `read-body` (15/24), because this is prose you write and read back. Placeholder: "Describe the issue. Write or dictate as much as you like." It scrolls inside itself. It is read-only (`surface-sunken`, `ink-muted`) only while Review drafts.
- **Issue pane** (right): the `bg` well with a Reader sheet (`surface`, `reader-max` 624pt, `read-body` in `read-measure`). The sheet head reads `TODO · todo/<slug>.md`, or `todo/` + "named in the background" before a title exists. Under it come the receipt line (once a draft exists), the labels, the title in `read-h1`, then the body.
- **Foot**: key hints on the left. On the right: Discard (ghost, only when there is something to discard), Close `esc`, **Draft with Review** `⌘⇧R` (`review` label on a `line-soft` hairline; it reads "Redraft" once a draft exists), and **Add to Todo** (primary, `⌘S`; `↩` is among the hints on the left). After a draft, a "Keep my notes under ## Notes" checkbox appears before the buttons. Its value is remembered.
- **Narrow** (modal under 900pt): the panes stack, with the notes on top at 40% of the height.

## States

| State | Notes pane | Issue pane | Foot |
| --- | --- | --- | --- |
| **Empty** | Focused, with the placeholder | Sheet head `todo/` named in the background; title placeholder "Title is named from your notes" in `ink-faint`; empty body | Draft with Review disabled until the notes have text |
| **Writing** | Text grows; word count | **Mirror**: the body is your notes, rendered as markdown, live | `↩ add` · `⇧↩ new line` · `⌘↩ add & edit` (shown while ⌘ is held) |
| **Manual** | Unchanged | The title row on the sheet takes QuickAdd's Manual state: engine Off or unavailable, placeholder "Add a title", tag "auto title off" | "empty title uses the first line" |
| **Drafting** | Read-only, with the label "Your notes · read-only while Review drafts" | Streaming in place, see below. Sheet head right: small Thinking i + "Review is drafting · 9s" | Draft button reads "Drafting…" (disabled); hint `esc stop, keep what arrived`; Add to Todo stays available and adds what is shown |
| **Drafted** | Editable again | Receipt: **Drafted by Review** from your notes · `tool · model · 14s` · Undo draft `⌘Z`. Title selected (typing renames). Full draft. | Redraft, Keep my notes, Add to Todo |
| **Notes changed** | Edited after the draft | Receipt turns `surface-sunken` with **Your notes changed since this draft** in `status-doing` and a Redraft button. The draft stays as it is. | Same |
| **Stopped** (`esc` while drafting) | Editable | What arrived stays. A section that was never written becomes a ghost tile ("Stopped before it was written"); a click on it opens the Editor on that section once the issue is added. | Same |
| **Failed** | Untouched | Receipt in `danger-soft`: **Review couldn't draft it**, then the reason naming the tool ("opencode exited after 120s"), then Retry. The sheet goes back to the mirror. | `↩` still adds your notes as written |
| **No agent** | — | — | Draft with Review is disabled, with the tooltip "Set up Review in ⌘, › AI review" |

## Drafting, in detail

This is the core of the change: you watch the ticket being written, at full size, in the place it will live.

- **The sheet opens at its final shape at once.** Following the Skeleton rule (what is already known shows for real), the standard headings (`[reader] sections`: Context, Acceptance criteria, Out of scope, Open questions) appear straight away as real `h2`s, each with two or three pulsing `surface-sunken` bars under it. The title row shows a 12pt bar until the title arrives. Nothing appears before 300ms, so a fast answer never flashes placeholders.
- **The agent writes top to bottom.** Its output is a stream of events: title, labels, summary, then one section at a time. The section being written gets a 2pt `review` rule on its left, a "writing" note in `meta` `review` beside its heading, and a `review` caret at the end of the text. Finished sections are plain `ink`. Waiting sections keep their bars.
- **Text lands by whole words**, repainted at most every 50ms, so lines don't jitter as characters arrive.
- **The sheet follows the writing line** and keeps it about a third of the way up from the bottom of the pane. If you scroll away, following stops, and a "Follow the draft ↓" pill appears at the bottom of the pane. A click on the pill, or `End`, starts following again.
- **A section the agent leaves empty** is not written. It shows as a ghost tile under the sheet, like the Reader's "Not written yet".
- **Copy and timing follow Thinking**: "Review is drafting" with an elapsed clock from 2s; from 20s, "Still working" and "`esc` stops and keeps what arrived"; a timeout gives the failed state. No percentage.
- **The agent never writes the file.** It fills the draft; you add it.

## What gets written

`↩` writes exactly what the sheet shows, under the title you typed or the one Review drafted. With neither, the file is `unnamed-ticket-xxxx` until the title lands in the background: see QuickAdd's "When the title lands".

- **No draft**: `# <title>`, a blank line, then your notes. This is QuickAdd's rule, unchanged.
- **Drafted**: `# [Label] <title>`, the summary paragraph, then each written section as `## <Section>`. With "Keep my notes" on (remembered in `[new_issue]` of `iboard.toml`), a final `## Notes` section holds your notes verbatim, so nothing you said is lost.
- The slug, collisions, the Todo position-1 renumbering and the new card's flash are all as QuickAdd describes. The modal closes, and the card flashes at the top of Todo.
- `⌘↩` writes the file and opens the Editor on it. Small fixes to a draft happen there, so iboard keeps one editor rather than an editable preview.

## Closing without adding

- **Close, `esc` or ×** with notes: no question. The modal closes and the notes and draft are **parked** in `.iboard/new-issue.md`, which survives a restart. The next `⌘N` opens with them, and the Todo `+` shows a small `accent` dot (tooltip "1 parked note"). With empty notes, nothing is parked.
- **Discard** (ghost) drops the notes and the draft. It doesn't ask, and `⌘Z` in the next 10s brings them back (the status bar says "Discarded the new issue · ⌘Z").

## Keys

| Key | Action |
| --- | --- |
| `⌘N` | Open (anywhere; from the Reader or Editor the board shows behind it) |
| `↩` | Add it to Todo and close; without a title, it is named in the background |
| `⌘S` | Save it to Todo and close, as `↩` does |
| `⇧↩` | New line in the notes |
| `⌘↩` | Add and open the Editor |
| `⌘⇧R` | Draft with Review, or Redraft |
| `⌘Z` | Undo the draft (back to the mirror) when focus is not in the notes |
| `tab` | Notes → title → foot buttons |
| `End` | Follow the draft again |
| `esc` | Stop drafting (keeps what arrived); else close and park |

## Interaction model

The modal works on a **NewIssueDraft** read model: `{ notes, title, title_source: none | typed | drafted, labels, body: mirror | draft { summary, sections[], partial, notes_hash }, keep_notes, state }`. Nothing touches `todo/` until IssueCreated.

| Screen | Command (user) | Event | Next screen / read model |
| --- | --- | --- | --- |
| Any | New issue (`⌘N`, header button, Todo `+`) | NewIssueOpened (with ParkedDraftRestored if one exists) | NewIssueModal; focus in the notes |
| NewIssueModal | Type in the notes | NotesChanged | Mirror updated; a draft's `notes_hash` no longer matches, so the receipt shows "changed" |
| NewIssueModal | Type in the title | TitleEdited | Slug updates |
| NewIssueModal | Draft with Review (`⌘⇧R`) | DraftRequested | Notes read-only; sheet skeleton with the known headings |
| (agent) | — | DraftTitleReceived, DraftLabelsReceived, DraftSectionStarted, DraftTextAppended, DraftSectionCompleted | Text fills in place; the writing section is marked |
| (agent) | — | DraftCompleted / DraftFailed | Receipt / failed receipt, mirror back |
| NewIssueModal, drafting | `esc` | DraftStopped | Partial draft kept; unwritten sections become ghost tiles |
| NewIssueModal | Undo draft (`⌘Z`) | DraftUndone | Mirror |
| NewIssueModal | Toggle Keep my notes | KeepNotesToggled | `## Notes` shown or hidden at the end of the sheet |
| NewIssueModal | Add to Todo (`↩` or `⌘S`) with a title | IssueCreated | Board; the new card flashes at the top of Todo |
| NewIssueModal | `↩` with no title | IssueCreated, TitleInferenceRequested → TitleInferred / TitleInferenceFailed | Board; the card reads "Naming…", then takes its title / stays `unnamed-ticket-xxxx` |
| NewIssueModal | `⌘↩` | IssueCreated → EditStarted | Editor on the new file; a missing title lands once the Editor closes |
| NewIssueModal | Close, `esc`, × | NewIssueParked (notes not empty) / NewIssueClosed | Board; Todo `+` shows the parked dot |
| NewIssueModal | Discard | NewIssueDiscarded | Board; `⌘Z` restores it for 10s |

The consumer provides what QuickAdd needed (the Todo path, the engine state, `infer_title`, `create`), plus the section list, `draft_issue(notes) -> Stream<DraftEvent>` (cancellable, from the configured Review tool in one-shot mode) and the park store (`.iboard/new-issue.md`). The Review prompt asks for JSON lines (`{"title"}`, `{"labels"}`, `{"section","text"}`) so iboard knows which section is being written. If the tool returns plain markdown instead, iboard splits it on `## ` headings as they arrive.
