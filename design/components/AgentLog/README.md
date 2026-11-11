# AgentLog

The build's transcript in the side panel beside its issue: what the agent is doing now, what it said, what it changed, and how the run ended. It replaces the panel where every tool step was one line, so a single run of twenty `Reading …` calls filled the panel with near-identical rows and the only way to find the agent's actual reasoning was to read past all of them.

## What changed, and why

| Before | After | Why |
| --- | --- | --- |
| A flat list of steps. One run reads as fifty near-identical lines, each wrapped to the panel's width. | Steps between two messages become one **activity row** — "Read 5 files, searched 2, ran 1 command" — that opens in place. | The story stays on the surface; the machinery is one line until you ask for it. A run is read in seconds, not scrolled. |
| The newest step is repeated above the timeline in Conversation, with no home, and disappears from history. | A **Now** strip under the switch owns the live step and the clock. It sits outside the scroll and is never repeated. | One place for "what is it doing", visible while you read the rest. |
| Conversation drops every step but the last; Log prints them all. Both read as one stream. | **Timeline** is the default, folded; **Raw** is the exact log, line numbered, in mono. | Two views, two jobs: understand the run, or inspect what really came out. |
| Edits are eight `Using edit tests/release.rs…` steps buried in the same list. | One **Changes** block per run: "5 files changed +214 −63", a row per file, each opening the Diff screen. | The audit question is "what did it change", not "which tool did it call". This is the dropdown issue 002 asked for. |
| The head shows `running · 1:54`; run boundaries are a faint `──` line lost in the text; a three-run log reads as one endless stream. | The head shows the state; each run has a heading with its start time and agent, and closes on its outcome. | A log is a history. Runs are the chapters, outcomes are the full stops. |
| The composer says "Reply to the agent…" while it runs, after the run, and after a failure alike. | Placeholder and Send follow the state; while a question waits there is no composer at all. | The box says what pressing Send will do. |

## Placement

- The side panel beside the Reader, 360pt wide by default and resizable, as today. One `surface` card with the `review-soft` head. The panel is the agent talking, so its head is violet; the transcript inside is mostly ink.
- Top to bottom: **head** · **Timeline/Raw switch** · **Now** (while running) · scrolling **timeline** · **composer**. Only the timeline scrolls.
- A waiting **Question** card sits above the switch, first in the body, as specified in Question. The head then reads "waiting on you", and the Now strip is absent. An answerable question replaces the composer with the quiet line "Answer the question above to continue."; a question only another terminal answers keeps the disabled composer instead. Closing the panel is always allowed.
- There is no Stop in the panel: stopping belongs to the Doing card and column header, where the build as a whole is run.

## Head

- `review-soft` fill, title in `ui-strong` `review`, detail in `meta` `review`. Both wrap; a long file name breaks rather than widening the panel.
- Title: "Agent log of `<file>`". Detail, one state only: "running", "not running" or "waiting on you".
- The agent is not repeated here. Each run heading names the agent that ran it, which is what matters when a log mixes OpenCode and Claude.
- No clock in the head: while running the Now strip owns it, once finished the outcome owns the duration.

## Switch

- A 24pt segmented control, left, "Timeline" and "Raw", 22pt rows on `surface-sunken` with the selected one in `accent-soft` and `accent` (see `.ib-seg`). Timeline is selected on open.
- The switch changes only the body. The head, the Now strip and the composer stay put, so switching views never loses your place or your draft.
- `Tab` reaches the switch; `←` `→` move between the two; `space` or `↩` picks one.

## Now

- Only while a run is live. A 28pt band above the scroll: the 14pt Thinking mark, the step, and the elapsed clock at the far right in `meta`, tabular.
- The verb reads in `ink-muted`, its object in `ink` at 500: "Editing **src/app/release.rs**". One line; the object truncates from the end with the full text in a tooltip. No trailing ellipsis: the mark is the progress.
- If there is no step yet: "Reading **the issue**".
- No percentage, no progress bar, no invented phase. The counts belong to the activity row; the clock is the only time signal.
- While the panel is live, the timeline follows its end. Scrolling up detaches it; a compact **Latest** pill appears at the bottom right of the timeline and returns to the live edge when pressed. It is not shown while you are at the bottom, so it never competes with the composer.
- The band leaves in one frame when the run ends. The outcome replaces it as the place that says how it went.

## Timeline

### Runs

- Every run starts with a run head: "Run 2" in 12/16 500 `ink-muted` on the left, "today 14:02 · opencode" in `meta` `ink-faint` on the right. Not today: "Nov 7 · 14:02". The agent stays `ink-muted` so it reads after the time.
- Runs are separated by one `line-soft` rule and `space-4`. The first run has no rule above it. The run head is always visible at the top of its run, even when its steps are folded.
- A run is one build attempt. A retry starts a new run; the old failures stay in the history.

### Messages

