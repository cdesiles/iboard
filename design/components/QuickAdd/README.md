# QuickAdd

An inline composer at the top of the Todo column that turns one line of text into a new ticket file.

- Opened by `⌘N` from any screen, the board header's "New ticket" button, or the `+` in the Todo column header. It takes focus immediately.
- A card-shaped `surface` with an `accent` 2pt frame (the same "you are editing" signal as the Editor), a borderless title field in `ui-strong`, and a `meta` line showing the file that will be written.
- `↩` writes `todo/<slug>.md` containing `# <title>`, inserts the new card directly under the composer with an `accent-soft` fill that fades to `surface` after about 2 seconds, and clears the field for the next ticket.
- `⌘↩` writes the file and opens it in the Editor with the cursor on the line after the title.
- `esc`, or clicking elsewhere with an empty field, closes it. Clicking elsewhere with text keeps the composer and its text; nothing is created without `↩`.
- The meta line turns `status-doing` when the slug collides and shows the suffixed name (`-2`) that will be used.
- A write failure shows a `danger` line in place of the meta line, naming the folder and keeping the typed title.

The consumer provides the Todo folder path and a `create(title) -> Result<PathBuf>` that does the slug, collision suffix and write. Emits ComposerOpened, TicketCreated, ComposerClosed.
