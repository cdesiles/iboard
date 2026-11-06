# BoardScreen

The board at rest, the screen iboard opens on. It is the reference for "calm but dense": four columns on a 14" MacBook Pro, nine cards visible, one filled button.

## What it shows, once each

| Where | What |
| --- | --- |
| Header bar | Traffic lights on the centre line (y 20), the project name (opens the ProjectMenu), the filter, **New issue** (the one `primary` fill). Refresh shows only with Auto off |
| Column heads | Status icon, name in title case, count |
| Cards | Labels (neutral pills with a dot; `bug` tinted), title (2 lines), summary (2 lines), rows 8pt apart; Done adds its date |
| Hovered or selected card | File name and actions, in a strip over the card's bottom |
| Status bar | The issues folder |

Colour on this screen: the ink New issue button, four 14pt status icons, label dots, one red `bug` pill, and blue only on the selected card and the focused filter. No key hints, no scrollbars, no file names at rest.

## Removed from v1

- The issues path in the header (now the status bar), the "9 issues" total in the status bar (the column counts say it), the file name on every card, the 2pt coloured rule under each column title, the uppercase column titles, `line-strong` edges on header buttons and labels, the blue New issue fill.

See **Board, holding ⌘** (BoardHoldingCmd) for the same screen with key hints shown.
