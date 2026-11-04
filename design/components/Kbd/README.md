# Kbd

A keyboard hint in `meta` (mono 11pt) with a `line-strong` key outline.

- Use the macOS glyphs: ⌘ ⌥ ⇧ ⌃ ↩ ⌫ ⇥ and the arrows, and the word `esc`. JetBrains Mono draws all of them; never add a symbol it lacks.
- Show hints in the status bar, inside the filter, beside buttons and in footers only while ⌘ is held: after half a second of ⌘ alone they slide out (120ms), and letting go or pressing anything with ⌘ puts them away. Tooltips and menus always show their keys, and so do the RefinePanel's numbered choices, whose number is their name. Never more than three in the status bar at once.