- The agent's messages are rendered markdown at `read-body` 15/24, in the panel's width, capped at `read-measure` when the panel is widened. No bubble, no avatar, no violet fill: inside the violet-headed panel, the agent's prose is the document. Inline code sits on `surface-sunken`; links use `accent`.
- The author's instructions are the only tinted block in the timeline: right-aligned, at most 86% of the width, `accent-soft`, `radius-sm`, a "You" label in `meta` 500 `accent`, and the text at `ui-body` 13/18. Shape and colour make a reply findable without a label on every turn.
- Messages are `space-4` apart. A message on either side of an activity row needs no "Agent" label: the violet head, the run heading and the right-aligned reply are enough.

### Activity rows

- The steps between two messages — reads, searches, commands, shell calls — are one **burst**; edits happen in that span but are collected into Changes. A burst with two or more steps folds into one activity row. A lone step stays a single row: a disclosure that opens onto itself is noise.
- The summary names the verbs and their counts, most used first, ties by first appearance: "Read 5 files, searched 2, ran 1 command". One reads "1 file", "1 search", "1 command"; more than three kinds still list them, truncating the line only if the panel is very narrow.
- The row is 28pt, 12/17 `ink-muted`, with a 12pt chevron on the left, no edge, no fill at rest. Hover `surface-hover`. It is a native `<details>`/`<summary>`: `esc` closes a menu, not this; `↩` or `space` toggles it; the screen reader announces "Read 5 files, searched 2, ran 1 command, collapsed".
- Open, its rows sit on a 1px `line-soft` rail, `space-1` padding, 24pt minimum: the verb in `ink-muted`, then the object in `ink` at 12/17. The verb is the step as the agent logged it, in the present: "Reading", "Searching", "Running", the same words the Now strip uses. Only the summary speaks in the past. Paths truncate from the end; the full path is a tooltip. Searches show the query unquoted; commands are mono at `edit-body` 13/20.
- The group's rows are evidence, not actions: nothing opens. The one clickable thing a run produces is its Changes list.
- The live step is never in the open group; the Now strip owns it. Once the run stops, the last step returns to the group.
- Edits are not activity rows. They belong to Changes, so the same edit is not said twice.

### Changes

- One block per run, after the last message or activity row and before the outcome: the run's receipt.
- While running the title reads "3 files changed so far"; when the run ends, "5 files changed". The right side is the run's diffstat, `+214` in `status-done` and `−63` in `danger`, tabular — the same pair the Diff screen uses.
- The summary is 28pt on `surface-sunken`, `radius-sm`, a chevron, the title in 500, the diffstat in `meta`. It is a native `<details>`; collapsed by default, even after a success, so the transcript stays short.
- Open, one row per file at 26pt: path at 12/17 `ink`, the diffstat right-aligned. An added file gets a leading `A`, a deleted file `D`, in `ink-faint`; a modified file needs no letter. The path truncates from the end; the full path is the tooltip.
- Each row is a button that opens the **Diff screen** on that file, where the hunks are side by side with syntax colours. This is the deliberate split: a 360pt panel cannot show a side-by-side diff, so the log is the index and the Diff screen is the reader. The row's accessible name is "Open the diff of `<path>`, +86 −24".
- The diffstat is the sum of the rows. A run that edited nothing has no block.
- The block needs the run's working-tree changes, not only its commits. Until the Diff screen can take a working-tree set, the rows can still list the files from the log; the diffstat and the action wait for that data rather than inventing it.

### Outcome

- The last line of a run: a 12pt check in `status-done`, "Finished" at 500, then "in 3:12" in `meta` `ink-muted`; or an `✗` in `danger`, "Stopped:", then the reason in `ui-small` `danger`, wrapping to as many lines as it needs.
- No card, no fill, no second button. The run asks nothing of you; if it failed, the composer below is the next move.
- Nothing follows the outcome within the run. The next run's rule is the boundary.

## Raw

- The exact log, one line per row, `edit-body` 13/20 in the panel's width. A 28pt gutter of line numbers in `meta` `ink-faint`, tabular and not selectable; long lines wrap, so a command is readable at 360pt.
- Colour is the only addition: `──` run headings in `review`, `▸` author lines in `accent`, `✓` outcomes in `status-done`, `✗` in `danger`. The characters and the text never change.
- The Now strip and the composer stay; Raw is a view of the log, not a different screen.
- Raw is for copying a command, reading a stack trace, or checking whether the folded summary lost something. It is not the default: most looks are Timeline.

## Composer

- The foot, above a `line` rule, `space-4`: a two-row textarea at `ui-body`, `line-strong` edge, `radius-sm`, focus in `focus` with the `focus-halo`; then a row with "⌘↩ sends" in `meta` `ink-faint`, a spacer, and the panel's one filled button, **Send**, in `primary` ink.
- Send is primary, not `review`: the author is speaking, not the agent. It is disabled while empty and drawn on `surface-sunken` with `ink-faint`, as every disabled primary.
- Placeholder by state:

| State | Placeholder | Send |
| --- | --- | --- |
| Running | "Reply to the agent…" | Enabled with text; the agent reads it after the current step |
| Finished | "Ask for another pass…" | Enabled with text |
| Stopped | "Ask it to fix what failed…" | Enabled with text |
| Waiting on a question | The composer is gone; "Answer the question above to continue." | — |
| A build that cannot take a reply (a child process, or a Claude background build) | "This build cannot take a reply." above the field, field disabled | Disabled |

