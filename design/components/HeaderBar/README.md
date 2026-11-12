# HeaderBar

The one top bar: `bar-h` 40pt, `bg` ground, a `line` rule below. It holds where you are on the left and the screen's few controls on the right. Header buttons have no edge at rest; `surface-hover` fades in under the pointer (see Button).

## Window and traffic lights

- The window's title bar is transparent and the header bar sits in it. The macOS traffic lights sit on the bar's **centre line, 20pt from the window's top** (`titlebar-light-y`): three 12pt lights with centres at x 20, 40 and 60 (`titlebar-light-x`, then every 20pt), so they span y 14 to 26. Every crumb, button and the filter centres on the same line.
- AppKit puts the buttons at its default (centred on a 28pt title bar, y 14), which is why they sat high on the 40pt bar. The app moves the standard window buttons to `theme::traffic_light_centres()` after creating the window and again after every resize and full-screen exit, since AppKit resets them.
- Nothing is drawn or clickable in the leading `titlebar-lead` 80pt: the project name, or the Board button, starts at 80. The bar's empty space drags the window, and a double-click on it zooms, as a native title bar would. Every screen keeps this bar, Stats included, at the same height so nothing jumps.

## Board

- Left: the project name in `ui-title` with a chevron; it opens the ProjectMenu. The issues path is no longer here: it is in the status bar.
- Right: FilterField (220pt, 480pt while in use, growing to the left; its `line-strong` edge stays, since a text field is identified by it), Design (only when `idesign` is on, on a single project's board), Refresh (`⌘R`, only when Settings › General turns Auto off), **New issue**, the board's one filled button (`primary`), then the notification bell at the very right. Stats is not in the header: it opens from the ProjectMenu footer or `⌘I`.

## Notifications

- The bell is the header's **last** control on every screen, at the very right: an 18pt bell in a 24pt hit area, `ink-muted`, no edge at rest, 8pt after the control before it. Board: after New issue; Reader: after its actions; Editor: after Save; Diff and Stats: after their own controls.
- It opens the Notifications tray. The count badge, the question rows under `Waiting on you`, the empty state, the keys and the focus behaviour all live in Notifications. The bell is not one of the dynamic icon buttons and has no key hint.
- **No header badge.** No count sits beside New issue: the bell's badge is the one count, and a card keeps its own note. See IssueCard.

## Reader and Editor

- Left: **Board** (chevron and label, no edge), a 16pt `line` divider, then the breadcrumb `iboard / ○ Todo`: the column carries its status icon. The issue title joins the breadcrumb as its last segment (`ui-strong`, `ink`, ellipsis, without its `[label]` prefix) only once the page's `read-h1` has scrolled out of view, and always in the Editor, where the title line is raw text.
- Breadcrumb links are `ink-muted`, 24pt tall hit areas, with a `surface-hover` fill and an underline on hover. Every segment but the last is a link.
- Right, left to right:
  1. **Edit, Add label, Delete** as dynamic icon buttons (below). Edit only in Todo and Ready (Doing and Done open read-only; Done shows Commits instead). Add label only on Todo and Ready issues (labels are settled once work starts). Delete not on Done issues, which are archived, never deleted. The Editor keeps only Delete, without a key, since `Del` edits text there.
  2. **The AI split button**: `review` label, no edge, `review-soft` on hover. Its face follows the column. **Todo**: Refine (`⌘R`); the chevron opens Refine with a challenger (`⌘R`), Review this issue (`⌘⇧R`), the other review scope as a one-off (Deep review or Quick review) and Ask… (`⌘K`). Reviewing refines an issue, so Refine and Review belong to Todo only. **Ready, Doing and Done**: no AI split button; on Done Archive sits right after the icons. Every menu ends with the current tool and model, which opens Settings (`⌘,`). See ReviewButton for running and Review ready.
  3. **The issue's actions**: header buttons, no edge at rest, shown only where they apply. Left to right: Refresh (`⌘R`; only when Auto is off, and with no key on Todo, where `⌘R` refines), Agent log (only when the issue has one; toggles the panel), Draft design (only with `idesign` on, not on Done) and Archive (Done only). There is no `…` menu: every option is one click away.
  4. **Move to…**: Reader only, a secondary text button showing `M`. Its menu lists the other columns, numbered 1–9, and the standard columns the project lacks; picking one, or typing its number after `M`, sends the issue there. `esc` closes it.
  5. **Editor only**: Save (`⌘S`), the Editor's one filled button. The Editor has none of the Reader's actions. The Unsaved pill is in the status bar.
  6. **The notification bell**, at the very right on both screens: after the Reader's actions and after Save in the Editor. See Notifications.
- Menus always show their keys, right-aligned in `meta-mono` `ink-faint`. Refresh leaves the Reader header: with Auto on (the default) the file watcher refreshes; with Auto off it is one of the issue's actions.

### Dynamic icon buttons

Edit (`E`), Add label (`L`) and Delete (`Del`) stay visible, so the actions people use most are one click away, but take only 24pt each at rest.

| State | Looks like |
| --- | --- |
| Rest | The 14pt line icon in `ink-muted`, no edge, no fill. 2pt apart, 8pt before what follows: the AI split on Todo, the issue's actions elsewhere. |
| Pointer on it, or keyboard focus | The icon takes the fill and a pill grows to the **left of the whole row**, 2pt before its first icon, over `motion-hint` 120ms, holding the label in `ui-strong` 12pt: "Edit", "Add label", "Delete". Fill `surface-hover` and text `ink` (Delete: `danger` on `danger-soft`, the only red in the header). The pill is paint only and covers none of the icons, which all stay in sight: the icon never moves under the pointer and nothing reflows, and moving the pointer to the next icon switches labels at once. Focus adds the 2px ring round pill and icon. No tooltip, since the pill is the label. |
| ⌘ held | The key slides out after each icon, in layout, in `meta-mono` (`E`, `L`, `Del`), like every other hint. |
| ⌘ held, pointer on it | Label, icon and key. |

Leaving collapses the pill at once. With reduced motion it appears and goes in one frame. In egui: `widgets::DynamicButton`.
- In the Editor, `esc` asks first when there are unsaved changes.

## Key hints

Hidden at rest. While ⌘ is held: `esc` beside Board, `⌘F` in the filter, `⌘R` beside Refresh (when shown), `⌘N` beside New issue, `⌘R` beside the AI face (Todo), `⌘S` beside Save.
