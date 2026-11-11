# FoldersDialog

When a project's board folder exists but lacks one of the four standard column folders (`todo/`, `ready/`, `doing/`, `done/`), iboard offers to create them when that board opens, rather than leave you on a board that is quietly short a column. The folder itself is editable here, because not every project calls it `issues/`.

## The folder is the project's, not iboard's

- **The board folder is whatever the project's `iboard.toml` says.** `root = "tickets"` at the project root names it; with no config it is `issues/`. Discovery, the scan, `init`, Move to…'s missing-column creation and this dialog all read that one value.
- **The dialog never joins a literal `issues`.** It names, reads and creates in the resolved root, so a project that calls its board folder `tickets/` is never given a second, empty `issues/`.
- **Renaming writes the config first.** When the name in the field differs from the root on disk, the dialog writes `root` to the project's `iboard.toml` *before* creating anything, keeping every other key in that file (and creating the file when there is none). The next launch resolves the same folder, so the choice sticks.
- **Nothing is moved.** The folder that was there keeps its files and simply leaves the board. The sentence says so when the name changes: "The board moves to `tickets/`; `issues/` stays where it is, off the board."

## When it appears

- **Whenever a single-project board opens** — at launch, and on every project switch from the ProjectMenu or **Add project…** — when the resolved root exists and at least one of the four standard folders is missing. Never on a group board: a group merges repositories, and one project's missing folder is not a missing column there. Never on a later refresh, a rescan or an agent landing an answer in the same board.
- **Once per project per launch.** A project that has been shown the dialog is not shown it again in the same run, whether it was answered or dismissed; switching back and forth between two projects does not repeat it.
- **A project with no board folder at all is not this dialog.** That stays the error strip's button (and the WelcomeScreen on a first run); both call `init`, which honours `root` too, so they create the configured folder and not `issues/`. The strip names the folder it will create — **Create tickets/** when the config says `tickets` — so it never promises `issues/` and writes something else.
- **Asked once per project, remembered.** *Not now* remembers the project in the user config, so no later launch asks again. The way back is **Set up folders…** in the ProjectMenu footer, a row that appears only while that project is missing a standard folder, and which clears the memory.
- A folder counts as present by its **kind**, not its name: `wip/` is Doing and `completed/` is Done, so those projects are not asked for `doing/` and `done/` as well.

## Layout

A **Modal S**: `modal-s` (360pt) on `surface`, `line` edge, `radius-md`, `shadow-modal`, 20pt padding, centred over the dimmed board. No head bar (S has none): the title is the header and *Not now* is the close.

- **Title**, `ui-title`: "Board folders". A noun, and deliberately not the folder's name: the name is in the field, once.
- **Subject**, the `surface-sunken` box of every S: the project in `ui-strong` ("shop"), its path in `meta` `ink-muted` ("~/code/shop").
- **Folder field**: a label "Folder", then an `ib-input--mono` field holding the name alone. Mono because it is a path segment typed by hand and every character counts. A `meta` helper under it says where the name comes from when a config sets it: "Set in `iboard.toml`." Nothing when it is the default `issues`.
- **Keep this folder out of git**, a checkbox under the field, drawn **only when the dialog will create the folder** — a name that does not exist yet. Checked by default, with a `meta` helper naming the exact entry it writes: "Adds `/tickets/` to `.gitignore`." A folder that already exists never gets the box: iboard does not suggest ignoring a board someone else may already be tracking.
- **Body**, `ui-small` `ink-muted`: the count and the consequence — "Two of the four columns are missing from this board. Each is created empty, with a `.gitkeep` so git keeps it." When the typed name differs from the root on disk, these give way to the rename sentence above, which is the consequence that matters then.
- **The four rows**, in canonical order Todo, Ready, Doing, Done, so the gap is seen in place rather than listed:
  - each row is 26pt: the column name in `ui-strong`, the folder that backs it in `meta` `ink-faint` (`todo/`, or `wip/` when that is the folder), then the state right-aligned in `meta`;
  - a **present** row is muted: name `ink-muted`, state "present";
  - a **missing** row is the content: name `ink`, state "missing" in `ink` at 500. No colour: a missing folder is not a failure, so nothing here is `danger`, and it is not the agent's business, so nothing is `review`.
  - **No status icons in this list.** The first draft had them, and they lie: Done's icon is a filled circle with a check, so "Done ✓ … missing" reads as a completed item next to the word missing. The name and the folder are the identity here; the board behind keeps the icons where they mean a column, not a state.
