# QuickAdd

An inline composer at the top of the Todo column. You describe the issue; iboard names it. The title is inferred from the description when you press `↩`, and the title names the file.

## Anatomy

- A card-shaped `surface` with the `accent` 2pt frame (the same "you are editing" signal as the Editor), padding `space-3` × `space-4`, `radius-md`.
- **Title row**, always reserved (22pt including a `line` rule under it), so no state makes the layout jump. `ui-strong`, at most 80 characters; a `meta` counter (`74/80`) appears from 70.
- **Description**, `ui-body` in `ink`, unbounded. It starts at 3 lines (54pt) and grows a line at a time.
- **Meta line**: the file that will be written, in `meta`, with the key hints on the right.

## States

| State | Title row | Description | Meta line |
| --- | --- | --- | --- |
| Idle | Placeholder "Title is named from the description" in `ink-faint`, and a `meta` tag naming the engine (`on-device` or `review agent`). Clicking the row lets you type a title yourself. | Focused, placeholder "Describe the issue". | `todo/` + "named on save" in `ink-faint`; `↩ add`, `esc close`. |
| Typing | Unchanged. | Grows from 3 to 8 lines. | `↩ add`, `⇧↩ new line`. |
| Naming | "Naming…" in `ink-muted` with a pulsing 6pt `status-doing` dot. A beat, never a dialog; budget 3s. | Read-only, `ink-muted`. | `todo/naming`; `esc` stops and puts the cursor in the title. |
| Inferred | The suggested title in `ink`, fully selected so typing replaces it, and an **Inferred** tag (`accent` on `accent-soft`). Editing it drops the tag. | Unchanged. | The final file name in `status-done`; `↩ add`, `⌘↩ edit`. |
| Manual | Engine Off or unavailable. An empty line with a dashed `line-strong` rule, placeholder "Add a title", tag "auto title off". | Unchanged. | "empty title uses the first line". |
| Failed | Revealed with the cursor in it, placeholder "Type a title", rule in `status-doing`. | Kept as typed. | `status-doing` "Couldn't name it · ↩ uses the first line". |

## Keys

- `↩` with an empty title: infer (Naming), then stop on Inferred so you can check the name. `↩` again writes the file. With a title already there (typed, or inferred), `↩` writes straight away.
- `⌘↩` never stops: it infers if needed, writes, and opens the Editor on the new file.
- `⇧↩` inserts a new line in the description. `esc` closes the composer (or, while naming, skips to typing the title). An empty description never creates a file.
- Saving is never blocked. With no title (Manual, Failed, or a timeout) the first line of the description, cut at a word boundary to 80 characters, becomes the title.

## Labels

Typing `[bug] ` at the start of the title turns it into a Label tag as soon as the `]` is typed; `⌫` right after the tag turns it back into text. The file keeps the brackets (`# [Bug] Fix login redirect`), but the slug is made from the title without them: `todo/fix-login-redirect.md`. Inferred titles never invent labels.

## What gets written

`todo/<slug>.md` containing `# <title>`, a blank line, then the description as typed. The slug is the title, lowercase ASCII with hyphens, at most 60 characters, with `-2`, `-3` on a collision (the meta line turns `status-doing` and shows the suffixed name). The new card appears just under the composer with an `accent-soft` fill that fades to `surface` after about 2 seconds, showing the title and the first lines of the description as its summary. The composer closes; `⌘N` opens a new one.

## A long description in a narrow column

The description stops growing at 8 lines (144pt), so the whole composer never exceeds about 210pt, roughly two cards. Past that it scrolls inside its frame, and its last line fades out to show there is more. The composer is sticky at the top of the Todo column, so the cards under it scroll beneath it and the other columns are not affected. Anything that needs more room than a quick note goes to the Editor with `⌘↩`, which names and writes the issue on the way. This keeps the board usable without a separate capture mode. A full-width capture sheet would be a follow-up, only worth doing if long notes turn out to be the usual case.

The consumer provides the Todo folder path, the engine state (`apple`, `agent`, `off`, or unavailable with a reason), `infer_title(description) -> Result<String>` and `create(title, description) -> Result<PathBuf>`. Emits ComposerOpened, TitleInferenceRequested, TitleInferred, TitleInferenceFailed, TitleEdited, IssueCreated, ComposerClosed.
