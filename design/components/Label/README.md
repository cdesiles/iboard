# Label

A short tag that groups issues, read straight from the title. `# [Discovery] Offline mode` gives the label **discovery**, and `# [BUG] Hover actions` gives **bug**. There is no separate label store: the file is the truth, and editing the title edits the labels.

## Parsing

- Only bracket groups at the **start** of the first `#` heading count: `[Bug] [UI] Title` and `[Bug][UI] Title` both give `bug` and `ui`. Brackets later in the title stay part of it (`Support [WIP] markers`).
- A label is the bracket text trimmed and lowercased, with spaces as hyphens, at most 25 characters: `[Tech debt]` gives `tech-debt`. Empty `[]` is ignored and repeats are merged.
- A `labels:` list in front matter, if a file has one, is merged in after the title's labels.
- Everywhere iboard shows the title (card, breadcrumb, Reader, ConfirmDialog, status bar), it shows it **without** the bracket prefix. The Editor shows the raw line, brackets included.
- New file names are slugged from the title without its labels: `[Bug] Fix login redirect` is written to `todo/fix-login-redirect.md`. Existing files are never renamed.

## Look

- 18pt tall, `radius-pill`, padding 0×7, lowercase, the `ui-strong` face at 11pt.
- A label with a colour (the presets `discovery`, `bug`, `feature`, `chore` and `question`, or one set in Settings › Labels) is filled with a soft tint of it over `surface`, and written in the colour itself when that reads at 4.5:1, else in `ink`.
- A label with no colour: `ink-muted` on `surface-sunken`.
- Hover, or the label is the active filter: `accent` text and edge on `accent-soft`.

## Where it shows

- **IssueCard**: a row above the title, 4pt gaps, wrapping if needed. No row when there is no label.
- **Reader**: a row between the sheet's head and the `read-h1` title.
- **FilterField**: the suggestion chips are the three most used labels, and a picked chip becomes a token.
- **QuickAdd**: `[bug] ` typed at the start of the title becomes a tag as you type.

## Behaviour

Clicking a label (card or Reader) shows the board filtered by it, the same as picking it in the filter. It never opens the issue.

The consumer provides `labels(title, front_matter) -> Vec<String>` and `display_title(title) -> String`. Emits LabelClicked → FilterSuggestionPicked.
