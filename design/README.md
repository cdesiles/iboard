Roadmap Viewer is a desktop tool for developers who keep their roadmap as markdown files next to the code. Four folders (`todo/`, `ready/`, `doing/`, `done/`) are four columns; one file is one ticket. Tickets flow left to right: Todo holds ideas that may still need refining, Ready holds refined tickets queued for an agent (a `001-` prefix is the priority), Doing is what someone is working on, Done is finished. The interface should feel like an editor panel, not a project-management suite: dense, quiet, fast, and readable at a glance on a MacBook Pro.

## Principles

- **The file is the truth.** Every screen shows what is on disk. Name things the way the filesystem does (`todo/ai-assist.md`), never with an invented ID.
- **Quiet chrome, loud content.** Navigation and controls use `ink-muted` and `line-strong`; only ticket text uses `ink`. Color is spent on state (column, selection, unsaved, review), never on decoration.
- **Small but visible.** Controls are 24pt tall, yet every control edge uses `line-strong` (at least 3:1 on every surface) and every text token clears 4.5:1. Never shrink a control below `control-h` or set text in `line` or a lighter grey.
- **Keyboard first, pointer complete.** Every action has a shortcut shown next to it, and every shortcut has a visible control.

## Content

- Sentence case everywhere except column titles (`TODO`, `DOING`, `DONE` in `ui-label`).
- Buttons are verbs: New ticket, Refresh, Save, Review, Discard. No "OK", no ellipses unless a dialog follows.
- Counts are bare numbers in `meta`. "4 tickets", never "4 ticket(s)".
- Empty states say what goes there: "No tickets in doing/" rather than "Empty".
- Errors name the file and the fix: "Couldn't save todo/ai-assist.md: the file changed on disk. Reload or keep your version."
- No emoji anywhere in the UI.

## Color

- `bg` is the window ground; `surface` is anything you can click into (card, reader page, editor, input). Do not stack `surface` on `surface` without a `line` border.
- Text: `ink` for content, `ink-muted` for secondary text (summaries, path, counts), `ink-faint` only for placeholders and separators.
- Control edges: `line-strong`. Card edges and dividers: `line`. A card is an object, not a control, so its rest border can be soft; on hover it takes `line-strong`, when selected `accent`.
- Selection and focus: `accent`. Focus is always a solid 2px `focus` ring outside the element, never a color change alone.
- Columns carry a status color as a 2px rule under the column title plus an 8px dot: `status-todo`, `status-ready`, `status-doing`, `status-done`. The column word is always shown, so color is never the only signal.
- `review` is reserved for the AI review feature. Nothing else in the app may use it, so the user learns that this hue means "an agent is talking".
- `danger` only for write failures, file conflicts and the Block verdict.

## Type

- Interface in IBM Plex Sans, code and file names in IBM Plex Mono. Bundle both TTFs in the binary; fall back to the macOS system faces (`-apple-system`, `SF Mono`).
- Interface sizes: `ui-title` 15, `ui-body` and `ui-strong` 13, `ui-small` 12, `ui-label` and `meta` 11. Nothing below 11pt.
- Reading sizes are one step larger than the UI: `read-h1` 20, `read-body` 14/22, at most 68 characters per line (`reader-max`).
- The editor uses `edit-body` (mono 13/20) so markdown syntax lines up and is obviously "raw".

## Space and layout

- 4pt base: `space-1` 2 … `space-7` 32. Window gutter and column gap `space-5` (16). Gap between cards `space-3` (8). Card padding `space-4` × 10.
- Columns are equal width between `column-min` 280 and `column-max` 420. On a 14" MacBook Pro (1512pt wide) four columns sit near 360; with fewer folders they cap at 420 and the spare room stays empty on the right instead of stretching cards.
- Window skeleton, top to bottom: header bar (`bar-h` 40) · board or page · status bar (`statusbar-h` 24).
- Radii: `radius-sm` 4 for controls, `radius-md` 6 for cards and pages, `radius-pill` for 18pt pills. Cards are flat; `shadow-popover` is only for the review panel and menus.

## Navigation

Navigation is two discreet pieces, both in `ink-muted`:

1. **Breadcrumb in the header**: `roadmap-viewer / Todo / AI assist to refine a ticket`. Every segment but the last is a link back. On the board it shows only the project name and path.
2. **Status bar at the bottom**: the current file path on the left, save state and the two or three shortcuts that apply to the current screen on the right.

There is no sidebar and no tab bar. Three screens are enough: Board, Reader, Editor.

## Adding a ticket

Capturing an idea must take one shortcut and one line of text. Writing the rest can wait for the editor or the AI review.

