# Question

The card a build stops on when its agent needs a person. It sits at the top of the agent panel, inside the ticket, above the timeline. Answering it resumes the build; there is nothing to dismiss.

It replaces the first in-ticket card, where a `surface-sunken` block with an edge derived from `status-doing` held 18pt chips, a mono field for prose, two filled Send buttons and no scroll: a long form ran past the panel, an empty "Other…" looked chosen, and the agent's own question title disappeared when the form had one field.

## Placement

- First block in the agent panel body, above the Timeline/Raw switch and the timeline's scroll area. The panel head stays: "Agent log of `<file>`" with the state, now "waiting on you"; the run heading below keeps the agent and start time.
- The question never lives in a modal, a tray or a toast. The bell and the card's pin only take you to this spot in the ticket.
- The card is bounded: at most 55% of the panel body. Its fields scroll inside; the foot with Send is pinned. The conversation keeps its own scroll below.
- While a question waits, the composer under the timeline is not drawn. A single quiet line takes its place: "Answer the question above to continue." No second textarea and no second Send.
- Closing the panel is allowed, and the X never refuses with a notice. The question stays in the ticket; the card pin and the bell both reopen the ticket on it.

## Anatomy

1. **Head.** A 14pt `bubble` in `review`, "The agent asks" in `ui-label` uppercase `review`, and how long it has waited at the right, in `meta` `ink-faint`, tabular ("18m ago"). No fill, no edge on the head.
2. **Title** in `ui-title` `ink`: the agent's question title. When it is empty, the single field's title; when both are empty, "Question". The field label below is drawn only when it differs from the title, so a single-field form never says the same thing twice.
3. **Fields.** One per question field, `space-4` apart, in the order the agent sent them. A field is its label in `ui-strong` (with "Optional" in `meta` `ink-faint` when it can be left empty), its help line in `ui-small` `ink-muted`, then its control.
4. **Foot.** A `line-soft` rule, `space-3` padding, one filled `review` Send on the right, "Send answer" with `⌘↩`. Nothing on the left at rest; while ⌘ is held, the selection keys ("1–3 choose") appear in `meta` `ink-faint`.

The card is `surface` with a `line-soft` edge and `radius-md`, `space-4` at the sides and the head. It sits on the side panel's `surface`, which the edge separates. Nothing is tinted `status-doing`: the card is the agent talking, so its identity is `review`. The blocked Doing ticket keeps that colour on the card pin and the bell badge.

## Fields

| Kind | Control | Answer |
| --- | --- | --- |
| Text, no options | 3-row field, Inter 13/20 on `surface`, `line-strong` edge, focus halo | The text, trimmed; empty when optional |
| Text, options | Option rows, single-select, radio marks; an Other… row when the tool allows a custom answer | The chosen option's value, or the typed text |
| Number | 24pt field, 120pt wide, Inter with tabular figures | Parsed; invalid shows inline and blocks Send |
| Yes/No | Two option rows, Yes and No | `true` or `false`; unselected is not `false` |
| Multiple | Option rows with checkbox marks, plus Other… when allowed | The checked values; Other adds a typed value as its own selected row |
| External | One `ui-small` line and an "Open form" link | Nothing: when it is the form's only field, Send is absent, since the answer happens outside iboard |

Fields the agent marked hidden are not drawn and are not sent.

## Option rows

- 36pt minimum: an 18pt number key in `surface-sunken`, the title in `ui-strong` `ink`, the description in `ui-small` `ink-muted` under it, and a 14pt mark on the right.
- The description is always visible. It is not a tooltip: the choice between options usually lives in that line.
- At rest `surface` and `line-soft`; under the pointer `surface-hover` and `line-strong`; selected `accent-soft` with a 1px `accent` edge, its radio taking the accent ring (a checkbox fills), keeping its fill under the pointer.
- Focus draws the 2px `focus` ring outside the row, never a fill. Selection is `accent`; focus is the ring.
- The number key picks or toggles the row; it is drawn in `surface-sunken` like the RefinePanel's choices, and stays visible.
- "Other…" is a row like the others. Its text field appears under it only once the row is picked, is focused on arrival, and is cleared when another row is picked. An empty custom answer is never the initial state.

## Foot and sending

