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
- **Calm by default.** A screen at rest shows content and the few controls you need, nothing else: no key hints, no scrollbars, no file names on cards, no edges that don't identify a control. Density stays (24pt controls, 13pt UI, four columns on a 14" screen); what goes is noise. Devin's restraint is the reference.
- **Each fact once.** For every screen, list what it shows and say each thing in one place (see Each fact once below). A path, a count or a date that appears twice is one too many.
- **Colour once per screen.** One filled button per screen, in `primary` (ink). On a control, blue means selected, focused or a link and nothing else; beyond controls it is the data colour, on charts and the build line. Status colour lives in the four small column icons; label colour in 6pt dots; violet only where an agent talks; red only for a failure or inside the Delete confirmation.
- **Edges only where they identify.** `line-strong` (3:1 or better, WCAG 1.4.11) only on text fields, Selects, checkboxes, radios and a ghost tile's dashed edge, and under the pointer on a card, an Agenda day, the Reader's edit outline and a pane divider. A text button is identified by its label, so it has no edge in the header and a `line-soft` hairline in modal feet and dialogs. Cards rest on `line-soft`.
- **Keyboard first, pointer complete.** Every action has a shortcut, and every shortcut has a visible control. The keys stay out of sight until you ask: hold ⌘ alone for half a second and each key slides out beside its control (buttons, the filter, footers, popups) while the status bar lists the screen's keys and a small card lists them all (see Shortcuts). Letting go, or pressing anything with ⌘, puts them away, so a shortcut never flashes them. Tooltips, menus and the RefinePanel's numbered choices always show their keys.
- **Type to filter.** Any list that opens (the ProjectMenu, the GroupsDialog and LabelPicker menus, select menus, filter suggestions) filters the moment you type: the first printable key goes into its filter field, already holding that character, with no click and no shortcut first. A list short enough to hide its field shows it on that first key. `↑ ↓` still move, `↩` opens the first match, `esc` clears the filter before it closes anything.

### Each fact once

