# Scrollbar

One overlay scrollbar for every scrolling pane (Reader sheet, Editor, a column, ColumnFocus, the RefinePanel reasoning, popup lists). The thumb floats over the content, 2pt inside the pane's right edge, and never takes layout width, so text never reflows when it appears or widens. There is no track and no arrow buttons.

## States

The pane is **focused** when it holds keyboard focus (or contains the focused element) and the window is active. Every other pane, and every pane of an inactive window, is **unfocused**.

| State | Thumb | When |
| --- | --- | --- |
| Rest | 4pt `line-strong` | Focused pane, not scrolling, pointer away. Always shown when the content overflows, so "there is more below" never depends on a gesture. |
| Scrolling | 4pt `ink-muted` | Wheel, trackpad, keys (`↑ ↓`, `space`, `⇧space`, `fn ↑ ↓`) or a jump (search, a new card). Goes back to rest 800ms after the last movement. |
| Pointer over | 8pt `ink-muted` | The pointer is inside the 12pt `scroll-hit` strip along the edge. Width grows towards the content, the right edge stays put. |
| Dragging | 8pt `accent` | Mouse down on the thumb, until mouse up, even when the pointer leaves the strip. |
| Unfocused | 4pt `line` | Another pane or app has focus. Passive: it only says where you are. Scrolling or pointing over it wakes it to the states above, without moving focus. |

- Content shorter than the pane: no thumb, no hit strip.
- The thumb length is the visible share of the content, at least `scroll-thumb-min` (24pt). Its position tracks the scroll offset.
- A click in the strip outside the thumb pages by 90% of the pane height towards the click. Clicking never moves keyboard focus.
- Width and colour change over 120ms; with reduced motion they switch at once. Nothing else animates.
- Contrast: rest, scrolling, pointer over and dragging all clear 3:1 against `surface` and `bg` in both themes. Unfocused uses the decorative `line` on purpose, as a dimmed state; it wakes on any interaction.

## Behaviour and events

Scrolling is view state, not a command: no event is emitted and the file is untouched. The Reader and Editor keep each issue's scroll offset for the session, so going back to the board and reopening the issue lands where you were.

## In the consumer

- **Web/CSS** (previews): wrap the scrolling element as `.ib-scroll > .ib-scroll__view` plus `.ib-scroll__rail > .ib-scroll__thumb`; the consumer places the thumb and toggles `is-scrolling`, `is-hover`, `is-drag`, `is-blur` and `is-short`.
- **egui**: `theme::scroll_area` in `theme/scroll.rs` wraps `egui::ScrollArea` with these widths and colours (floating bars, zero allocated width, the rail 2pt inside every edge) and tracks the scrolling state; pass whether the pane is focused. The pane's padding goes inside the scroll area with `content_margin(theme::scroll_margin(x, y))`, never around it, so the thumb sits on the pane's edge and text keeps `SCROLL_CLEAR` (14pt) from it.

The consumer provides the pane's focus state. Tokens: `scroll-thumb`, `scroll-thumb-hover`, `scroll-hit`, `scroll-thumb-min`.
