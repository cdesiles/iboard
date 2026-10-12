# MarginComment

A note the author leaves on one block of the Reader, for the next AI review.

- The comment button sits in the sheet's right padding, level with the top of its block. It shows on hover in `ink-faint`, and stays in `review` once the block has comments, with a count when there are several.
- A click opens the composer under the block: a `review`-outlined field, then Comment (`⌘↩`) and Cancel (`esc`).
- A saved note is a `review-soft` card under its block: the note in `ink`, then `meta` saying whether a review has seen it, and a ghost Resolve.
- Notes whose block was edited away move to "Outdated comments" at the end of the sheet, under an excerpt of what they were on.
- Only Todo issues take comments, like the review itself.
