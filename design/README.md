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
- **Keyboard first, pointer complete.** Every action has a shortcut, and every shortcut has a visible control. The keys stay out of sight until you ask: hold ⌘ alone for half a second and each key slides out beside its control (buttons, the filter, footers, popups) while the status bar lists the screen's keys. Letting go, or pressing anything with ⌘, puts them away, so a shortcut never flashes them. Tooltips and menus always show their keys.
- **Type to filter.** Any list that opens (the ProjectMenu, the GroupsDialog and LabelPicker menus, select menus, filter suggestions) filters the moment you type: the first printable key goes into its filter field, already holding that character, with no click and no shortcut first. A list short enough to hide its field shows it on that first key. `↑ ↓` still move, `↩` opens the first match, `esc` clears the filter before it closes anything.

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

- Interface in Inter, code, file names and key hints in JetBrains Mono. Both are under the SIL Open Font License; bundle the TTFs in the binary with their license text.
- Both faces carry the macOS key symbols themselves: ⌘ ⌥ ⇧ ⌃ ↩ ⌫ ⌦ ⇥ ⇪ and the arrows. That is why they replaced IBM Plex, which has none of them. egui never falls back to system fonts, so a symbol the bundled face lacks shows as an empty box: check a new glyph against the face before using it (JetBrains Mono has no ⏏).
- Interface sizes: `ui-title` 15, `ui-body` and `ui-strong` 13, `ui-small` 12, `ui-label` and `meta` 11. Nothing below 11pt.
- Reading sizes are one step larger than the UI: `read-h1` 20, `read-body` 14/22, at most 68 characters per line (`reader-max`).
- The editor uses `edit-body` (mono 13/20) so markdown syntax lines up and is obviously "raw".

## Space and layout

