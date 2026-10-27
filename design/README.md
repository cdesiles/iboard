iboard is a high-performance desktop tool for demanding code builders.

Key takeways:

- Fast: it stays dead simple
- Modern: you can now be a product developer.
- Opinionated: its target is small, high-impact teams and solopreneurs who build and test locally with tight feedback loops. There's no time to bounce between Jira, Notion, GitHub, GitLab and the rest. You only share on git what's already done and verified.
- Extendable: it's open source. Come with your ideas, contribute, copy it.

At its core, iboard is a kanban board built on markdown files. That's all. You can drive it entirely from the keyboard, and create and work through tasks with or without AI, and even with or without iboard itself, since it's all just files.

Issues live as markdown files next to the code. Four folders (`todo/`, `ready/`, `doing/`, `done/`) are four columns; one file is one issue. Issues flow left to right: Todo holds ideas that may still need refining, Ready holds refined issues queued for an agent (a `001-` prefix is the priority), Doing is what someone is working on, Done is finished. The interface should feel like an editor panel, not a project-management suite: dense, quiet, fast, and readable at a glance on a MacBook Pro.

## Principles

- **The file is the truth.** Every screen shows what is on disk. Name things the way the filesystem does (`todo/ai-assist.md`), never with an invented ID.
- **Quiet chrome, loud content.** Navigation and controls use `ink-muted` and `line-strong`; only issue text uses `ink`. Color is spent on state (column, selection, unsaved, review), never on decoration.
- **Small but visible.** Controls are 24pt tall, yet every control edge uses `line-strong` (at least 3:1 on every surface) and every text token clears 4.5:1. Never shrink a control below `control-h` or set text in `line` or a lighter grey.
- **Keyboard first, pointer complete.** Every action has a shortcut shown next to it, and every shortcut has a visible control.

## Content

- Sentence case everywhere except column titles (`TODO`, `DOING`, `DONE` in `ui-label`). Labels are always lowercase (`bug`, `discovery`), whatever the case in the title.
- Buttons are verbs: New issue, Refresh, Save, Review, Discard. No "OK", no ellipses unless a dialog follows.
- Counts are bare numbers in `meta`. "4 issues", never "4 issue(s)".
- Empty states say what goes there: "No issues in doing/" rather than "Empty".
- Errors name the file and the fix: "Couldn't save todo/ai-assist.md: the file changed on disk. Reload or keep your version."
- No emoji anywhere in the UI.

## Color

- `bg` is the window ground; `surface` is anything you can click into (card, reader page, editor, input). Do not stack `surface` on `surface` without a `line` border.
- Text: `ink` for content, `ink-muted` for secondary text (summaries, path, counts), `ink-faint` only for placeholders and separators.
- Control edges: `line-strong`. Card edges and dividers: `line`. A card is an object, not a control, so its rest border can be soft; on hover it takes `line-strong`, when selected `accent`.
- Selection and focus: `accent`. Buttons, chips, cards and other controls show focus as a solid 2px `focus` ring outside the element. Text entry (fields, textareas, the composer, the editor) keeps a single border: it turns `focus` and gains a soft 3px `focus-halo` glow, so the field never looks double-bordered. Focus is never a color change alone.
- Columns carry a status color as a 2px rule under the column title plus an 8px dot: `status-todo`, `status-ready`, `status-doing`, `status-done`. The column word is always shown, so color is never the only signal.
- `review` (violet) is reserved for the AI review feature. Nothing else in the app may use it, so the user learns that this hue means "an agent is talking". It is inviting on purpose: distinct from the blue `accent` and never close to `danger`.
- `danger` only for write failures, file conflicts and the Block verdict.
- Delete is the one control drawn in `danger` at rest: an outlined, labelled button in the Reader and Editor header (`Delete` `Del`; in the Editor it drops the key hint, because there `Del` edits text). The filled `danger` button only appears inside the ConfirmDialog, which is on by default and can be turned off in Settings › General.

## Type

- Interface in IBM Plex Sans, code and file names in IBM Plex Mono. Bundle both TTFs in the binary; fall back to the macOS system faces (`-apple-system`, `SF Mono`).
- Interface sizes: `ui-title` 15, `ui-body` and `ui-strong` 13, `ui-small` 12, `ui-label` and `meta` 11. Nothing below 11pt.
- Reading sizes are one step larger than the UI: `read-h1` 20, `read-body` 14/22, at most 68 characters per line (`reader-max`).
- The editor uses `edit-body` (mono 13/20) so markdown syntax lines up and is obviously "raw".