- Send stays enabled when fields are unanswered. Pressing it marks the first missing or invalid field, focuses it, and says why under it ("Answer Version first."). A disabled button with no explanation is a dead end.
- While sending, the label is "Sending…" and the controls are inert, so a second answer cannot be sent.
- A failure keeps every answer. The foot keeps its rule and shows one `danger` line with Retry in `danger` text in place of Send; it never replaces the card with an error page.
- When the build stops before the answer, the card becomes a `surface-sunken` note: "This build stopped before it was answered.", the question title, and a Ghost Dismiss. Answers are kept in case the build resumes.

## Permission, the rare fallback

The builder is meant to run with its project folder as its scope, so a tool it cannot run is a misconfiguration, not a step in the flow. The card is the same shape, with one job: show exactly what would run, and where.

- Title: "Allow `<action>`?" where the action is "a command", "writing a file", and so on.
- Each resource on its own line in `surface-sunken`, `edit-body` (mono 13/20), wrapped, at most 96pt tall and scrolled past that. Mono is right here: it is code or a path.
- One scope line in `meta`: "Inside `<project>`" in `ink-muted` when every resource is under the project root. Otherwise a `danger-soft` band above the foot: "Outside `<project>`. The builder should only work in its own folder." — the one place this card raises its voice.
- Foot, left to right: **Reject** (Ghost, quiet; it turns `danger` only under the pointer), **Always allow in `<project>`** (Secondary), **Allow once** (filled `review`, `⌘↩`). "Always" names the project because that is the widest scope iboard offers; there is no global allow.
- When a resource is outside the project, **Always allow is not offered**: an always-grant outside the folder would undo the scope. Reject and Allow once remain, under the band.
- The tool's own message, when it sent one, is one `ui-small` `ink-muted` line above the foot. No paragraph explaining what the buttons mean.

## States

| State | What shows |
| --- | --- |
| Waiting, untouched | Every answer at rest; Send ready |
| Picked | The option's accent fill; Send names no choice, since the answer is what goes |
| Invalid | A `danger` line under the field, the field focused; the rest of the card unchanged |
| Sending | Send reads "Sending…", fields inert; no spinner under 2s |
| Failed | The `danger` line and Retry above the rule; every answer kept |
| Stale | The `surface-sunken` note and Dismiss |
| Answered | The card leaves in one frame; the panel says "Answered `<title>`" in the status bar |

## Keyboard and accessibility

- When the card arrives or the ticket opens on it, focus moves to the first unanswered control; the card is announced once, politely, as "Question from the build: `<title>`".
- `Tab` and `Shift+Tab` move between fields; `↑` `↓` move between options in a group; `1`–`9` pick or toggle; `⌘↩` sends. `esc` does nothing: the question cannot be dismissed.
- Each group is a labelled `radiogroup` or `group`; each option is a native radio or checkbox whose name reads its title and description; errors are `role="alert"` and the field carries `aria-invalid`.
- Contrast: `review` on `surface` is 6.1:1 and on `review-soft` 5.2:1; `ink-muted` and `ink-faint` keep their 4.5:1 floors; the selected row's `accent` edge is 5.5:1 on `accent-soft`.

## Tokens used

`surface`, `surface-hover`, `surface-sunken`, `line-soft`, `line-strong`, `ink`, `ink-muted`, `ink-faint`, `accent`, `accent-soft`, `review`, `on-review`, `danger`, `danger-soft`, `focus`, `focus-halo`, `radius-sm`, `radius-md`, `space-1` to `space-4`, `control-h`, and the `ui-title`, `ui-strong`, `ui-body`, `ui-small`, `ui-label`, `meta`, `edit-body` styles.

## In egui

`question_panel` draws `answer_card` from this spec. The card is one `Frame` (`surface`, `line-soft`, `RADIUS_MD`) whose body sits in an `egui::ScrollArea` capped by the host and whose foot is outside it, so Send cannot scroll away. Option rows are one custom widget that reports `Role::RadioButton` or `Role::CheckBox` with its title and description in `widget_info`, and stands 36pt tall. Send uses `ButtonKind::Send`, not `Primary`. The locked composer is removed from `reply_box` while `question_for(path).is_some()`; the panel body draws the quiet line instead. `toggle_agent_log` stops refusing to close and no longer raises "Answer the build's question first".

Emits AnswerSent, AnswerFailed, AnswerDismissed.
