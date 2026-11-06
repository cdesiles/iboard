# Label

A short tag that groups issues, read straight from the title. `# [Discovery] Offline mode` gives the label **discovery**, and `# [BUG] Hover actions` gives **bug**. There is no separate label store: the file is the truth, and editing the title edits the labels.

## Parsing

- Only bracket groups at the **start** of the first `#` heading count: `[Bug] [UI] Title` and `[Bug][UI] Title` both give `bug` and `ui`. Brackets later in the title stay part of it (`Support [WIP] markers`).
- A label is the bracket text trimmed and lowercased, with spaces as hyphens, at most 25 characters: `[Tech debt]` gives `tech-debt`. Empty `[]` is ignored and repeats are merged.
- A `labels:` list in front matter, if a file has one, is merged in after the title's labels.
- Everywhere iboard shows the title (card, breadcrumb, Reader, ConfirmDialog, status bar), it shows it **without** the bracket prefix. The Editor shows the raw line, brackets included.
- New file names are slugged from the title without its labels: `[Bug] Fix login redirect` is written to `todo/fix-login-redirect.md`. Existing files are never renamed.

## Look

- **Neutral pill**: 18pt tall, `radius-pill`, padding 0×7 (6 before the dot), lowercase, `surface-sunken` fill, no edge, text in `meta` (Inter 12/16) `ink-muted`, and a 6pt dot in the label's colour 5pt before the text.
- **Colour**: the presets take the label palette, which has a light and a dark value each: `bug` `label-red`, `feature` `label-green`, `discovery` `label-teal`, `question` `label-ochre`, `chore` `label-grey` (also `label-orange` and `label-pink` for your own). Settings › Labels can give any label a `#rrggbb` of its own (see SettingsLabels); the dot uses it as is in both themes. A label with no colour has no dot. No blue (selection) and no violet (review) in the presets. Every preset dot is 4.5:1 or better on `surface-sunken`.
- **Loud**: a label marked "Stand out" in Settings › Labels (by default only `bug`) is a tinted pill instead: its colour as text, Medium, on its colour at `label-tint` (10% light, 16% dark) over `surface` (`theme::label_tint`), no dot. Preset text reads 4.7 to 5.0:1 (light) and 5.2 to 5.9:1 (dark) on the tint; for a custom colour, the text falls back to `ink` when the colour reads under 4.5:1. Keep it to what must stand out; grey is never loud.
- Hover: `surface-hover` fill, `ink` text, fading in over 100ms. Active filter: `accent` text on `accent-soft`, since blue means selected.

## Where it shows

- **IssueCard**: the first row, 4pt gaps, wrapping if needed. No row when there is no label.
- **Reader**: the first row of the sheet, above the `read-h1` title.
- **FilterField**: the suggestion chips are the three most used labels, and a picked chip becomes a token.
- **NewIssueModal**: `[bug] ` typed at the start of the title becomes a tag as you type.
- **LabelPicker**: from the card's Add label action or the Reader's Add label icon (`L`), pick board labels or type a new one; it writes `[label]` into the title, or into front matter `labels:` when the file has that list.

## Behaviour

Clicking a label (card or Reader) shows the board filtered by it, the same as picking it in the filter. It never opens the issue.

The consumer provides `labels(title, front_matter) -> Vec<String>`, `display_title(title) -> String` and the `[labels]` config. Emits LabelClicked → FilterSuggestionPicked.