## Space and layout

- 4pt base: `space-1` 2 … `space-7` 32. Window gutter and column gap `space-5` (16). Gap between cards `space-3` (8). Card padding `space-4` × 10.
- Columns are equal width between `column-min` 280 and `column-max` 420. On a 14" MacBook Pro (1512pt wide) four columns sit near 360; with fewer folders they cap at 420 and the spare room stays empty on the right instead of stretching cards.
- Window skeleton, top to bottom: header bar (`bar-h` 40) · board or page · status bar (`statusbar-h` 24).
- Radii: `radius-sm` 4 for controls, `radius-md` 6 for cards and pages, `radius-pill` for 18pt pills. Cards are flat; `shadow-popover` is only for the review panel, menus, dialogs and a card being dragged.
- Scrollbars are overlays that never take layout width: a 4pt thumb (`scroll-thumb`) in `line-strong` at rest, `ink-muted` while scrolling, 8pt (`scroll-thumb-hover`) under the pointer, `accent` while dragging, and `line` in an unfocused pane or window. No track, no arrows. See Scrollbar.
- Two panes side by side are separated by the 16pt gutter, and that gutter is the resize handle: a 2×3 dot grip in `ink-faint` at rest, a full-height 2pt `line-strong` rule under the pointer, `accent` while dragging. It never overlaps a pane's scroll strip. See PaneDivider.
- Anything slow (stats from git history, charts, an agent's answer) shows Skeleton blocks at the content's final size: real labels and axes, data shapes in `surface-sunken`, all pulsing together on a 1.6s beat. Nothing before 300ms; the Thinking i and an elapsed clock from 2s; each block fills in on its own. See Skeleton.
- The Agenda shows commits per day across supervised projects: 18pt day squares in one `accent` ramp whose scale is rebuilt from what is shown, today ringed in `ink`, empty stretches of 4+ weeks folded, a per-project list with weekly bars, and the selected day's commits beside it. See Agenda.
- The Stats screen tracks maintenance cost: cycle time, complexity per kLOC, test coverage and lines of code as small multiples on one time axis with a shared crosshair (never a dual axis), a Refactor watch that turns 2+ signals into a "Draft a refactor issue" action, the biggest files with hotspots (complex and often changed), and the Agenda. See StatsScreen.

## Navigation

Navigation is three discreet pieces:

1. **Board button** at the far left of the Reader and Editor header: a bordered secondary button, chevron, "Board" and `esc`. Discreet means small, not invisible: it keeps the `line-strong` edge every control has.
2. **Breadcrumb in the header**, in `ink-muted`: `iboard / Todo / AI assist to refine an issue`. Every segment but the last is a link back, with a hover fill and underline. On the board it shows only the project name and path.
3. **Status bar at the bottom**: the current file path on the left, save state and the two or three shortcuts that apply to the current screen on the right.

There is no sidebar and no tab bar. Three screens are enough: Board, Reader, Editor. The Board has one extra mode, **ColumnFocus**: clicking a column title (or `⌘1` to `⌘4`) gives that column the whole window, with wide cards and its own sort, and `esc` goes back.

## Adding an issue

Capturing an idea takes one shortcut and a sentence. iboard names the issue from what you wrote; writing the rest can wait for the Editor or the AI review.

- **Two entry points, one composer.** The "New issue" button in the board header (primary, `⌘N`) and the `+` icon button in the Todo column header both open the same inline composer at the top of the Todo column. `⌘N` works from every screen: from the Reader or Editor it returns to the board first.
- **Description first.** The composer is a card with a reserved title row on top, a description that grows from 3 to 8 lines, and the meta line naming the file. See QuickAdd for its six states (idle, typing, naming, inferred, manual, failed).
- **`↩` names, then adds.** With an empty title, `↩` infers one from the description and stops on it, selected and tagged Inferred, so you can rename it by typing. `↩` again writes `todo/<slug>.md` (`# <title>`, then the description) and clears the composer. A title you typed yourself skips the stop.
- **`⌘↩` names, adds and opens the Editor** in one go. `⇧↩` is a new line in the description.
- **Never blocked.** With the engine Off, unavailable, failing or too slow, the first line of the description becomes the title.
- **Long notes** stop growing at 8 lines and scroll inside the composer, which stays sticky at the top of Todo. The board stays usable; `⌘↩` moves to the Editor.
- **`esc` closes** the composer; an empty composer never creates a file.
- **Labels** come from `[Brackets]` at the start of the title: `[Discovery]` gives the label discovery, and `[BUG]` gives bug (see Label). The title is shown without them everywhere except the Editor.
- **File name** is the title without its labels, slugified (lowercase ASCII, hyphens, at most 60 characters). If the file exists, append `-2`, `-3`; the meta line shows the final name before the file is written.
- **The engine** (On-device Apple by default, Review agent, or Off) is chosen in Settings › Titles (`⌘,`).
- New issues always land in Todo. Creating directly in Doing or Done is not offered; move the card afterwards.
- **A new Todo issue lands at position 1**, right under the composer: the column is file-name ordered, so the files are renumbered (`001-…`) once the click that created it has settled.
- **Editing is open in Todo and Ready only.** Doing and Done tickets open read-only: no Edit button, no `E`, no click-to-edit. A Done ticket's Reader header instead offers **Commits**, with the newest hash shown beside it.

| Key | Board | Reader | Editor |
| --- | --- | --- | --- |
| `↑ ↓ ← →` | Move selection between cards and columns | Scroll | Cursor |
| `↩` | Open selected issue | Edit | New line |
| `E` | Edit selected issue (Todo, Ready) | Edit (Todo, Ready) | |
| `⌘↩` | Move selected issue to the next column (no composer open) | | |
| `esc` | Clear filter | Back to board | Leave editor (asks if unsaved) |
| `⌘S` | | | Save to file |
| `⌘N` | New issue in Todo | New issue (returns to board) | New issue (asks if unsaved) |
| `⌘F` | Focus and widen the filter; `↓` reaches the suggestions | | |
| `⌘R` | Refresh from disk | Refine this issue (Todo; Refresh elsewhere) | Refine draft (Todo) |
| `⌘[` | | Back | Back (asks if unsaved) |
| `⌘⇧R` | | Review this issue | Review draft |
| `⌘Z` | Undo the last delete, when asking is off | | Undo typing |
| `⌘1`–`⌘4` | Show Todo, Ready, Doing or Done full screen (again: back to the board) | | |
| `⌘,` | Open Settings (last tab used) | Open Settings | Open Settings |
| `⇧↩` | New line in the composer's description | | |
| `A` | Archive selected (Done only) | | |
| `Del` | Delete selected (asks first unless turned off in Settings › General) | Delete this issue (same) | Delete key deletes text; use the header button |

## Interaction model

Spec-first, as commands and events (event-modeling style). A screen only changes in response to an event.

| Screen | Command (user) | Event | Next screen |
| --- | --- | --- | --- |
| Board | Select card | IssueSelected | Board, card selected |
| Board | Open issue (click, `↩`) | IssueOpened | Reader |
| Any | New issue (`⌘N`, header button, Todo `+`) | ComposerOpened | Board, composer focused in Todo |
| Board | Describe, `↩` (no title yet) | TitleInferenceRequested → TitleInferred | Composer shows the selected title, tagged Inferred |
| Board | `↩` while naming fails or times out | TitleInferenceFailed | Composer: description kept, cursor in the title |
| Board | Type in the title row | TitleEdited | Inferred tag drops; meta shows the new file name |
| Board | `↩` with a title (or the first line as fallback) | IssueCreated | Board, new card under the composer, composer cleared |
| Board | `⌘↩` | (TitleInferred →) IssueCreated → EditStarted | Editor on the new file |
| Board | `esc` in composer | ComposerClosed | Board |
| Reader | Click description, `E`, `↩` | EditStarted | Editor, cursor at click point |
| Reader | Click a ghost section or an "Add a section" chip | SectionAddRequested → EditStarted | Editor with `## <Section>` appended, cursor under it |
| Reader | Click the empty sheet below the text | EditStarted | Editor, cursor at the end of the file |
| Reader | Draft them with Review | ReviewRequested → ReviewReceived | Review panel; proposed sections apply to the draft |
| Reader (Done) | Click Commits | CommitsOpened | Popup lists the commits whose message names the file (`closes <file>.md`) |
| Commits popup | Click a commit, or All | DiffOpened | Diff: the changed files on the left, the unified diff on the right; `⌘F` finds, `n p` and `] [` move by file and hunk, `esc` goes back to the issue |
| Editor | Type | DraftChanged | Editor, Unsaved pill |
| Editor | Save (`⌘S`, or auto on leave when Auto is on in Settings › General) | IssueSaved / SaveFailed | Reader / Editor with error |
| Any | Refresh, or file watcher fires | IssuesReloaded | Same screen; open draft kept, conflict flagged |
| Board | Focus filter (`⌘F`, click) | FilterFocused | Field widens to 480pt, the three most used labels shown |
| Board, Reader | Click a label | LabelClicked → FilterSuggestionPicked | Board filtered by that label |
| Board | Click a suggestion, or type | FilterSuggestionPicked / FilterChanged | Board shows matches only; counts read "2/6" |
| Board | `esc` in the filter | FilterCleared | Full board; field back to 220pt on blur |
| Board | Click a column title, `⌘1`–`⌘4` | ColumnFocused | ColumnFocus: that column full screen, wide cards, its default sort |
| ColumnFocus | Pick a sort, toggle Group by label | ColumnSortChanged / ColumnGroupingToggled | Same view, reordered; files untouched |
| ColumnFocus | Click another column tab | ColumnFocused | That column full screen |
| ColumnFocus | Click the active tab, Board button, `esc` | ColumnUnfocused | Board, same card selected |
| Reader, Editor, ColumnFocus | Two-finger swipe right on the trackpad, or `⌘[` | NavigatedBack | One level up the breadcrumb: Editor to Reader, Reader to the board (or its focused column), ColumnFocus to the board |
| Any | Move to column (drag, or menu) | IssueMoved | Same screen; file moved between folders |
| Board | Start dragging a card | IssueDragStarted → DropZonesShown | Board, Archive and Delete zones slide up from the bottom |
| Board | Drop on Archive (Done only), or card Archive button, `A` | IssueArchived | Board; file moved to `.archive/` |
| Board | Drop on Delete, card trash, `Del` | DeleteRequested → DeleteConfirmRequested (asking on) | ConfirmDialog centred over the board |
| Reader, Editor | Delete header button, `Del` (Reader only) | DeleteRequested → DeleteConfirmRequested (asking on) | ConfirmDialog centred over the page |
| Board, Reader, Editor | Any delete, with asking off | DeleteRequested → IssueDeleted | Board; status bar "Moved … to the Trash", `⌘Z` undo |
| Board | `⌘Z` after a delete | IssueRestored | Board; file and card back in place |
| ConfirmDialog | Delete (`↩`) / Cancel (`esc`) | IssueDeleted / DeleteCancelled | Board |
| Reader, Editor | Review (`⌘⇧R`) | ReviewRequested → ReviewReceived | Review panel opens beside the page |
| Any | Settings (`⌘,`, Review menu, composer engine tag) | SettingsOpened | Settings dialog on the matching tab |
| Settings › AI review | Pick tool, pick model, Save (`⌘↩`) | ReviewToolChosen, ReviewModelChosen → SettingsSaved | Back to the screen; Review menu shows the new tool and model |
| Settings › General | Pick a theme, Save (`⌘↩`) | ThemeChosen → SettingsSaved | Back to the screen, drawn in the new theme |
| Settings › General | Toggle "Ask before deleting an issue", Save (`⌘↩`) | DeleteConfirmToggled → SettingsSaved | Back to the screen; deletes ask or not |
| Settings › Titles | Pick engine, Save (`⌘↩`) | TitleEngineChosen → SettingsSaved | Back to the screen; composer tag shows the engine |
| RefinePanel | Click a choice, `1`–`9` (same key again clears) | RefineChoicePicked / RefineChoiceCleared | Choice selected; note and Send labels follow it |
| RefinePanel | Type in the note | RefineNoteChanged | Send enabled |
| RefinePanel | Send (`⌘↩`) | RefineAnswerSent → RefineRoundReceived | Next round, or the refined draft in the Editor, unsaved |
| RefinePanel | Hide (`esc`) / End refining | RefinePanelHidden / RefineSessionEnded | Reader; nothing written |
| RefinePanel, waiting | `esc`, Cancel | RefineRequestCancelled | Previous round back, choice and note kept |
| Two panes | Drag the gutter, `← →` on the focused divider | PaneSplitChanged | Panes resized; split remembered for that pair |
| Two panes | Double-click the gutter, `↩` on the focused divider | PaneSplitReset | Default split |

## Motion

Motion is short and only ever says where something went: the filter field widening (120ms), the drop zones sliding up (140ms), the Design panel's rail sliding to its components (220ms), the naming dot's pulse, the Thinking mark's rocking card. When macOS asks for reduced motion (Accessibility › Display › Reduce motion), every one of them is skipped: states change in one frame, the dot stays still and the Thinking card rests at −8°. The app reads the setting once at launch (`theme::reduced_motion`); `bundle.css` gates the same transitions behind `prefers-reduced-motion`, which reads the same setting in WebKit.

## Iconography

Line icons at 14pt, 1.6pt stroke, `currentColor`, drawn on the 24pt control grid: refresh, back chevron, edit pencil, plus, review, trash, archive, tag-plus, git node, play and square (stop). The repo has no icon set yet; use Lucide (MIT) glyphs of the same names, and keep any icon paired with a word (the back chevron comes with "Board") except the per-card actions (trash, archive), which carry a tooltip and only appear when the card is hovered, selected or focused.

## AI review

The Review button asks an agent (OpenCode, Claude, or any command the user
configures in `~/.config/iboard/iboard.toml`, from `⌘,`) to challenge the open
issue. The tool and model are chosen in the ReviewSettings dialog: a Tool select (OpenCode, Claude Code, Custom command) and a Model select whose first option, Default, uses the tool's first favorite model:

- **Placement**: the rightmost control of the header on Reader and Editor screens, after Refresh; always labelled "Review", with `review` text and border on `surface`. The Board's selected-card hover row is still to come.
- **What it returns**: three findings, always in this order, each with a verdict pill and one or two sentences.
  - *Relevance*: is this still worth doing, does it duplicate another issue?
  - *Size*: can one person or one agent finish it in one sitting, or should it split? It proposes the split.
  - *Clarity*: could someone implement it without asking a question? It lists missing acceptance criteria, files, or decisions.
- **Verdicts**: Pass (`status-done`), Challenge (`review`), Block (`danger`), always with the word.
- **From an idea to an issue**: when the issue is only a title, the review drafts the missing sections (Context, Acceptance criteria, Out of scope, Open questions) as a proposal. *Apply to draft* opens it in the Editor, unsaved; the agent never writes the file itself.
- **States**: idle, running (button shows "Reviewing…", `esc` cancels), result (panel open), failed (inline `danger` text with retry). Only the issue text is sent; interactive agents are run in one-shot mode, so no terminal is attached yet.
- **Refining (RefinePanel)**: an agent in a role (the Challenger) pushes back on the issue and offers numbered resolutions. The panel has two panes: the TL;DR (`ui-title` with a 3px `review` rule) and the reasoning (`read-body`, 68ch) on the left, the choices on the right so they never scroll away. Each choice is a bordered card with a short title, a muted detail, its number key and a radio; the selected one takes the `accent` edge and `accent-soft` fill. The note under the choices follows the selection ("Add a note to choice 1", or "Or answer in your own words"). Send is the one filled `review` button and names what goes out ("Send choice 1", "Send answer"). Under 820pt it stacks and folds the reasoning under the TL;DR. See `components/RefinePanel`.
- **Waiting (Thinking)**: while the agent works (often about 10s), the iboard "i" stands in for a spinner: its stem holds still and its card-dot rocks gently from −8° to +5° every 1.6s. The panel opens at its final size with placeholder bars and cards, the status line says what it waits for with an elapsed clock ("Challenger is reading the issue · 4s"), and after 20s it says it is slow and that `esc` cancels. No fake progress. See `components/Thinking`.

## Using the tokens from Rust

Tokens are points, which map 1:1 to egui points and to CSS px on a Retina display. The header font (Ubuntu) and widget shapes in the screenshot look like egui's defaults, so the companion `theme.rs` in the project files maps these tokens onto `egui::Visuals` and `egui::Style`. If the app uses iced, Slint or Tauri instead, the same `tokens.json` drives it; only the mapping file changes.

egui has no font weights: register `IBMPlexSans-Medium` and `IBMPlexSans-SemiBold` as their own named families and use them where a style says 500 or 600 (card titles, column labels, the project name).
