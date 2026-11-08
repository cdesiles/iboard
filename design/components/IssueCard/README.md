# IssueCard

An issue on the board: its labels, the file's first heading as title, its first paragraph as summary. The file name is not on the card at rest.

## At rest

- `surface` fill, `line-soft` edge (1.19:1 on `bg` light, 1.38:1 dark: the white fill does the work), `radius-md`, padding 10×12.
- Rows, top to bottom, `space-3` (8pt) apart:
  1. **Labels** (see Label), after the review verdict pill when there is one; only when the issue has either.
  2. **Title**, `ui-strong` `ink`, at most two lines, ellipsis.
  3. **Summary**, `ui-small` `ink-muted`, at most two lines, ellipsis.
  4. **Done only**: the date it was finished (`YYYY-MM-DD`), `meta` (Inter) `ink-muted`.
- No file name, no actions, no shadow, no coloured stripe, no avatar.
- Done cards keep full contrast.

## Hover, selection, focus

- **Hover**: edge to `line-strong`, fading in over `motion-hover` 100ms.
- **Selected** (keyboard or single click): `accent` 2pt edge on `accent-soft`, in one frame.
- On hover, on the selected card, on keyboard focus and while its LabelPicker is open, a **strip** is laid over the card's bottom 28pt on the card's own fill: the file name (`meta`, `ink-muted`, truncates with an ellipsis), the date on Done, then the actions. It covers the last summary line or the date row and never changes the card's height, so nothing below moves. A card with neither summary nor date keeps 28pt under its title for it.
- **Expand** sits in the card's top right corner, 6pt in, over the card's fill, and shows with the strip: "Expand  ↩", the outward arrows icon. With a multi-selection only the card with focus offers it.
- Actions, left to right: Add label, Review (the selected Todo card, once an agent is set up), then Delete. Done cards offer Show the agent log, Show the diff and Archive instead: done issues are archived, never deleted, and keep the labels they have. Each is a 22×22 icon button, 14pt line icon at 1.5pt stroke, `ink-muted`, no edge; on its own hover `surface-hover`, Review turns `review` and Delete turns `danger` only then. Tooltips name the key: "Add label  L", "Review  ⌘⇧R", "Show the agent log", "Show the diff", "Archive  A", "Delete  Del".
- Actions are their own click targets, so pressing one never opens the issue. Delete always goes through ConfirmDialog (unless asking is off); Archive acts at once (it only moves the file to `.archive/`). Add label opens the LabelPicker under the action cluster. Show the diff opens the Diff screen on every commit that closed the issue (`closes <file>.md`), or says in the status bar that none did. Show the agent log opens the Reader with the build agent's log beside it, or says that no agent built the issue.

## Behaviour

- A click selects the card, and only it; it never opens it. `↩`, a double click or **Expand** opens the Reader. With a multi-selection, `↩` opens the first picked card in board order.
- Command-click picks the card into a multi-selection without opening it; command-clicking a picked card takes it out. Shift-click picks every visible card from the selected one to the clicked one, in board order; the selected card stays the anchor of the next shift-click. Picked cards share the selected look. An action on a picked card (Delete, Archive, Add label) applies to every picked card, and dragging one carries them all, landing together in board order. Delete asks once for all of them.
- Clicking a label filters the board by it and never opens the issue.
- **Review verdict.** The last review's verdict (Pass, Challenge, Block) is a StatusPill first in the label row; `ink-faint` once the issue changed since. A click reopens the review. While a review runs in the background the pill says "Reviewing…", then "Review ready" in `review` once it lands, until a click opens the issue with its review panel.
- **AI build.** The card the build agent takes flashes `status-doing` for 0.6s as it lands in Doing (skipped with reduced motion); while the agent works, its 2pt `status-doing` edge breathes and a light runs along its foot (a steady edge with reduced motion). Its strip stays shown and holds the time spent, the Thinking clock ("4:12") in `status-doing`, left of the actions. It offers Stop ("Stop the AI build") in Delete's place and no Retry, since the agent is still on it. With no build running, Doing's cards offer Retry ("Retry with AI", the refresh icon), which starts the agent on it over, beside Delete.
- **Build question.** While the service waits on the issue's form or tool approval, the card adds a `surface-sunken` note under its text: a `bubble` icon and "Question" in `status-doing`, then the form's title (or "Allow bash?") in `ui-small` `ink`. A **question pin** sits at the card's top right, left of where Expand appears: a 22pt icon button with the `bubble` at 14pt in `status-doing`, on `status-doing` at 16% over `surface`, so it reads before the pointer arrives. It stays at rest and while the strip shows, and the title and summary stop before it. A click on the pin or the note opens the ticket on its agent log, with the question above it; both are their own click targets and never only select the card. The bell counts the waiting questions. The card that answers it is `components/Question`.
- Dragging a card opens the DropZones dock as a second way to archive or delete. The card being dragged stays in place at 40% opacity; a lifted copy follows the pointer.
