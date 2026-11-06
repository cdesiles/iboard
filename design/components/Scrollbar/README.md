# Scrollbar

One overlay scrollbar for every scrolling pane (Reader sheet, Editor, a column, the RefinePanel reasoning, popup lists). The thumb floats over the content, 2pt inside the pane's right edge, and never takes layout width, so text never reflows when it appears or widens. There is no track and no arrow buttons.

## States

Hidden at rest, like macOS overlay scrollbars: a pane shows no thumb until something moves.

| State | Thumb | When |
| --- | --- | --- |
| Rest | none | Not scrolling, pointer away from the edge. Whether there is more below is told by the content cut at the pane's edge, not by a bar. |
| Scrolling | 4pt `scrollbar` | Wheel, trackpad, keys (`↑ ↓`, `space`, `⇧space`, `fn ↑ ↓`) or a jump (search, a new card). Fades in over `scroll-fade` 150ms, stays `scroll-linger` 800ms after the last movement, then fades out. |
| Pointer at the edge | 8pt `scrollbar-active` | The pointer enters the 12pt `scroll-hit` strip along the edge: the thumb appears at once at 8pt (width grows towards the content, the right edge stays put). It stays while the pointer is in the strip, then lingers like scrolling. |
| Dragging | 8pt `accent` | Mouse down on the thumb, until mouse up, even when the pointer leaves the strip. |

- Focus no longer changes the thumb: the v1 "unfocused" `line` state is gone, since a hidden bar has nothing to dim.
- Content shorter than the pane: no thumb, no hit strip.
- The thumb length is the visible share of the content, at least `scroll-thumb-min` (24pt). Its position tracks the scroll offset.
- A click in the strip outside the thumb pages by 90% of the pane height towards the click. Clicking never moves keyboard focus.
- Width and colour change over 120ms and the thumb fades over 150ms; with reduced motion all of it switches at once.
- Colour: light `scrollbar` #bfc4cb (1.75:1 on `surface`) while scrolling and `scrollbar-active` #a1a7b0 (2.4:1) at the edge, faded greys like Chrome's; dark #737c89 / #a7aeb9 (3.9:1 and 7.3:1). Position is a cue, never the only one, since keys and the pointer always scroll.

## Behaviour and events

Scrolling is view state, not a command: no event is emitted and the file is untouched. The Reader and Editor keep each issue's scroll offset for the session, so going back to the board and reopening the issue lands where you were.

## In the consumer

- **Web/CSS** (previews): wrap the scrolling element as `.ib-scroll > .ib-scroll__view` plus `.ib-scroll__rail > .ib-scroll__thumb`; the consumer places the thumb and toggles `is-scrolling`, `is-hover`, `is-drag` and `is-short` (no class at rest: the thumb is transparent).
- **egui**: `theme::scroll_area` in `theme/scroll.rs` wraps `egui::ScrollArea` with these widths and colours (floating bars, zero allocated width, the rail 2pt inside every edge): the dormant and active handle opacity follow the linger and fade, and egui's interact state (pointer on the bar) shows the 8pt thumb. The pane's padding goes inside the scroll area with `content_margin(theme::scroll_margin(x, y))`, never around it, so the thumb sits on the pane's edge and text keeps `SCROLL_CLEAR` (14pt) from it.

Tokens: `scrollbar`, `scrollbar-active`, `scroll-thumb`, `scroll-thumb-hover`, `scroll-hit`, `scroll-thumb-min`, `scroll-linger`, `scroll-fade`.