- **Foot**: *Not now* (secondary, `line-soft` hairline) then **one** filled `primary`, rightmost, in `primary` ink: "Create 2 folders" (or "Create folder"), key `↩`.
- **Keys**: `↩` creates, `esc` is *Not now*. An M would take `⌘↩`; this is an S with one short field, and `↩` submits it as in a single-field prompt.

## The folder name

- **At most 20 characters**, enforced by the field itself (`maxlength`), so the limit is a fact of the control and not an error a user can hit by typing.
- **What is allowed**: letters, digits, `-`, `_` and inner `.`; not empty, not `.` or `..`, no `/`, no leading dot. A name that breaks this shows a `danger` line under the field naming the rule ("A folder name has no slash.") and the primary is disabled — it never fails silently. The current name is selected on open, so typing replaces it.
- **The rows follow the name.** They read the four states from `project_dir/<name>` as it is typed, so the count on the primary never lies: a fresh name shows four missing, a folder that already has its columns shows four present.
- **A name that is already complete** changes the primary: it reads "Use `tickets/`" and only writes `root`, since there is nothing to create. That is the one case where the dialog moves the board without adding a folder.
- **No second folder, ever.** Whatever is typed, the dialog creates exactly one folder — the one it names — plus the column folders inside it, and writes `root` so a later launch agrees.

## Keeping it out of git

A board folder is the author's workbench: cards, drafts, half-formed notes. A project that gives it a name of its own usually does not want its teammates to clone it. So when this dialog creates the folder, it also offers to keep it out of git, checked by default.

- **What it writes**: one line in the project's `.gitignore`, root-anchored and folder-only — `/tickets/`, or `/notes/tickets/` when the root is nested. The file is created when there is none; existing lines are never rewritten or reordered.
- **Only for a new folder.** The box is not drawn when the folder already exists, so the dialog can never suggest ignoring a board with a history, tracked or not. Unticking leaves the folder exactly as git sees it, which is untracked until the user stages it.
- **Order of work**: write `root`, then `.gitignore` when the box is ticked, then create the folders. If the ignore write fails, nothing is created and the danger line says so — no board silently lands outside git when the user asked for it to stay inside.
- **Each project decides once.** The entry lives in that project's `.gitignore`, so a teammate who clones the repo sees the project without the board, as intended, and `iboard init` on a fresh clone is unaffected.

## States

| State | What is on screen |
| --- | --- |
| Offered | The field with the current root, the count sentence, the four rows, *Not now* / *Create 2 folders* |
| Renamed | The name changed: the rename sentence replaces the count, rows follow the new name, *Create 4 folders*, and because the folder is new, *Keep this folder out of git* is drawn, ticked |
| Already complete | All four rows present, primary reads *Use `tickets/`* |
| Invalid name | A `danger` line under the field, primary disabled |
| Created | The dialog closes; the board reloads in place on the named folder and the new columns are there, empty, with their dashed "No issues in done/" well. No status bar message: the columns arriving is the answer |
| Failed | The dialog stays open with the folders that still do not exist, and a `danger` line above the foot: "Couldn't create done/: permission denied." The primary stays enabled for a retry. Creating is per folder, so a half-finished run loses nothing. If the config could not be written, that is the line instead, and nothing is created |
| Not now | The dialog closes, the board opens as it was, and the project is remembered as quiet |

## A modal that opens by itself, on purpose

The Modal rules say a modal never opens by itself: background events go to the status bar. This is the one exception, and it is bounded so it stays an exception.

- **It runs on every project open** — launch and switch — because a project is a new context and the board in front of you should be true when it appears. Two limits keep that from becoming noise: once per project per launch, and only while a standard folder is actually missing. Switching back to a project in the same run never asks again, and *Not now* is remembered across launches.
- **Dismissing it is always safe.** `esc` leaves the board exactly as it was, and Set up folders… in the ProjectMenu is the way back, so nothing is lost by closing it.
- **It is still not a notification.** Nothing arrives in the background, no agent finishes behind it, and once *Not now* is answered iboard never says it again.

If a project switch ever feels like too much, the calmer alternative is a single "2 folders missing" line under the header that opens this dialog, instead of opening it — the dialog itself does not change.

## What the consumer provides

The resolved `root` (config name or default), the four columns with their presence, the project name and path, the report of what failed, and the handlers: `create_folders(name, missing, ignore)` — which writes `root`, appends the `.gitignore` entry when `ignore` is set, then creates the folder and the `.gitkeep`s, or only writes `root` when nothing is missing — and `quiet(project)`. Emits FolderSetupOffered, BoardRootChanged, FoldersCreated, FolderSetupQuieted. Needs no new token: `modal-s`, `space-*`, `radius-*` and `shadow-modal` already carry it.
