# IssueGone

What the Reader shows when the issue it has open is no longer at its path. It replaces the one grey line "This issue no longer exists. It may have been moved or deleted." in a bare card, which read as a crash at the worst moment: you were following an agent's log, the build finished, the file moved from `doing/` to `done/`, and the screen went from your work to an error.

## What changed, and why

| Before | After | Why |
| --- | --- | --- |
| One sentence that guesses: "moved or deleted". | The notice says which. The app looks for the same file name in the other columns: found, it names the column and both folders; not found, it says so. | A move is good news and a deletion is not. The panel should not make you work out which. |
| A dead end. The only way out is the Board button in the header. | **Open in Done**, the one filled button, takes you to the file where it now lives. **Back to board** stays beside it. | The feeling of "something broke" goes when the next step is one key away. |
| A bare card with 32pt of nothing around a muted line. | A picture, a title with the column's own icon, one sentence, the actions. | It looks like something iboard meant to say. |
| The agent log, if it was open, disappears with the issue. | The log panel stays beside the notice, and **Open in Done** keeps it open on the file. | The log is what you were reading; the move is what interrupted it. |

## Three cases

| Case | Picture | Title | Sentence | Actions |
| --- | --- | --- | --- | --- |
| **Moved to Done** | A card drifting into the last of four strips, where a green check waits (`issue-moved-*.jpg`) | "Moved to Done", with the Done icon in `status-done` | "**file.md** moved from doing/ to done/." | **Open in Done** `↩` (primary), Back to board `esc` |
| **Moved to another column** | None | "Moved to Ready", with that column's icon in its status colour | "**file.md** moved from todo/ to ready/." | **Open in Ready** `↩` (primary), Back to board `esc` |
| **Not found** | An empty card-shaped slot pressed into the third strip (`issue-gone-*.jpg`) | "Issue not found", no icon | "**file.md** is no longer in any column. It was deleted, or renamed outside iboard." | **Back to board** `esc` (primary) |

- Only Done has a picture, because only Done is the common arrival: a build finishes. The other moves are a person dragging a card in another window, and a line and a button are enough.
- A move to a folder that is not one of the four standard columns names the folder: "Moved to archive/".
- When the agent log is open on the issue, a second line follows the sentence in `meta`: "Its agent log is still open beside it." Without a log, there is no second line.
- Not found is not an error in the `danger` sense. Nothing failed to write, so nothing is red.

## Anatomy

- One `surface` card, `line-soft` edge, `radius-md`, at most `reader-max` (624pt) wide, centred in the issue pane, `space-6` below the header. It is the page the issue would have been, so it sits where the sheet sits.
- **Picture.** Full width at the top, 7:2, cropped by `object-fit: cover`, with a `line-soft` rule under it. About 624 × 178pt. No text inside, so nothing to translate or to keep in step with the copy. Decorative: `alt=""`.
- **Body.** `space-6` above, `space-7` to the sides and below. Title, sentence and actions are `space-5` apart.
- **Title.** `read-h2` 16/22, `ink`, with the column's 14pt status icon `space-3` before it. The icon repeats the Done icon of the board, so the word and the shape say the same thing.
- **Sentence.** `ui-body` 13/18, `ink-muted`, at most `read-measure` wide. The file name is the only part in `ink`, at 500, and breaks anywhere rather than widening the card.
- **Actions.** `space-3` apart, wrapping. The primary is the screen's one filled button and takes focus when the notice opens, so `↩` follows the issue. `esc` is Back to board, as on every Reader.
- The header does not change: Board and the project crumb. The issue is gone, so there is no breadcrumb for it.

## Two themes, two pictures

The picture is a raster, so it carries its own theme. Each case ships a light and a dark version, the same composition recoloured to the `surface-sunken` and `surface` family of its theme, with the same green. The card shows the one that matches the theme: `ib-gone__art--light` and `ib-gone__art--dark`, swapped by `[data-theme="dark"]`. The files are generated, not drawn, and live in `files/`. Redraw them together, so the two themes never drift apart.

## Layout with a side panel

- With the agent log or the Review panel open, the notice sits in the issue pane and the panel stays where it was, 360pt. The pane is narrower, so the card shrinks with it. Under `reader-max` the picture keeps its 7:2 ratio and crops.
- Stacked, when the window is too narrow for both (see PaneDivider), the notice takes the top half.
- With no panel, the card is centred in the full width.

## Behaviour

- The notice appears the frame after the path stops resolving. It does not wait for a refresh.
- Which case it is comes from one lookup: the file name in the other columns of the same project, in column order. If two columns hold it, the one furthest right wins.
- **Open in Done** opens the Reader on the new path, and carries the agent log or Review panel with it when one was open on the old path. It never reopens one that was closed.
- If the file reappears at the old path (an editor's atomic save, a git checkout), the notice leaves in one frame and the issue is back, with the scroll position it had.
- Nothing animates. The picture is still.

## Accessibility

- The card is a `section` named by its title. The title is the first thing a screen reader reads, then the sentence, then the actions.
- The picture is decorative and hidden from assistive technology. The title and the sentence carry everything it says.
- Text contrast follows the system: `ink` and `ink-muted` on `surface` are 6.4:1 or more. The status icons are 3:1 or more on `surface` and are always beside their word.
- The primary button's focus ring is the system's solid 2pt `focus` ring, outside the button.

## Content

- Sentence case, no trailing ellipsis, no emoji, no exclamation mark: "Moved to Done", not "Done!".
- The title names the column the way the board does ("Done", "Ready"); the sentence names the folders the way the filesystem does (`doing/`, `done/`).
- Each fact once: the file name appears in the sentence only, the destination in the title and the button only.