| Screen | Shows, once each | Removed repeats |
| --- | --- | --- |
| Board | Project name (header, opens the ProjectMenu) · per-column icon, name and count (column head) · cards: labels, title, summary; Done adds its date · the issues folder (status bar) · file name and actions of the hovered or selected card (card strip) | The issues path left the header for the status bar. The total "9 issues" left the status bar: the column counts already say it. The file name left every card at rest. |
| Reader | Column and project (breadcrumb, with the column's status icon) · labels and title (sheet) · body · what is not written yet (dock) · path, modified time, progress or word count (status bar) | The sheet's head strip (column mark, path, "modified 15d ago", word count) is gone: the breadcrumb has the column, the status bar has the rest. The issue title joins the breadcrumb only once the `read-h1` has scrolled out of view. |
| Editor | Breadcrumb with the title · the raw file · path and save state (status bar) | Save state is said once, in the status bar pill (not also in the header). |
| Modals | Title and context in the head (`in todo/`, Inter `ink-muted`) · body · one primary in the foot | The file to be written is named once: in the foot when it is the commit point, otherwise in the head. The NewIssueModal shows the file on its sheet, so its head names only the folder (`in todo/`) and the sheet head alone names the file (`todo/<slug>.md`). |

## Content

- Sentence case everywhere, column titles included (`Todo`, `Doing`, `Done` in `ui-strong`); only `ui-label` section labels such as "Not written yet" are drawn uppercase. Labels are always lowercase (`bug`, `discovery`), whatever the case in the title.
- Buttons are verbs: New issue, Refresh, Save, Review, Discard. No "OK", no ellipses unless a dialog follows.
- Counts are bare numbers in `meta` (Inter). "4 issues", never "4 issue(s)".
- Empty states say what goes there: "No issues in doing/" rather than "Empty".
- Errors name the file and the fix: "Couldn't save todo/ai-assist.md: the file changed on disk. Reload or keep your version."
- No emoji anywhere in the UI.

## Color

- `bg` is the window ground; `surface` is anything you can click into (card, reader page, editor, input, menu, modal). Cards and pages on `bg` take a `line-soft` edge; do not stack `surface` on `surface` without one.
- Text: `ink` for content, `ink-muted` for secondary text (summaries, paths, counts, dates, the status bar, modal context), `ink-faint` only for placeholders, separators and menu key caps.
- **Edges.** `line-strong` identifies a control: text fields, Selects, checkboxes, radios and a ghost tile's dashed edge, plus, under the pointer, a card, an Agenda day, the Reader's edit outline and a pane divider. `line-soft` is a hairline for cards at rest and for secondary buttons in modal feet and dialogs. `line` is for dividers only: the header and status bar rules, modal head and foot rules, a modal's frame. Header buttons have no edge at all. See Button for which applies where.
- **One filled button per screen**, in `primary` (`ink`), label in `on-primary`: New issue on the Board, Save in the Editor, the commit button of a modal. The Reader has none. The violet filled button in RefinePanel (Send), the red one in ConfirmDialog (Delete) and Apply to draft in the Review panel are that surface's one fill. Switching `primary` back to `{accent}` restores today's blue; the Button card shows both.
- Selection and focus: `accent`. With an ink primary, blue on a control means only selected, focused or a link. It is also the data colour: the Stats charts and bars, and the Agenda's ramp. Buttons, chips, cards and other controls show focus as a solid 2px `focus` ring outside the element. Text entry (fields, textareas, the composer, the editor) keeps a single border: it turns `focus` and gains a soft 3px `focus-halo` glow, so the field never looks double-bordered. Focus is never a color change alone.
- **Columns** carry their status as a 14pt icon whose shape differs per column, in the status colour: Todo an empty circle (`status-todo`), Ready a dotted circle (`status-ready`), Doing a half-filled circle (`status-doing`), Done a filled circle with a check (`status-done`). Shape and word carry the meaning, so colour is never the only signal. There is no coloured rule.
- **Labels** are neutral pills (`surface-sunken`, `ink-muted`) with the label's colour as a 6pt dot. The presets use the `label-*` palette (`bug` red, `feature` green, `discovery` teal, `question` ochre, `chore` grey); Settings › Labels can give any label its own `#rrggbb`, and a label with no colour has no dot. A label marked Stand out (`[label_style] stand_out`, default `["bug"]`) is a tinted pill: its colour as text on itself at `label-tint` (`label-tint-dark` in dark) over `surface`, 4.5:1 or better, else `ink` text. The presets have no blue (selection) and no violet (review).
- `review` (violet) is reserved for the agent: the AI split button, the Challenge verdict, the RefinePanel and what an agent wrote. Nothing else may use it, so the user learns that this hue means "an agent is talking". It is inviting on purpose: distinct from the blue `accent` and never close to `danger`.
- `danger` only for write failures, file conflicts, the Block verdict, and Delete where it is chosen: the header's Delete icon under the pointer (its label pill in `danger` on `danger-soft`) and the filled Delete in the ConfirmDialog, which is on by default and can be turned off in Settings › General. Nothing is drawn in `danger` at rest.

### Contrast

| Pair | Light | Dark | Needs |
| --- | --- | --- | --- |
| `line-strong` on `bg` / `surface` / `surface-sunken` | 3.46 / 3.77 / 3.25 | 4.25 / 3.87 / 4.41 | 3:1 (identifies a control) |
| `line-soft` on `bg` / `surface` | 1.19 / 1.30 | 1.38 / 1.25 | decorative |
| `ink-muted` on every surface | 6.48 or more | 6.46 or more | 4.5:1 |
| `on-primary` on `primary` / `primary-hover` | 17.8 / 12.7 | 14.8 / 11.3 | 4.5:1 |
| Status icon colour on `bg` | 5.2 to 5.5 | 7.9 to 8.8 | 3:1 |
| Label dot on `surface-sunken` | 4.6 to 5.0 | 7.3 to 9.3 | 3:1 |
| Loud label text on its tint | 4.7 to 5.0 | 5.2 to 5.9 | 4.5:1 |


## Type

- **Inter for the interface, JetBrains Mono only for code**: the editor, code blocks and inline code, diffs, and key caps. Paths, file names, counts, dates, word counts, the status bar and modal context ("in todo/", "named in the background", "Saved for this session") are Inter in `meta`, `ink-muted`. Both faces are under the SIL Open Font License; bundle the TTFs in the binary with their license text.
- Weights are the static faces only: Inter Regular, Medium, SemiBold and JetBrains Mono Regular, Medium. egui has no weights, so each is a named family (`theme::install`).
- Both faces carry the macOS key symbols themselves: ⌘ ⌥ ⇧ ⌃ ↩ ⌫ ⌦ ⇥ ⇪ and the arrows. That is why they replaced IBM Plex, which has none of them. egui never falls back to system fonts, so a symbol the bundled face lacks shows as an empty box: check a new glyph against the face before using it (JetBrains Mono has no ⏏).
- Interface sizes: `ui-title` 15, `ui-body` and `ui-strong` 13, `ui-small` 12, `meta` 12 (Inter), `ui-label` and `meta-mono` 11. Nothing below 11pt. Column titles are `ui-strong` in title case ("Todo"); `ui-label` uppercase stays for section labels such as "Not written yet".
- Reading sizes: `read-h1` 20/26, `read-h2` 16/22 and `read-h3` 14/20, all SemiBold, so `##` and `###` read as sections under the title instead of rivalling it. `read-body` is 15/24 in a `read-measure` 560pt column, about 75 characters per line (measured 70 to 79 on prose); the sheet is `reader-max` 624, the column plus 32pt each side.
- The editor uses `edit-body` (mono 13/20) so markdown syntax lines up and is obviously "raw".

## Space and layout

- 4pt base: `space-1` 2 … `space-7` 32. Window gutter and column gap `space-5` (16). Gap between cards, and between a card's rows, `space-3` (8). Card padding `space-4` × 10.
- Columns are equal width between `column-min` 200 and `column-max` 420. On a 14" MacBook Pro (1512pt wide) four columns sit near 360; with fewer folders they cap at 420 and the spare room stays empty on the right instead of stretching cards.
- Window skeleton, top to bottom: header bar (`bar-h` 40) · board or page · status bar (`statusbar-h` 24). The title bar is transparent: the traffic lights sit on the header bar's centre line, 20pt from the window's top (`titlebar-light-y`), centres at x 20, 40, 60, inside the leading `titlebar-lead` 80pt; no modal frame enters the top `titlebar-h` 28pt.
- Radii: `radius-sm` 4 for controls, `radius-md` 6 for cards and pages, `radius-pill` for 18pt pills. Cards are flat; `shadow-popover` is only for the review panel, menus and a card being dragged; modals use the stronger `shadow-modal`.
- Modals come in four sizes and one anatomy (head 40 · scrolling body · foot 40 with one primary, rightmost): **S** 360 asks a question (ConfirmDialog, FoldersDialog), **M** 480 holds a form (Settings, GroupsDialog), **L** 760 holds one document, **XL** fills the window minus a 32pt inset, at most 1200 × 900, for work surfaces where you write at length or an agent's answer streams in (NewIssueModal, RefinePanel). Only S closes on a scrim click; XL never asks on `esc`, it parks your input. See Modal.
- Scrollbars are overlays that never take layout width and are hidden at rest, like macOS overlay scrollbars: a 4pt `scrollbar` thumb while scrolling, 8pt `scrollbar-active` when the pointer reaches the pane's edge, `accent` while dragging. No track, no arrows. See Scrollbar.
- Two panes side by side are separated by the 16pt gutter, and that gutter is the resize handle: a 2×3 dot grip in `ink-faint` at rest, a full-height 2pt `line-strong` rule under the pointer, `accent` while dragging. It never overlaps a pane's scroll strip. See PaneDivider.
- Anything slow (stats from git history, charts, an agent's answer) shows Skeleton blocks at the content's final size: real labels and axes, data shapes in `surface-sunken`, all pulsing together on a 1.6s beat. Nothing before 300ms; the Thinking i and an elapsed clock from 2s; each block fills in on its own. See Skeleton.
- The Agenda shows commits per day across supervised projects: 18pt day squares in one `accent` ramp whose scale is rebuilt from what is shown, today ringed in `ink`, empty stretches of 4+ weeks folded, a per-project list with weekly bars, and the selected day's commits beside it. See Agenda.
- The Stats screen tracks maintenance cost: cycle time, complexity per kLOC, test coverage and lines of code as small multiples on one time axis with a shared crosshair (never a dual axis), a Refactor watch that turns 2+ signals into a "Draft a refactor issue" action, the biggest files with hotspots (complex and often changed), and the Agenda. See StatsScreen.

## Navigation

Navigation is four discreet pieces:

1. **Board button** at the far left of the Reader and Editor header: chevron and "Board", no edge like every header button, `surface-hover` on hover; `esc` shows beside it while ⌘ is held.
2. **Breadcrumb in the header**, in `ink-muted`: `iboard / ○ Todo`, the column with its status icon. The issue title joins it as the last segment (`ui-strong`, `ink`) only once the page's `read-h1` has scrolled out of view, and always in the Editor. Every segment but the last is a link back, with a hover fill and underline. On the board it shows only the project name. There the project name opens the ProjectMenu: the groups (each its own board), then your projects in your own order, scrolling inside the menu and filtered by typing, then a footer strip on `bg` with **Stats** (`⌘I`) over three actions: **Add project…**, **Edit groups…** (GroupsDialog, written back to `[[workspace.group]]` in `~/.config/iboard/iboard.toml`; the board reloads in place) and **Reorder** (drag or `⌥↑ ⌥↓`, saved as a user preference).
3. **Status bar at the bottom**, Inter `meta` in `ink-muted`: on the board the issues folder; on the Reader the file path, modified time and progress; in the Editor the path and the save pill. The current file path and save state sit on the left; the two or three shortcuts that apply to the current screen show on the right only while ⌘ is held, never more than three.
4. **Notification bell** at the very right of the header on every screen, carrying the count of AI-builder questions waiting. It opens the tray under it. See Notifications.

There is no sidebar and no tab bar. Three screens are enough: Board, Reader, Editor.

## Adding an issue

Capturing an idea takes one shortcut and a sentence. iboard names the issue from what you wrote; writing the rest can wait for the Editor, or Review can draft it while you watch.

- **Two entry points, one composer.** The "New issue" button in the board header (primary, `⌘N`) and the `+` icon button in the Todo column header both open the same **NewIssueModal**, an XL modal over the board. `⌘N` works from every screen.
- **Notes left, the file right.** You write (or dictate) in the notes pane; the right pane is a Reader sheet showing exactly what `↩` will write, at reading size and never cut short. Of the title states only Manual remains (QuickAdd's), shown on the sheet's title row.
- **Draft with Review (`⌘⇧R`) fills the sheet in place.** The known section headings appear at once with Skeleton bars, the section being written carries a `review` rule and caret, and the sheet follows the writing line. `esc` stops and keeps what arrived. The agent never writes the file. See NewIssueModal.
- **`↩` adds, and never waits.** It writes `todo/<slug>.md` (`# <title>`, then the notes, or the draft when there is one) and closes the modal. With an empty title the file is `unnamed-ticket-xxxx` until the engine's title lands in the background, within 15s, renaming the file and setting its heading; the card reads "Naming…" meanwhile.
- **`⌘↩` adds and opens the Editor**; the title lands once the Editor closes. `⇧↩` is a new line in the description.
- **Never blocked.** With the engine Off or unavailable, the first line of the description becomes the title. Failing or too slow, the issue stays `unnamed-ticket-xxxx`.
- **Long notes** have the whole notes pane; nothing is capped or truncated.
- **`esc` closes** the modal and parks the notes (`.iboard/new-issue.md`); the next `⌘N` brings them back. Discard drops them. An empty note never creates a file.
- **Labels** come from `[Brackets]` at the start of the title: `[Discovery]` gives the label discovery, and `[BUG]` gives bug (see Label). The title is shown without them everywhere except the Editor.
- **File name** is the title without its labels, slugified (lowercase ASCII, hyphens, at most 60 characters). If the file exists, append `-2`, `-3`; the meta line shows the final name before the file is written.
- **The engine** (On-device Apple by default, Review agent, or Off) is chosen in Settings › Titles (`⌘,`).
- New issues always land in Todo. Creating directly in Doing or Done is not offered; move the card afterwards.
- **A new Todo issue lands at position 1**, at the top of Todo: the column is file-name ordered, so the files are renumbered (`001-…`) once the click that created it has settled.
- **Editing is open in Todo and Ready only.** Doing and Done tickets open read-only: no Edit button, no `E`, no double-click to edit. A Done ticket's Reader header instead offers **Commits**, with the newest hash shown beside it.

| Key | Board | Reader | Editor |
| --- | --- | --- | --- |
| `↑ ↓ ← →` | Move selection between cards and columns | `↑ ↓` scroll | Cursor |
| `↩` | Open selected issue | Edit (Todo, Ready) | New line |
| `E` | Edit selected issue (Todo, Ready) | Edit (Todo, Ready) | |
| `⌘↩` | Move selected issue to the next column (no modal open) | | |
| `esc` | Clear filter | Back to board | Leave editor (asks if unsaved) |
| `⌘S` | | | Save to file |
| `⌘N` | New issue (NewIssueModal) | New issue, over the page | New issue, over the page (the unsaved edit stays open behind it) |
| `⌘F` | Focus and widen the filter; `↓` reaches the suggestions | | |
| `⌘R` | Refresh from disk (Auto off) | Refine this issue (Todo; Refresh elsewhere, Auto off) | Refine draft (Todo) |
| `⌘[` | | Back | Back (asks if unsaved) |
| `⌘⇧R` | Review the selected issue (Todo) | Review this issue (Todo) | Review draft (Todo) |
| `⌘K` | Ask… on the selected issue (Todo, Ready, Doing) | Ask… (Todo, Ready, Doing) | Ask… (Todo, Ready, Doing) |
| `⌘I` | Stats (also in the ProjectMenu footer, and with that menu open) | | |
| `⌘Z` | Undo the last delete (asking off) or split, until the next key or pointer press; moving a card between columns has no undo | | Undo typing |
| `⌘,` | Open Settings (last tab used) | Open Settings | Open Settings |
| `⇧↩` | New line in the new issue's notes | | |
| `A` | Archive selected (Done only) | | |
| `L` | Add a label to the selected issue (Todo, Ready) | Add a label to this issue (Todo, Ready) | |
| `Del` | Delete selected (asks first unless turned off in Settings › General) | Delete this issue (same) | Delete key deletes text; use the Delete icon in the header |

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
| Reader (Todo, Ready) | Double-click description, `E`, `↩` | EditStarted | Editor |
| Reader | Click a ghost section or an "Add a section" chip | SectionAddRequested → EditStarted | Editor with `## <Section>` appended, cursor under it |
| Reader (Todo, Ready) | Double-click the empty sheet below the text | EditStarted | Editor, cursor at the end of the file |
| Reader | Draft them with Review | ReviewRequested → ReviewReceived | Review panel; proposed sections apply to the draft |
| Reader (Done) | Click Commits | CommitsOpened | Popup lists the commits whose message names the file (`closes <file>.md`) |
| Commits popup | Click a commit, or All | DiffOpened | Diff: the changed files on the left, the unified diff on the right; `⌘F` finds, `n p` and `] [` move by file and hunk, `esc` goes back to the issue |
| Editor | Type | DraftChanged | Editor, Unsaved pill |
| Editor | Save (`⌘S`, or auto on leave when Auto is on in Settings › General) | IssueSaved / SaveFailed | Reader / Editor with error |
| Any | Refresh, or file watcher fires | IssuesReloaded | Same screen; open draft kept, conflict flagged |
| Open a project (launch, or a switch) whose board folder lacks a standard folder | (none) | FolderSetupOffered | FoldersDialog over the board (`components/FoldersDialog`); once per project per launch, single-project boards only |
| FoldersDialog | Create 2 folders (`↩`) | FoldersCreated | Board reloaded in place; the new columns arrive empty, and the folder joins `.gitignore` when the box is left ticked |
| FoldersDialog | Not now (`esc`) | FolderSetupQuieted | Board; that project is not asked again, and Set up folders… opens it again |
| ProjectMenu | Set up folders… (only while one is missing) | FolderSetupOffered | FoldersDialog over the board |
| Board | Focus filter (`⌘F`, click) | FilterFocused | Field widens to 480pt, the three most used labels shown |
| Board, Reader | Click a label | LabelClicked → FilterSuggestionPicked | Board filtered by that label |
| Board | Click a suggestion, or type | FilterSuggestionPicked / FilterChanged | Board shows matches only; counts read "2/6" |
| Board | `esc` in the filter | FilterCleared | Full board; field back to 220pt on blur |
| Reader, Editor | Two-finger swipe right on the trackpad, or `⌘[` | NavigatedBack | One level up the breadcrumb: Editor to Reader, Reader to the board |
| Any | Move to column (drag, or the Reader's Move to…: `M` then its number) | IssueMoved | Same screen; file moved between folders; a standard column the project lacks is created |
| Board, Reader | Add label (card action, Reader header button, `L`) | LabelPickerOpened | LabelPicker under the trigger, field focused |
| LabelPicker | Type, `↩` (or click a board label, or `↓` then `↩`) | LabelAdded / LabelAddFailed | Same popup, field cleared; the label shows on the card and the Reader, and the file is rewritten |
| LabelPicker | Click a label the issue has | LabelRemoved | Same popup; the label turns off and the file is rewritten |
| LabelPicker | `esc`, click outside, trigger again | LabelPickerClosed | Board or Reader; text left in the field is dropped |
| Board | Start dragging a card | IssueDragStarted → DropZonesShown | Board, Archive and Delete zones slide up from the bottom |
| Board | Drop on Archive (Done only), or card Archive button, `A` | IssueArchived | Board; file moved to `.archive/` |
| Board | Drop on Delete, card trash, `Del` | DeleteRequested → DeleteConfirmRequested (asking on) | ConfirmDialog centred over the board |
| Reader, Editor | Delete icon in the header, `Del` (Reader only) | DeleteRequested → DeleteConfirmRequested (asking on) | ConfirmDialog centred over the page |
| Board, Reader, Editor | Any delete, with asking off | DeleteRequested → IssueDeleted | Board; status bar "Moved … to the Trash", `⌘Z` undo |
| Board | `⌘Z` after a delete | IssueRestored | Board; file and card back in place |
| ConfirmDialog | Delete (`↩`) / Cancel (`esc`) | IssueDeleted / DeleteCancelled | Board |
| Reader, Editor (Todo) | Review (`⌘⇧R`) | ReviewRequested → ReviewReceived | Review panel opens beside the page |
| Reader, Editor | AI split chevron | AIMenuOpened | Todo: Refine, Review, Deep or Quick review, Ask. Ready, Doing and Done have no AI split button |
| Reader | Move to… (`M`, or the header button) | IssueMoved | Menu lists the other columns numbered 1–9, and the standard columns the project lacks; `M` then a number moves the issue there, `esc` closes |
| Reader | Its actions: Refresh, Agent log, Draft design, Archive | IssueActionTaken | Header buttons, each shown where it applies: Refresh with Auto off; Agent log when the issue has one; Draft design with `idesign` on (not on Done) and Archive on Done. The Editor has none |
| Any | Hold ⌘ alone 0.5s / release or another key | KeyHintsShown / KeyHintsHidden | Same screen with key hints beside controls, in the filter, footers, popups and status bar (view state only, no file change) |
| Any | Settings (`⌘,`, Review menu, new issue engine tag) | SettingsOpened | Settings dialog on the matching tab |
| Settings › AI review | Pick tool, pick model, Save (`⌘↩`) | ReviewToolChosen, ReviewModelChosen → SettingsSaved | Back to the screen; Review menu shows the new tool and model |
| Settings › General | Pick a theme, Save (`⌘↩`) | ThemeChosen → SettingsSaved | Back to the screen, drawn in the new theme |
| Settings › General | Toggle "Ask before deleting an issue", Save (`⌘↩`) | DeleteConfirmToggled → SettingsSaved | Back to the screen; deletes ask or not |
| Settings › Titles | Pick engine, Save (`⌘↩`) | TitleEngineChosen → SettingsSaved | Back to the screen; new issue tag shows the engine |
| Board | Stats in the ProjectMenu footer, `⌘I` | StatsOpened | Stats; on a group's board the status bar says Stats measure one project |
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

Motion is short and only ever says where something went or what just became available. Durations are tokens (`motion` in `tokens.json`, `theme::MOTION_HOVER` and its neighbours in Rust, each passed through `theme::motion`):

- **Hover** fills and edges fade in over `motion-hover` 100ms and go at once when the pointer leaves (`widgets::hover_fade`). Keyboard selection never animates: a selected card, menu row or focus ring changes in one frame, so arrowing through a list never lags.
- **Key hints** wait `hint-hold` 0.5s of ⌘ held alone, then slide out over `motion-hint` 120ms; they hide in one frame on release or on any other key.
- **Scrollbars** appear while scrolling or when the pointer enters the pane's 12pt edge strip, stay `scroll-linger` 800ms after the last movement, then fade over `scroll-fade` 150ms.
- The rest is unchanged: the filter field widening (120ms), the drop zones sliding up (140ms), modals (scrim 120ms, frame 160ms, close 100ms), the Design panel's rail (220ms), the Thinking mark's rocking card.

When macOS asks for reduced motion (Accessibility › Display › Reduce motion), every one of them is skipped: states change in one frame, hints and scrollbars appear and disappear without fades, the dot stays still and the Thinking card rests at −8°. The app reads the setting once at launch and stores it with `theme::set_reduced_motion`; `theme::motion` turns any duration into 0 under it. `bundle.css` gates the same transitions behind `prefers-reduced-motion`, which reads the same setting in WebKit.

## Iconography

Line icons at 14pt, 1.6pt stroke, `currentColor`, drawn on the 24pt control grid: refresh, back chevron, edit pencil, plus, review, trash, archive, tag-plus, git node, play and square (stop). The repo has no icon set yet; use Lucide (MIT) glyphs of the same names, except trash, which is iboard's own soft bin (see the Icons card): a 15-wide lid, a flat-topped knob with rounded corners instead of a square tab, a body that tapers 0.65pt each side, and two long ribs. Use that one path everywhere a delete appears (card action, header Delete button, ConfirmDialog, Delete drop zone); never Lucide's straight-sided `trash-2`. Keep any icon paired with a word (the back chevron comes with "Board") except the per-card actions (trash, archive), which carry a tooltip and only appear when the card is hovered, selected or focused, and the header's dynamic icons, whose pill is their label.

## AI review

The Review button asks an agent (OpenCode, Claude, or any command the user
configures in `~/.config/iboard/iboard.toml`, from `⌘,`) to challenge the open
issue. The tool and model are chosen in the ReviewSettings dialog: a Tool select (OpenCode, Claude Code, Custom command) and a Model select whose first option, Default, uses the tool's first favorite model:

- **Placement**: the AI split button in the Reader and Editor header, after the breadcrumb and before the issue's actions: `review` text, no edge, `review-soft` on hover. It sits after the Edit, Add label and Delete icons (see HeaderBar › Dynamic icon buttons). Its face follows the column: Refine (`⌘R`) on Todo, with Review (`⌘⇧R`), Deep or Quick review and Ask (`⌘K`) in its chevron; none on Ready, Doing or Done. Reviewing refines an issue, so Refine and Review belong to Todo only. The current tool and model sit at the foot of the menu (`⌘,`). Ask is named here so the menu is complete; its flow is specified separately. On the board, Review sits in the selected Todo card's hover strip.
- **What it returns**: three findings, always in this order, each with a verdict pill and one or two sentences.
  - *Relevance*: is this still worth doing, does it duplicate another issue?
  - *Size*: can one person or one agent finish it in one sitting, or should it split? It proposes the split.
  - *Clarity*: could someone implement it without asking a question? It lists missing acceptance criteria, files, or decisions.
- **Verdicts**: Pass (`status-done`), Challenge (`review`), Block (`danger`), always with the word.
- **From an idea to an issue**: when the issue is only a title, the review drafts the missing sections (Context, Acceptance criteria, Out of scope, Open questions) as a proposal. *Apply to draft* opens it in the Editor, unsaved; the agent never writes the file itself.
- **States**: idle, running (button shows "Reviewing…", `esc` cancels), result (panel open), failed (inline `danger` text with retry). Only the issue text is sent; interactive agents are run in one-shot mode, so no terminal is attached yet.
- **Refining (RefinePanel)**: an agent in a role (the Challenger) pushes back on the issue and offers numbered resolutions. The panel has two panes: the TL;DR (`ui-title` with a 3px `review` rule) and the reasoning (`read-body`, in `read-measure`) on the left, the choices on the right so they never scroll away. Each choice is a bordered card with a short title, a muted detail, its number key and a radio; the selected one takes the `accent` edge and `accent-soft` fill. The note under the choices follows the selection ("Add a note to choice 1", or "Or answer in your own words"). Send is the one filled `review` button and names what goes out ("Send choice 1", "Send answer"). Under 900pt it stacks and folds the reasoning under the TL;DR. See `components/RefinePanel`.
- **Waiting (Thinking)**: while the agent works (often about 10s), the iboard "i" stands in for a spinner: its blue dot stretches across the stem, the stem tucks into the freed corner, and the two hand over until the i stands again a quarter turn on (1.6s per quarter, a full square in 6.4s). The panel opens at its final size with placeholder bars and cards, the status line says what it waits for with an elapsed clock ("Challenger is reading the issue · 4s"), and after 20s it says it is slow and that `esc` cancels. No fake progress. See `components/Thinking`.

### Build logs

The build's transcript sits beside its issue in the side panel: the head says whether a run is live, the **Now** strip owns the current step and its clock above the scroll, each run opens under a heading with its agent and start time and closes on its outcome, and every burst of tool calls between two messages folds into one **activity row** ("Read 5 files, searched 2, ran 1 command") that opens in place. Edits are not activity: each run ends with a **Changes** block, "5 files changed +214 −63", one row per file, and a row opens the Diff screen on that file. The switch offers **Timeline**, the folded story, and **Raw**, the exact log line-numbered in mono. The composer is the panel's one filled `primary` button and follows the state ("Reply to the agent…", "Ask for another pass…", "Ask it to fix what failed…"); while a question waits it is replaced by "Answer the question above to continue." See `components/AgentLog`.

### Build questions

A build that needs a person stops on a **Question** card at the top of its agent panel, inside the ticket, above the timeline. The bell counts it and the Doing card pins it, but neither is where it is answered; the card is, and answering it resumes the build. See `components/Question`.

- **The card is the agent talking**, so its bubble, "The agent asks" label and filled Send are `review`. The Doing orange stays on the card pin and the bell badge, where it means the ticket is blocked, not who is speaking.
- **One card for one field or six.** The agent's title is the heading; a field label repeats it only when it adds something. Options are 36pt rows with their description always visible and a number key to pick; an open answer is Inter 13, never mono. The card is bounded to 55% of the panel, its fields scroll and Send stays pinned. A required field left empty says so under itself and takes focus; Send is never a disabled button with no explanation.
- **While it waits, the composer goes.** The box under the timeline is one quiet line, "Answer the question above to continue.", so the panel has one filled Send, not two. Closing the panel is allowed; the pin and the bell reopen the ticket on the question.
- **A tool permission is a fallback, not a step.** The builder's scope is its project folder. The same card shows the exact resource in mono, says "Inside `<project>`", and "Always allow" names the project. When a resource is outside it, one `danger-soft` band says the builder should stay in its own folder, and Always allow is not offered. There is no global allow.

## Previewing a component

- A `preview.html` is a **fragment**: it carries no stylesheet link, because the design tool and `idesign` splice `tokens.css` and `bundle.css` into it. A bare fragment opened in a browser is unstyled, and that is not a bug to fix in the file.
- **`make preview C=FoldersDialog`** composes one through `idesign` and prints two paths: `/tmp/iboard-previews/<Name>.html`, the fragment with the system inlined, and its rendered PNG. Double-click the HTML to look at it, scroll it, and resize it; that is the same page the catalog renders.
- **`ARGS` go through**: `make preview C=FoldersDialog ARGS="--theme dark"` renders the dark system, `ARGS="--height 1600"` a tall page whole instead of cut at the renderer's default.
- Both font faces fall back to the system's in this snapshot: the bundle's font import is dropped because neither a file snapshot nor the catalog may reach the network. Kerning will differ from the app; measure nothing from it.
- `design/.verify/` holds test renders and is ignored by git.

## Using the tokens from Rust

Tokens are points, which map 1:1 to egui points and to CSS px on a Retina display. The header font (Ubuntu) and widget shapes in the screenshot look like egui's defaults, so the companion `theme.rs` in the project files maps these tokens onto `egui::Visuals` and `egui::Style`. If the app uses iced, Slint or Tauri instead, the same `tokens.json` drives it; only the mapping file changes.

egui has no font weights: register `Inter-Medium`, `Inter-SemiBold` and `JetBrainsMono-Medium` as their own named families and use them where a style says 500 or 600 (card titles, column labels, the project name, `meta`). `theme.rs` does this in `font_definitions`.
