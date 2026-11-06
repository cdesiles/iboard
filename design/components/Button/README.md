# Button

Buttons are 24pt-tall verbs. The label identifies a text button (WCAG 1.4.11 asks 3:1 only for what identifies a control), so buttons carry no `line-strong` edge. Where a button sits decides its edge:

| Where | Secondary button at rest | Hover |
| --- | --- | --- |
| Header bar (Board, Refresh, the AI split, `…`) | No edge, no fill | `surface-hover`, fading in over `motion-hover` 100ms |
| Modal foot, ConfirmDialog, dialogs, content blocks (Not written yet) | `line-soft` hairline, no fill | `surface-hover`, same fade |
| Card actions | No edge, icon only, tooltip | `surface-hover` |

- **Primary**: `primary` fill (`ink`), `on-primary` label, `primary-hover` under the pointer. **One filled button per screen**: New issue on the Board, Save in the Editor, the commit button in a modal foot. The Reader has none. The Button card shows the `accent` fill beside it for comparison (`.ib-btn--primary-accent`); with ink, blue only means selected, focused or a link.
- **Secondary** (default): as the table says. Use for Refresh, Cancel, Close.
- **Back**: the header's first control: chevron and "Board", no edge. `esc` slides out beside it while ⌘ is held.
- **Review / AI split**: `review` label and icon, no edge in the header, `review-soft` on hover; a `line-soft` divider between the face and its chevron. See HeaderBar for the menu.
- **Dynamic icon**: Edit, Add label and Delete in the Reader and Editor header. A 24×24 icon in `ink-muted` at rest; on hover or focus its label grows to the left in a pill over the neighbours (paint only, nothing reflows); while ⌘ is held its key follows the icon. Delete's pill is `danger` on `danger-soft`. See HeaderBar › Dynamic icon buttons; in egui `theme::dynamic_button`.
- **More (`…`)**: a 24×24 icon button in `ink-muted`, no edge; opens Move to… (and Discard in the Editor).
- **Danger**: `danger` fill, `on-danger` label. Only as the confirm button inside ConfirmDialog. Delete is never drawn in `danger` at rest anywhere else: in the Reader and Editor it is a dynamic icon that turns red only under the pointer or focus. (The v1 "danger outline" button is retired.)
- **Ghost**: no border, `ink-muted` label. For the quiet action beside a louder one: icon buttons inside a row of other buttons (dialog close), and labelled secondary actions (Cancel in a comment, Resolve, Reset, Discard, End refining, Undo draft, Dismiss, Refresh and paging on the Stats screen). Never for going back to another screen, which is the Back button's.
- **Key hints** are hidden at rest. While ⌘ is held they slide out after the label in `meta-mono`, `ink-muted` (on the primary, its label colour at 72%). Tooltips always name the key.
- Icon-only buttons are 24×24 and need an accessible name and a tooltip with the shortcut.
- Label in `ui-strong` at 12pt; icon 14pt line, 6pt gap.
- Focus: the 2px `focus` ring outside the button, at once (no fade).
- Disabled keeps the shape, drops to `ink-faint` (a disabled primary takes `surface-sunken`); never hide a button that will become available.

In egui: `theme::button(ui, p, ButtonKind::{Header, Secondary, Primary, Review, Danger}, text)`. The consumer provides the label, an optional icon and the shortcut for the tooltip.
