# Board, holding ⌘

The board after ⌘ alone has been held for `hint-hold` 0.5s. Hints slide out over `motion-hint` 120ms and hide in one frame on release or when any other key is pressed with ⌘ (so `⌘N` itself never flashes them).

- Header: `⌘F` inside the filter, `⌘N` after New issue (in its label colour), and `⌘R` after Refresh when Auto is off and Refresh shows.
- Status bar, right: at most three shortcuts for the screen (`↩ open`, `E edit`, `⌘↩ next column`).
- Card actions keep their tooltips; no caps are added on cards.
- Layout does not move: a hint takes room that was empty (inside the field, after a label, at the status bar's right).

In egui: track `ui.input(|i| i.modifiers.command_only())` and the time it became true; show hints once it has been true for `theme::HINT_HOLD` with no key pressed since.