- A Claude background build takes no instruction while it runs: no command sends one into a live session. Its question names `claude attach <id>`, and the composer under it stays disabled with the line above.

- `⌘↩` sends from anywhere in the field. The text clears on send and the instruction appears in the timeline at once, right-aligned; the timeline follows it. No toast: the status bar already says "Told the agent on `<file>`".
- The composer never takes focus when the panel opens. It takes focus after a question is answered, so a follow-up is one keystroke away.

## Empty, failed, blocked

- **Starting up:** the Now strip, then "The agent has not said anything yet." in `ui-small` `ink-muted`, over three skeleton bars at the message width. No spinner under the head; the mark in the Now strip is the activity.
- **No log at all:** the panel is not opened. The `…` menu's "Agent log" says there is none, as today.
- **Failed:** the run's outcome names the failure; any changes remain inspectable; the composer reads "Ask it to fix what failed…". There is no Retry in the panel — the Doing card's play button starts the next run, and the log keeps the failed one.
- **Blocked:** the Question card first, then the switch and the timeline. No composer, one filled Send, on the question card.
- **No messages, many steps:** one activity row per burst. This is the screenshot's case: the whole run is a run head, a few folded rows and a Changes block, not a page of "Reading…".

## Keyboard and accessibility

- `Tab` order: switch → timeline (a focusable scroll region) → each activity summary → each Changes summary → each file row → composer → Send. No new global shortcut; `⌘↩` sends only inside the composer.
- The panel is a labelled complementary region, "Agent log of `<file>`". Each run is a labelled section, "Run 2, today 14:02, opencode"; the outcome is a status announcement at the end of the run.
- The Now line is a polite status. Visually it updates at once; for assistive tech the app speaks a new step at most every 10s and always announces the run's start and end. A step never interrupts what is being read.
- Native `<details>` and `<button>` carry the disclosures and file rows; focus is the 2px `focus` ring, or the inset ring on a file row, never a colour change alone.
- Contrast: `review` on `review-soft` 5.2:1; `ink-muted` 6.4:1 or better on every surface; `ink-faint` at least 4.5:1; the diffstat's `status-done` and `danger` on `surface-sunken` stay above 4.5:1.
- Reduced motion: the Thinking mark rests, the chevron changes state in one frame, and following the end of the timeline is instant. The live clock still counts.

## Width

- 360pt default; the pane resizes wider and the layout holds. The head wraps; the Now line truncates its object; activity and Changes summaries truncate; nothing stacks.
- Messages keep `read-measure` 560 when the panel is widened, so the prose does not run the full width of a 900pt panel. Activity rows, Changes and Raw take the panel's width.
- The side panel's divider still resizes it; the log keeps the reader's scroll position when it opens or closes, as the pane does today.

## Tokens used

`surface`, `surface-hover`, `surface-sunken`, `line`, `line-soft`, `line-strong`, `ink`, `ink-muted`, `ink-faint`, `accent`, `accent-soft`, `review`, `review-soft`, `status-done`, `danger`, `focus`, `focus-halo`, `radius-sm`, `radius-md`, `radius-pill`, `space-1` to `space-4`, `read-measure`, `shadow-popover`, `motion-hover`, and the `ui-strong`, `ui-body`, `ui-small`, `meta`, `read-body`, `edit-body` styles.

## In egui

- `build_log::parse` already returns `Run`, `Step`, `Outcome`, `Author` and `Message`. The fold needs one step per line; the lost newlines of issue 002 are the parser's bug and a prerequisite, not something this view can repair.
- Add `Entry::Edit(&str)` when a step is an edit, so the view never matches "Editing " by hand, and a `segments()` fold in the same module: consecutive steps (edits excluded) become an `Activity { steps, counts }`, a lone step stays a `Step`, and everything else passes through. Give it tests: a burst folds, a lone step does not, edits leave the burst, counts pluralise, runs split the list.
- `agent_log_panel` maps the fold to the new widgets. The `LogMode` enum becomes `Timeline` and `Raw`. The Now strip is the last unfurled step of the live run; it is not drawn when `building_since` is `None`. The head detail is derived from `running` and `question_for`.
- The Changes block needs the run's file list and diffstat from the diff service. Until it can read the working tree, show the paths with no stats and no row action; do not fake the numbers.
- Activity and Changes are `CollapsingState` rows whose 28pt summary reports `Role::Button` and its full summary text through `widget_info`, including ", collapsed". File rows are 26pt buttons named "Open the diff of `<path>`, +86 −24".
- The composer uses the existing `text_entry` with `ButtonKind::Primary` for Send; the panel's one fill. `⌘↩` consumes only when the field is focused; an empty or question-locked field sends nothing.
- The raw view is a monospace `Label` per line in a `ScrollArea`, with a painted number gutter; do not use a `TextEdit` for two thousand lines. The timeline folds before it lays out, so a long run costs one row per burst, not one per step.