- 4pt base: `space-1` 2 … `space-7` 32. Window gutter and column gap `space-5` (16). Gap between cards `space-3` (8). Card padding `space-4` × 10.
- Columns are equal width between `column-min` 280 and `column-max` 420. On a 14" MacBook Pro (1512pt wide) four columns sit near 360; with fewer folders they cap at 420 and the spare room stays empty on the right instead of stretching cards.
- Window skeleton, top to bottom: header bar (`bar-h` 40) · board or page · status bar (`statusbar-h` 24).
- Radii: `radius-sm` 4 for controls, `radius-md` 6 for cards and pages, `radius-pill` for 18pt pills. Cards are flat; `shadow-popover` is only for the review panel, menus and a card being dragged; modals use the stronger `shadow-modal`.
- Modals come in four sizes and one anatomy (head 40 · scrolling body · foot 40 with one primary, rightmost): **S** 360 asks a question (ConfirmDialog), **M** 480 holds a form (Settings, GroupsDialog), **L** 760 holds one document, **XL** fills the window minus a 32pt inset, at most 1200 × 900, for work surfaces where you write at length or an agent's answer streams in (NewIssueModal, RefinePanel). Only S closes on a scrim click; XL never asks on `esc`, it parks your input. See Modal.
- Scrollbars are overlays that never take layout width: a 4pt thumb (`scroll-thumb`) in `line-strong` at rest, `ink-muted` while scrolling, 8pt (`scroll-thumb-hover`) under the pointer, `accent` while dragging, and `line` in an unfocused pane or window. No track, no arrows. See Scrollbar.
- Two panes side by side are separated by the 16pt gutter, and that gutter is the resize handle: a 2×3 dot grip in `ink-faint` at rest, a full-height 2pt `line-strong` rule under the pointer, `accent` while dragging. It never overlaps a pane's scroll strip. See PaneDivider.
- Anything slow (stats from git history, charts, an agent's answer) shows Skeleton blocks at the content's final size: real labels and axes, data shapes in `surface-sunken`, all pulsing together on a 1.6s beat. Nothing before 300ms; the Thinking i and an elapsed clock from 2s; each block fills in on its own. See Skeleton.
- The Agenda shows commits per day across supervised projects: 18pt day squares in one `accent` ramp whose scale is rebuilt from what is shown, today ringed in `ink`, empty stretches of 4+ weeks folded, a per-project list with weekly bars, and the selected day's commits beside it. See Agenda.
- The Stats screen tracks maintenance cost: cycle time, complexity per kLOC, test coverage and lines of code as small multiples on one time axis with a shared crosshair (never a dual axis), a Refactor watch that turns 2+ signals into a "Draft a refactor issue" action, the biggest files with hotspots (complex and often changed), and the Agenda. See StatsScreen.

## Navigation

Navigation is three discreet pieces:

1. **Board button** at the far left of the Reader and Editor header: a bordered secondary button, chevron, "Board" and `esc`. Discreet means small, not invisible: it keeps the `line-strong` edge every control has.
2. **Breadcrumb in the header**, in `ink-muted`: `iboard / Todo / AI assist to refine an issue`. Every segment but the last is a link back, with a hover fill and underline. On the board it shows only the project name and path. There the project name opens the ProjectMenu: the groups (each its own board), then your projects in your own order, scrolling inside the menu and filtered by typing, then a footer strip on `bg` with three actions: **Add project…**, **Edit groups…** (GroupsDialog, written back to `[[workspace.group]]` in `~/.config/iboard/iboard.toml`; the board reloads in place) and **Reorder** (drag or `⌥↑ ⌥↓`, saved as a user preference).
3. **Status bar at the bottom**: the current file path and save state on the left; while ⌘ is held, the two or three shortcuts that apply to the current screen on the right.

There is no sidebar and no tab bar. Three screens are enough: Board, Reader, Editor.

## Adding an issue

Capturing an idea takes one shortcut and a sentence. iboard names the issue from what you wrote; writing the rest can wait for the Editor, or Review can draft it while you watch.

- **Two entry points, one composer.** The "New issue" button in the board header (primary, `⌘N`) and the `+` icon button in the Todo column header both open the same **NewIssueModal**, an XL modal over the board. `⌘N` works from every screen.
- **Notes left, the file right.** You write (or dictate) in the notes pane; the right pane is a Reader sheet showing exactly what `↩` will write, at reading size and never cut short. The title states (naming, inferred, manual, failed) are QuickAdd's, shown on the sheet's title row.
- **Draft with Review (`⌘⇧R`) fills the sheet in place.** The known section headings appear at once with Skeleton bars, the section being written carries a `review` rule and caret, and the sheet follows the writing line. `esc` stops and keeps what arrived. The agent never writes the file. See NewIssueModal.
- **`↩` adds, and never waits.** It writes `todo/<slug>.md` (`# <title>`, then the notes, or the draft when there is one) and closes the modal. With an empty title the file is `unnamed-ticket-xxxx` until the engine's title lands in the background, within 15s, renaming the file and setting its heading; the card reads "Naming…" meanwhile.
- **`⌘↩` names, adds and opens the Editor** in one go. `⇧↩` is a new line in the description.
- **Never blocked.** With the engine Off, unavailable, failing or too slow, the first line of the description becomes the title.
- **Long notes** have the whole notes pane; nothing is capped or truncated.
- **`esc` closes** the modal and parks the notes (`.iboard/new-issue.md`); the next `⌘N` brings them back. Discard drops them. An empty note never creates a file.
- **Labels** come from `[Brackets]` at the start of the title: `[Discovery]` gives the label discovery, and `[BUG]` gives bug (see Label). The title is shown without them everywhere except the Editor.
- **File name** is the title without its labels, slugified (lowercase ASCII, hyphens, at most 60 characters). If the file exists, append `-2`, `-3`; the meta line shows the final name before the file is written.
- **The engine** (On-device Apple by default, Review agent, or Off) is chosen in Settings › Titles (`⌘,`).
- New issues always land in Todo. Creating directly in Doing or Done is not offered; move the card afterwards.
- **A new Todo issue lands at position 1**, at the top of Todo: the column is file-name ordered, so the files are renumbered (`001-…`) once the click that created it has settled.
- **Editing is open in Todo and Ready only.** Doing and Done tickets open read-only: no Edit button, no `E`, no click-to-edit. A Done ticket's Reader header instead offers **Commits**, with the newest hash shown beside it.

| Key | Board | Reader | Editor |
| --- | --- | --- | --- |
| `↑ ↓ ← →` | Move selection between cards and columns | Scroll | Cursor |
| `↩` | Open selected issue | Edit | New line |
| `E` | Edit selected issue (Todo, Ready) | Edit (Todo, Ready) | |
| `⌘↩` | Move selected issue to the next column (no composer open) | | |
| `esc` | Clear filter | Back to board | Leave editor (asks if unsaved) |
| `⌘S` | | | Save to file |
| `⌘N` | New issue (NewIssueModal) | New issue, over the page | New issue, over the page (the unsaved edit stays open behind it) |
| `⌘F` | Focus and widen the filter; `↓` reaches the suggestions | | |
| `⌘R` | Refresh from disk | Refine this issue (Todo; Refresh elsewhere) | Refine draft (Todo) |
| `⌘[` | | Back | Back (asks if unsaved) |
| `⌘⇧R` | | Review this issue | Review draft |
| `⌘Z` | Undo the last delete, when asking is off | | Undo typing |
| `⌘,` | Open Settings (last tab used) | Open Settings | Open Settings |
| `⇧↩` | New line in the new issue's notes | | |
| `A` | Archive selected (Done only) | | |
| `L` | Add a label to the selected issue (Todo, Ready) | Add a label to this issue (Todo, Ready) | |
| `Del` | Delete selected (asks first unless turned off in Settings › General) | Delete this issue (same) | Delete key deletes text; use the header button |

## Interaction model

Spec-first, as commands and events (event-modeling style). A screen only changes in response to an event.

| Screen | Command (user) | Event | Next screen |
| --- | --- | --- | --- |
| Board | Select card | IssueSelected | Board, card selected |
| Board | Open issue (click, `↩`) | IssueOpened | Reader |
| Any | New issue (`⌘N`, header button, Todo `+`) | NewIssueOpened | NewIssueModal (XL), focus in the notes; a parked note is restored |
| NewIssueModal | Draft with Review (`⌘⇧R`) | DraftRequested → DraftSectionStarted / DraftTextAppended … → DraftCompleted / DraftFailed | The sheet fills in place, section by section |
| NewIssueModal | `esc` while drafting | DraftStopped | Partial draft kept; unwritten sections become ghost tiles |
| NewIssueModal | Close, `esc` | NewIssueParked / NewIssueClosed | Board; Todo `+` shows a parked dot |
| NewIssueModal | Describe, `↩` (no title yet) | IssueCreated, TitleInferenceRequested | Board; the card, `unnamed-ticket-xxxx`, flashes at the top of Todo and reads "Naming…" |
| Board | The engine answers | TitleInferred | The file is renamed and its heading set, unless renamed or retitled meanwhile |
| Board | The engine fails or times out | TitleInferenceFailed | The issue stays `unnamed-ticket-xxxx` |
| NewIssueModal | `↩` with a title (or the first line, with no engine) | IssueCreated | Board, new card flashes at the top of Todo |
| NewIssueModal | `⌘↩` | IssueCreated → EditStarted (title as for `↩`, landing once the Editor closes) | Editor on the new file |
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
| Reader, Editor | Two-finger swipe right on the trackpad, or `⌘[` | NavigatedBack | One level up the breadcrumb: Editor to Reader, Reader to the board |
| Any | Move to column (drag, or menu) | IssueMoved | Same screen; file moved between folders |
| Board, Reader | Add label (card action, Reader header button, `L`) | LabelPickerOpened | LabelPicker under the trigger, field focused |
| LabelPicker | Type, `↩` (or click a board label, or `↓` then `↩`) | LabelAdded / LabelAddFailed | Same popup, field cleared; the label shows on the card and the Reader, and the file is rewritten |
| LabelPicker | Click a label the issue has | LabelRemoved | Same popup; the label turns off and the file is rewritten |
| LabelPicker | `esc`, click outside, trigger again | LabelPickerClosed | Board or Reader; text left in the field is dropped |
| Board | Start dragging a card | IssueDragStarted → DropZonesShown | Board, Archive and Delete zones slide up from the bottom |
| Board | Drop on Archive (Done only), or card Archive button, `A` | IssueArchived | Board; file moved to `.archive/` |
| Board | Drop on Delete, card trash, `Del` | DeleteRequested → DeleteConfirmRequested (asking on) | ConfirmDialog centred over the board |
| Reader, Editor | Delete header button, `Del` (Reader only) | DeleteRequested → DeleteConfirmRequested (asking on) | ConfirmDialog centred over the page |
| Board, Reader, Editor | Any delete, with asking off | DeleteRequested → IssueDeleted | Board; status bar "Moved … to the Trash", `⌘Z` undo |
| Board | `⌘Z` after a delete | IssueRestored | Board; file and card back in place |
| ConfirmDialog | Delete (`↩`) / Cancel (`esc`) | IssueDeleted / DeleteCancelled | Board |
| Reader, Editor | Review (`⌘⇧R`) | ReviewRequested → ReviewReceived | Review panel opens beside the page |
| Any | Settings (`⌘,`, Review menu, new issue engine tag) | SettingsOpened | Settings dialog on the matching tab |
| Settings › AI review | Pick tool, pick model, Save (`⌘↩`) | ReviewToolChosen, ReviewModelChosen → SettingsSaved | Back to the screen; Review menu shows the new tool and model |
| Settings › General | Pick a theme, Save (`⌘↩`) | ThemeChosen → SettingsSaved | Back to the screen, drawn in the new theme |
| Settings › General | Toggle "Ask before deleting an issue", Save (`⌘↩`) | DeleteConfirmToggled → SettingsSaved | Back to the screen; deletes ask or not |
| Settings › Titles | Pick engine, Save (`⌘↩`) | TitleEngineChosen → SettingsSaved | Back to the screen; new issue tag shows the engine |
| Board | Edit groups… in the ProjectMenu footer | GroupsEditorOpened | GroupsDialog over the board, draft read from the config |
| ProjectMenu | Type / Reorder, drag or `⌥↑ ⌥↓`, Done | ProjectFilterChanged / ProjectReorderStarted → ProjectMoved → ProjectOrderSaved | Matches only / projects in the new order, saved to the user config |
| ProjectMenu | Add project… | FolderPickerOpened → ProjectAdded → WorkspaceSwitched | The project at the top of the list, its board open |
| GroupsDialog | New group, rename, Add project, remove a project | GroupDrafted / GroupRenamed / ProjectAddedToGroup / ProjectRemovedFromGroup | Same dialog, Unsaved; nothing on disk changes |
| GroupsDialog | Delete group (trash, `⌘⌫`) | GroupDeleteRequested → GroupDeleteConfirmRequested → GroupDeleted | ConfirmDialog, then the group leaves the draft; projects untouched |
| GroupsDialog | Save (`⌘↩`) | GroupsSaved → UserConfigWritten → WorkspaceReloaded / GroupsSaveFailed | Board reloaded in place / dialog with the error |
| RefinePanel | Click a choice, `1`–`9` (same key again clears) | RefineChoicePicked / RefineChoiceCleared | Choice selected; note and Send labels follow it |
| RefinePanel | Type in the note | RefineNoteChanged | Send enabled |
| RefinePanel | Send (`⌘↩`) | RefineAnswerSent → RefineRoundReceived | Next round, or the refined draft in the Editor, unsaved |
| RefinePanel | Hide (`esc`) / End refining | RefinePanelHidden / RefineSessionEnded | Reader; nothing written |
| RefinePanel, waiting | `esc`, Cancel | RefineRequestCancelled | Previous round back, choice and note kept |
| Two panes | Drag the gutter, `← →` on the focused divider | PaneSplitChanged | Panes resized; split remembered for that pair |
| Two panes | Double-click the gutter, `↩` on the focused divider | PaneSplitReset | Default split |
| Agenda | Click a day | AgendaDaySelected | Day panel lists that day's commits by project |
| Agenda | Pick a range, click a project row | AgendaRangeChanged / AgendaProjectFiltered | Grid, scale and legend rebuilt for what is shown |
| Stats | Refresh, or the repo changes | MetricsScanned | Tiles, trends, watch and files update in place |
| Stats | Draft a refactor issue | RefactorIssueDrafted → NewIssueOpened | NewIssueModal, notes prefilled with the signals and hotspot files |

## Motion

Motion is short and only ever says where something went: the filter field widening (120ms), the key hints sliding out of their buttons while ⌘ is held (120ms), the drop zones sliding up (140ms), the Design panel's rail sliding to its components (220ms), the naming dot's pulse, the Thinking mark's rocking card. When macOS asks for reduced motion (Accessibility › Display › Reduce motion), every one of them is skipped: states change in one frame, the dot stays still and the Thinking card rests at −8°. The app reads the setting once at launch (`theme::reduced_motion`); `bundle.css` gates the same transitions behind `prefers-reduced-motion`, which reads the same setting in WebKit.

## Iconography

Line icons at 14pt, 1.6pt stroke, `currentColor`, drawn on the 24pt control grid: refresh, back chevron, edit pencil, plus, review, trash, archive, tag-plus, git node, play and square (stop). The repo has no icon set yet; use Lucide (MIT) glyphs of the same names, except trash, which is iboard's own soft bin (see the Icons card): a 15-wide lid, a round 2.5 knob instead of a square tab, a body that tapers 1pt each side into 3pt rounded bottom corners, and two short ribs. Use that one path everywhere a delete appears (card action, header Delete button, ConfirmDialog, Delete drop zone); never Lucide's straight-sided `trash-2`. Keep any icon paired with a word (the back chevron comes with "Board") except the per-card actions (trash, archive), which carry a tooltip and only appear when the card is hovered, selected or focused.

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
- **Waiting (Thinking)**: while the agent works (often about 10s), the iboard "i" stands in for a spinner: its blue dot stretches across the stem, the stem tucks into the freed corner, and the two hand over until the i stands again a quarter turn on (1.6s per quarter, a full square in 6.4s). The panel opens at its final size with placeholder bars and cards, the status line says what it waits for with an elapsed clock ("Challenger is reading the issue · 4s"), and after 20s it says it is slow and that `esc` cancels. No fake progress. See `components/Thinking`.

## Using the tokens from Rust

Tokens are points, which map 1:1 to egui points and to CSS px on a Retina display. The header font (Ubuntu) and widget shapes in the screenshot look like egui's defaults, so the companion `theme.rs` in the project files maps these tokens onto `egui::Visuals` and `egui::Style`. If the app uses iced, Slint or Tauri instead, the same `tokens.json` drives it; only the mapping file changes.

egui has no font weights: register `Inter-Medium`, `Inter-SemiBold` and `JetBrainsMono-Medium` as their own named families and use them where a style says 500 or 600 (card titles, column labels, the project name, `meta`). `theme.rs` does this in `install_fonts`.