- **Two entry points, one composer.** The "New ticket" button in the board header (primary, `⌘N`) and the `+` icon button in the Todo column header both open the same inline composer at the top of the Todo column. `⌘N` works from every screen: from the Reader or Editor it returns to the board first.
- **The composer is a card**, not a dialog: a single title field on `surface` with an `accent` frame, and under it the file it will create in `meta` (`todo/fix-login-redirect.md`), updated as you type.
- **`↩` creates and stays open.** The file is written with `# <title>` as its only content, the new card appears just below the composer, and the field clears for the next idea. This makes brain-dumping five tickets a matter of five lines.
- **`⌘↩` creates and opens the Editor** on the new file, for when you want to write the description now.
- **`esc` closes** the composer; an empty composer never creates a file.
- **File name** is the title slugified (lowercase ASCII, hyphens, at most 60 characters). If the file exists, append `-2`, `-3`; the meta line shows the final name before you press `↩`.
- New tickets always land in Todo. Creating directly in Doing or Done is not offered; move the card afterwards.

| Key | Board | Reader | Editor |
| --- | --- | --- | --- |
| `↑ ↓ ← →` | Move selection between cards and columns | Scroll | Cursor |
| `↩` | Open selected ticket | Edit | New line |
| `E` | Edit selected ticket | Edit | |
| `esc` | Clear filter | Back to board | Leave editor (asks if unsaved) |
| `⌘S` | | | Save to file |
| `⌘N` | New ticket in Todo | New ticket (returns to board) | New ticket (asks if unsaved) |
| `⌘F` | Focus filter | | |
| `⌘R` | Refresh from disk | Refresh | |
| `⌘[` | | Back | Back (asks if unsaved) |
| `⌘⇧R` | Review selected (future) | Review this ticket (future) | Review draft (future) |

## Interaction model

Spec-first, as commands and events (event-modeling style). A screen only changes in response to an event.

| Screen | Command (user) | Event | Next screen |
| --- | --- | --- | --- |
| Board | Select card | TicketSelected | Board, card selected |
| Board | Open ticket (click, `↩`) | TicketOpened | Reader |
| Any | New ticket (`⌘N`, header button, Todo `+`) | ComposerOpened | Board, composer focused in Todo |
| Board | Type title, `↩` | TicketCreated | Board, new card under the composer, composer cleared |
| Board | Type title, `⌘↩` | TicketCreated → EditStarted | Editor on the new file |
| Board | `esc` in composer | ComposerClosed | Board |
| Reader | Click description, `E`, `↩` | EditStarted | Editor, cursor at click point |
| Editor | Type | DraftChanged | Editor, Unsaved pill |
| Editor | Save (`⌘S`, or auto on leave when Auto is on) | TicketSaved / SaveFailed | Reader / Editor with error |
| Any | Refresh, or file watcher fires | TicketsReloaded | Same screen; open draft kept, conflict flagged |
| Any | Move to column (drag, or menu) | TicketMoved | Same screen; file moved between folders |
| Reader, Editor | Review (future) | ReviewRequested → ReviewReceived | Review panel opens beside the page |

## Iconography

Line icons at 14pt, 1.6pt stroke, `currentColor`, drawn on the 24pt control grid: refresh, back chevron, edit pencil, plus, review, trash, archive. The repo has no icon set yet; use Lucide (MIT) glyphs of the same names, and keep any icon paired with a word except the back chevron and the per-card actions (trash, archive), which carry a tooltip.

## AI review (reserved slot)

A future Review button asks an agent (OpenCode or a Claude agent) to challenge the open ticket. Its place and look are fixed now so the layout does not move when it ships:

- **Placement**: the rightmost control of the header on Reader and Editor screens, after Refresh; on the Board, in the selected card's hover row. Always labelled "Review", with `review` text and border on `surface`.
- **What it returns**: three findings, always in this order, each with a verdict pill and one or two sentences.
  - *Relevance*: is this still worth doing, does it duplicate another ticket?
  - *Size*: can one person or one agent finish it in one sitting, or should it split? It proposes the split.
  - *Clarity*: could someone implement it without asking a question? It lists missing acceptance criteria, files, or decisions.
- **Verdicts**: Pass (`status-done`), Challenge (`review`), Block (`danger`), always with the word.
- **From an idea to a ticket**: when the ticket is only a title, the review drafts the missing sections (Context, Acceptance criteria, Out of scope, Open questions) as a proposal. The user applies it into the editor draft; the agent never writes the file itself.
- **States**: idle, running (button shows "Reviewing…" and a cancel affordance), result (panel open), failed (inline `danger` text with retry).

## Using the tokens from Rust

Tokens are points, which map 1:1 to egui points and to CSS px on a Retina display. The header font (Ubuntu) and widget shapes in the screenshot look like egui's defaults, so the companion `theme.rs` in the project files maps these tokens onto `egui::Visuals` and `egui::Style`. If the app uses iced, Slint or Tauri instead, the same `tokens.json` drives it; only the mapping file changes.

egui has no font weights: register `IBMPlexSans-Medium` and `IBMPlexSans-SemiBold` as their own named families and use them where a style says 500 or 600 (card titles, column labels, the project name).
