# FilterField

The board's filter in the header: compact at rest, wide while you use it, with one-click keyword suggestions for the filters people use most.

## Width

- **Rest:** 220pt with a search icon, the placeholder "Filter issues" and the `⌘F` hint.
- **Focused, or a filter is on:** 480pt. It grows to the left into the header's empty space, so the controls on its right (Auto, Refresh, New issue) never move. On a narrow window it takes what is left between the breadcrumb and Auto, never less than 220pt; the breadcrumb path truncates first.
- **Focus leaves with an empty field:** it shrinks back to 220pt. With a filter on, it stays at 480pt with an `accent` edge, so a filtered board is always obvious.
- The width change takes 120ms and is skipped when reduced motion is on: the field jumps to its width in one frame (see Motion in `design/README.md`).

## Suggestions

- When the field is focused, the right end shows "Labels" and one chip for each of the three labels used most on the board (for example **bug**, **feature**, **discovery**), read from the `[Brackets]` in titles. Chips are 18pt pills in `meta`, `line-strong` edge; on hover they take `accent` on `accent-soft`.
- Clicking a chip (or `↓` to reach the chips, `← →` between them, `↩` to pick) turns it into a token at the start of the field: `bug ×` in `accent` on `accent-soft`. Focus stays in the field so you can keep typing.
- A label token matches issues that carry that label exactly (from `[Bug]` in the title, or a `labels:` list in front matter). Clicking a label on a card or in the Reader adds the same token. Free text after a token narrows further: tokens and text must all match.
- Picked suggestions leave the chip row; the others stay. When the row would not fit next to your text, it hides and comes back when the text is cleared.
- To pin the suggestions instead, set `[filter] suggestions = ["bug", "feature"]` in `~/.config/iboard/iboard.toml`; keep it to two or three.

## Feedback

- While a filter is on, the field shows the match count in `meta` ("5 of 14"), and each column count becomes "2/6" with the total in `ink-faint`.
- A column with no match shows its empty well with "No issues labelled bug".

## Keys

- `⌘F` focuses and widens. `esc` clears text and tokens; a second `esc` leaves the field. `⌫` in an empty field removes the last token. `↓` moves to the suggestion chips.

The consumer provides the suggestion list and the match function. Emits FilterFocused, FilterSuggestionPicked, FilterChanged, FilterCleared.
