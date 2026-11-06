# QuickAdd

> **Replaced by NewIssueModal (31 Oct 2026).** The inline composer in the Todo column is too narrow for long notes and for an agent's draft, which it cut short. `⌘N` and the Todo `+` now open NewIssueModal. Its title states, keys, labels and naming rules below still apply there, on the sheet's title row; the column composer and its 8-line cap go.

An inline composer at the top of the Todo column. You describe the issue; iboard names it. `↩` writes the file at once and the title lands in the background, naming the file.

## Anatomy

- A card-shaped `surface` with the `accent` 2pt frame (the same "you are editing" signal as the Editor), padding `space-3` × `space-4`, `radius-md`.
- **Title row**, always reserved (22pt including a `line` rule under it), so no state makes the layout jump. `ui-strong`, at most 80 characters; a `meta` counter (`74/80`) appears from 70.
- **Description**, `ui-body` in `ink`, unbounded. It starts at 3 lines (54pt) and grows a line at a time.
- **Meta line**: the file that will be written, in `meta`, with the key hints on the right.

## States

| State | Title row | Description | Meta line |
| --- | --- | --- | --- |
| Idle | Placeholder "Title is named from the description" in `ink-faint`, and a `meta` tag naming the engine (`on-device` or `review agent`). Clicking the row lets you type a title yourself. | Focused, placeholder "Describe the issue". | `todo/` + "named in the background" in `ink-faint`; `↩ add`, `esc close`. |
| Typing | Unchanged. | Grows from 3 to 8 lines. | `↩ add`, `⇧↩ new line`. |
| Manual | Engine Off or unavailable. An empty line with a dashed `line` rule, placeholder "Add a title", tag "auto title off". | Unchanged. | "empty title uses the first line". |

## Keys

- `↩` never waits. With a title you typed, it writes the file under that title. With an empty title and an engine ready, it writes `todo/unnamed-ticket-xxxx.md` (xxxx random) and closes, so the next ticket can be written straight away. The title lands within 15s, with either engine, whatever the number of namings running: it renames the file and sets the heading, wherever the issue has moved to, and the card reads "Naming…" until then.
- `⌘↩` does the same and opens the Editor on the new file.
- `⇧↩` inserts a new line in the description. `esc` closes the composer. An empty description never creates a file.
- Saving is never blocked. With no title and no engine (Manual) the first line of the description, cut at a word boundary to 80 characters, becomes the title. On a timeout or a failure the issue stays `unnamed-ticket-xxxx`, as file name and title, with its text as written; quitting with a naming under way leaves the same.

## When the title lands

- A file renamed by hand, or a heading edited meanwhile, keeps what it has: the engine's title is dropped.
- The Reader, the selection and every panel open on the issue follow the rename, and its images come along. A pressed pointer holds the rename back until it lifts.
- The Editor is never disturbed: while it is open on the issue, or holds unsaved edits of it, the title waits; when it closes, the title lands unless the heading was edited.
- Meanwhile the card's title reads `unnamed-ticket-xxxx` and "Naming…" sits under it as its last row, in `meta` `ink-muted`, at rest too; no spinner, nothing to click.

## Labels

Typing `[bug] ` at the start of the title turns it into a Label tag as soon as the `]` is typed; `⌫` right after the tag turns it back into text. The file keeps the brackets (`# [Bug] Fix login redirect`), but the slug is made from the title without them: `todo/fix-login-redirect.md`. Titles from the engine never invent labels.

## What gets written

`todo/<slug>.md` containing `# <title>`, a blank line, then the description as typed. The slug is the title, lowercase ASCII with hyphens, at most 60 characters, with `-2`, `-3` on a collision (the meta line turns `status-doing` and shows the suffixed name). The new card appears just under the composer with an `accent-soft` fill that fades to `surface` after about 2 seconds, showing the title and the first lines of the description as its summary. The composer closes; `⌘N` opens a new one.

## A long description in a narrow column

The description stops growing at 8 lines (144pt), so the whole composer never exceeds about 210pt, roughly two cards. Past that it scrolls inside its frame, and its last line fades out to show there is more. The composer is sticky at the top of the Todo column, so the cards under it scroll beneath it and the other columns are not affected. Anything that needs more room than a quick note goes to the Editor with `⌘↩`, which writes the issue on the way; its title lands once the Editor closes. This keeps the board usable without a separate capture mode. A full-width capture sheet would be a follow-up, only worth doing if long notes turn out to be the usual case.

The consumer provides the Todo folder path, the engine state (`apple`, `agent`, `off`, or unavailable with a reason), `infer_title(description) -> Result<String>` (15s budget) and `create(title, description) -> Result<PathBuf>`. Emits ComposerOpened, TitleEdited, IssueCreated, TitleInferenceRequested, TitleInferred (the file renamed), TitleInferenceFailed (it stays unnamed), ComposerClosed.
