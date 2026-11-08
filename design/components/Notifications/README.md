# Notifications

The bell in the top bar and the tray under it. It carries one thing today: a question the AI builder asks while it is building a ticket in Doing. The question is answered in the ticket, never in the tray; answering it removes the row and lowers the count. The card that answers it is `components/Question`.

## Placement

- The bell is the header's **last** control on every screen — Board, Reader, Editor, Diff and Stats — at the very right: an 18pt bell in a 24pt hit area, `ink-muted`, no edge at rest. It comes after New issue on the Board, after Save in the Editor, after `…` in the Reader and after Refresh on Diff, 8pt from the control before it and `space-5` from the window edge.
- Why the very right: the tray hangs under it, aligned to the window's right gutter. It never covers the header's own controls, and it stays in the same place while the rest of the toolbar changes between screens.
- **Waiting count.** A `status-doing` badge rides on the bell's top right: 16pt tall, numeral in `on-primary`, `radius-pill`, a `space-1` `bg` ring. 1 to 9 read as the number, 10 and more read 9+; nothing waiting, no badge. A question is about a ticket in Doing, so it takes the Doing colour. No new hue, no red.
- Hover fills the 24pt hit area with `surface-hover` over `motion-hover`; the icon stays `ink-muted`. Focus draws the 2px `focus` ring; `↩` or `space` opens.
- The bell is not one of the dynamic icon buttons (Edit, Add label, Delete): it has no label pill and no key hint. Its tooltip and accessible name are "Notifications, N waiting" — "Notifications" when the count is zero; the badge itself is `aria-hidden`.

## The tray

- 420pt wide, right-aligned with the bell, `space-2` under the bar. `surface` fill, `line` edge, `radius-md`, `shadow-popover`: it floats over the page and sits under any modal.
- **Head** 40pt (`bar-h`): "Notifications" in `ui-title` `ink`, a `line` rule below. Nothing on the right.
- **Group**: `Waiting on you` in `ui-label`, uppercase and `ink-muted`, `space-3` above and `space-4` at the sides. One group is enough while every row is a question.
- **Row** 56pt minimum, three columns: a 14pt speech bubble in `status-doing`, the ticket title in `ui-strong` `ink` cut with an ellipsis after one line, the question in `ui-small` `ink-muted` over at most two lines, and the time in `meta` `ink-faint` at the top right, tabular figures. `line-soft` is the hairline between rows only.
- **Time** says how long the build has waited, using the same relative format as everywhere else: "18m ago", "3h ago", "2d ago". Rows are oldest first, so the ticket blocked longest is on top.
- **Empty**: "No questions waiting." in `ui-strong` and "Questions from the AI builder land here." in `ui-small` `ink-muted`, `space-4` around both. The head stays; there is no group label. The bell has no badge.
- **Long lists** scroll inside the tray at 420pt; the head stays fixed. New questions append at the bottom, so nothing jumps under the pointer while the tray is open.

### Animation

Hover fills fade in over `motion-hover` and go at once. Nothing else moves: no slide-in, no stagger, no badge bounce. Under Reduce motion, the state changes in one frame.

## What it deliberately does not have

- **No "Mark all as read."** A question is not dismissed by looking at it; it clears when you answer it in the ticket. A bulk action would empty the tray while the build stays blocked.
- **No per-row unread dot.** Every row is unanswered by definition, and a dot that only goes away on read would never go away. The badge is the state; the row is the action.
- **No "Recent" group.** Finished builds and reviews are not questions and do not enter this tray while questions are its only use.
- **No toast, sound or Dock badge.** A waiting question is not an interruption. The bell and its count are the whole signal.

## Behaviour

| Do | Result |
| --- | --- |
| Click the bell, `↩` or `space` on it | The tray opens under the bell; focus moves to the first row |
| Click a row, `↩` on it | The ticket opens, scrolled to the question; the tray closes and the question stays counted until it is answered |
| `↑ ↓` in the tray | Moves focus between rows |
| `esc`, a click outside, or the bell again | The tray closes; focus returns to the bell |
| A question arrives, or is answered in the ticket | Badge and list update in place |

The tray is not modal: it does not trap focus and it does not dim the page. Clicking a control behind it closes it first, as a menu does.

## Accessibility

- The bell is a `<button>` with `aria-haspopup="dialog"`, `aria-expanded` and `aria-controls`; the tray is a labelled dialog, opened under the bell.
- Each row is one button whose name reads the ticket, the question and the time; the icon is decorative.
- The group label names the list, so a screen reader announces "Waiting on you, list, 3 items".
- Contrast: the `status-doing` icon is 5.8:1 on light and 7.7:1 on dark surface; the badge's `on-primary` numeral is 5.8:1 on `status-doing` in light and 8.4:1 in dark, both above 4.5:1.

## In egui

`notifications_button` draws the bell and opens the tray with `egui::Popup` at 420pt. Call it first in `header_controls`, before the screen's own controls, so it takes the rightmost slot in the right-to-left layout; clicking a row calls `open_issue` and the Reader scrolls to the inline question.
