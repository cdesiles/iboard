# iboard

A fast, local kanban board and markdown reader for the `issues/` folders used
across projects. One Rust binary, no external service, no browser — it reads the
markdown you already keep in git.

Point it at any project that has an `issues/` directory:

```sh
iboard                 # scan the current directory (and parents)
iboard ~/code/moon-cull
iboard init ~/new-app  # create issues/ (todo, ready, doing, done) and open it
iboard --list          # print the board to stdout, no window
```

## What it does today

- **Board** — one column per issues folder, one card per markdown issue, each
  showing the title, a summary clamped to three lines and the file name. Column
  headers carry a status dot, an uppercase name, the count and a colored rule.
  The arrow keys select a card (across columns, skipping empty ones); `↩`
  opens it and `E` edits it. Coming back from an issue keeps it selected. Cards use a
  hand cursor and are not text-selectable: a single click opens the issue, and
  dragging moves it.
- **Switch project** — the path in the board header is a menu. It lists the
  projects opened recently, most recent first, with the current one in accent;
  the last row opens the OS folder picker to choose another project containing
  an `issues/`. The list is cached at
  `$XDG_CACHE_HOME/iboard/recent-projects`, or
  `$HOME/.cache/iboard/recent-projects` when `XDG_CACHE_HOME` is unset,
  and folders that have since disappeared are skipped.
- **Done, most recent first** — Done cards show the day they were done
  (`YYYY-MM-DD`, the full ISO datetime on hover) and are sorted newest first.
  The date is the last commit that touched the file; an issue git does not
  know yet uses its file times, so a card just dragged into Done goes on top.
- **Ready, a queue for agents** — issues go `todo → ready → doing → done`.
  Todo holds ideas that may still need refining; Ready holds the ones an
  agent can start on. Dropping a card on Ready numbers it after the last one
  there (`make-colour-green.md` → `001-make-colour-green.md`), so the
  Ready column, sorted by name, is the priority order: rename a file to
  reorder. Moving it out of Ready drops the number again. An agent takes the
  first Ready issue by moving it to `doing/`. Doing, like Done, shows the
  most recently edited issue first.
- **Reorder by dragging** — drop a card at a position inside Todo or Ready and
  the column is renumbered (`001-`, `002-`, ...) to match the new order, so the
  board order and the file-name order never drift apart. Doing and Done ignore
  position: they stay most recently edited first.
- **Delete** — the trash icon on a card, `Del` on the selected card, the
  *Delete* button in the reader and editor header, or a drop on the Delete zone
  moves an issue to the Trash. It asks first by default; `esc` or *Cancel*
  backs out. Turn *Ask before deleting an issue* off in *Settings › General*
  (`⌘,`) to delete at once: the status bar then offers `⌘Z` to put the file
  back for 10 seconds.
- **Archive** — Done cards and the reader header carry an archive icon that
  moves the issue to `issues/.archive/`, a hidden folder discovery never
  reads. Nothing is deleted, and an existing archived file is never
  overwritten.
- **Drag and drop** — drag a card onto another column to move that issue: a
  copy of the card follows the cursor for the whole drag, so you always see what
  you are carrying. On drop the markdown file is renamed into the target folder,
  keeping its file name; the target column highlights while you hover it. A move
  never overwrites an existing file; it reports the clash instead.
- **Reader** — click a card to open the full file rendered as markdown
  (headings, lists, code blocks, tables) on a centred sheet that reaches
  down to the status bar. Its head shows the column, the path, when the file
  was modified and the checklist progress (or the word count). Ticking a
  `- [ ]` item writes it back to the file. Outside Done, the standard sections
  the issue still lacks (Context, Acceptance criteria, Out of scope, Open
  questions; set `[reader] sections` in `iboard.toml`, `[]` turns it off) show
  as ghost blocks with *Draft them with Review*, or as *Add a section* chips
  once some are written; a click opens the editor with that heading appended.
  Clicking the empty sheet below the text edits at the end of the file. The `‹ Board` button at the left of the
  header, a breadcrumb segment, `Esc` or `⌘[` returns to the board.
- **New issue** — `⌘N`, the *New issue* header button or the `+` on the Todo
  column open an inline composer at the top of Todo. Describe the issue and
  press `↩`: iboard names it from the description and stops on the suggested
  title (selected, so typing replaces it); `↩` again writes `todo/<slug>.md`
  containing `# <title>` and the description, and stays open for the next one.
  `⌘S` adds as `↩` does; `⇧↩` and `⌘↩` are a new line. Clicking anywhere
  else on the board (another card, blank space) names if needed, writes the
  issue and closes the composer. A title you type yourself skips the naming. Titles come from Apple's on-device
  model (macOS 26+ with Apple Intelligence), the AI review agent, or nobody
  (Off), chosen in *Settings › Titles* (`⌘,`, or click the composer's engine
  tag); when the engine is off, unavailable, failing or slower than 3 s, the
  description's first line becomes the title. `Esc` closes the composer (or,
  while naming, lets you type the title); an empty composer never creates a
  file.
- **Editor** — from the reader, *Edit*, `E` or a double-click on the page
  opens the editor: the markdown source in mono inside an accent frame, plus an
  editable file name (capped at 255 bytes, the filesystem limit), with no
  side-by-side preview. With *Auto* on, edits
  (including a rename) autosave 800 ms after the last keystroke and leaving
  saves. With *Auto* off, or when a save fails, leaving the editor (`Esc`,
  `⌘[`, a breadcrumb, `⌘N`, closing the window) first asks *Save changes to
  …?* — Save, Discard or Keep editing. *Save* (`⌘S`) saves and returns to the
  reader, *Discard* drops unsaved changes. If the file changes on disk
  while you have unsaved edits, you choose *Reload from disk* or *Keep mine* —
  it never clobbers silently.
- **AI review** — *Review* in the reader or editor header (`⌘⇧R`) asks the
  configured agent (OpenCode, Claude, or any command) to challenge the issue on
  relevance, size and clarity. It sends only the issue; the answer shows in a
  panel beside the page, each axis with a Pass, Challenge or Block verdict and,
  when useful, a proposed rewrite. *Apply to draft* opens that rewrite in the
  editor, unsaved: saving accepts it, Discarding refuses it. The agent never
  writes the file. `⌘,` configures the tool and model.
- **Filter** — a live text filter over titles, summaries and file names
  (`⌘F` to focus, `Esc` to clear). It widens while in use and suggests
  keywords (bug, feature) that become tokens matching the whole file; set
  your own with `[filter] suggestions = [...]` in
  `~/.config/iboard/iboard.toml`.
- **Status bar** — the current file, issue count or save state (Saved,
  Unsaved, Save failed), and the shortcuts that apply to the screen.
- **Auto** — the board reloads when files change on disk, so an editor or an
  agent writing an issue shows up immediately, and the editor saves as you
  type. Turn it off to save only on demand.
- **`init`** — `iboard init [path]` creates `issues/` with `todo`, `ready`,
  `doing` and `done`, each kept in git with a `.gitkeep`, then opens the board
  on it. It is safe to re-run: existing files are left untouched, and a
  configured root or an enclosing project is used when one is found.
- **`--list`** — a text rendering of the board, useful for scripts and for
  checking discovery without opening a window.

Writes are atomic (hidden temp file + rename), so a crash cannot leave an issue
half-written.

## Design

The look follows the design system in [`design/`](design/README.md): tokens in
`design/tokens.json`, the brand book in `design/README.md`, one guideline and
HTML preview per component under `design/components/`. The source of truth is
the [iboard design system on Claude Design](https://claude.ai/artifact/G22kpooZcDnB7Dd64xsJit);
`design/` is a local copy of it. `src/theme.rs` maps the
tokens onto egui (light and dark, following the system appearance) and
`src/widgets.rs` draws the controls egui renders differently. On macOS the
header bar doubles as the window's title bar. IBM Plex Sans and
Mono are bundled from `assets/fonts/` (SIL Open Font License, see
`assets/fonts/LICENSE.txt`).

## Install

From this repository:

```sh
cargo install --path .
```

This installs a single binary named `iboard`. Requires a recent Rust toolchain.

## How the issues are discovered

Discovery is intentionally lenient, because the format is intentionally free.

1. Starting from the given path (or the current directory), it looks for a
   `issues/` directory, walking up through parent folders. A config file below
   can override the location.
2. Inside the issues root, it builds columns from:
   - **Known sub-directories**, always shown even when empty, in kanban order:
     `backlog`, `todo`, `ready`, `doing`/`in-progress`/`wip`, `review`, `later`,
     `blocked`, `done`.
   - **Other sub-directories**, shown only if they directly contain markdown
     files.
   - **Markdown files at the issues root**, which form the first column
     (`Todo`, or `Backlog` when a `todo/` directory also exists).
3. Files are ordered naturally, so `2-foo.md` comes before `10-bar.md`.
   Doing and Done are the exception: most recently edited first.

### Issue title and summary

Nothing about the markdown is required:

- **Title**: first `# Heading`, else the first non-empty line, else a prettified
  file name (`2026-05-29-merge-nut-exporter` → `Merge nut exporter`).
- **Summary**: the first prose paragraph (or first list item if there is no
  prose), stripped of markdown syntax and truncated to 180 characters with
  `...`.

## Optional configuration

Drop an `iboard.toml` at the project root to override
discovery:

```toml
root = "issues"                        # where the issues live
columns = ["backlog", "doing", "done"]  # explicit order; omit to auto-detect
root_column_name = "Backlog"            # name for the root-level files column
title = "Moon Cull"                     # window title
```

### AI review

The review agent is configured once, for every project, in
`$XDG_CONFIG_HOME/iboard/iboard.toml`
(`$HOME/.config/iboard/iboard.toml` when `XDG_CONFIG_HOME` is unset), or from
the app with `⌘,`:

```toml
[ai]
tool = "opencode"   # "opencode", "claude" or "custom"
model = "default"   # or a model ID from the tool's list
command = ""        # custom only, e.g. "llm -m {model} < {prompt}"

[titles]
engine = "apple"    # "apple" (on-device), "agent" or "off"

[delete]
confirm = true      # false: delete without asking, with ⌘Z undo
```

`default` runs the tool's first favorite model: OpenCode's first favorite, or
the `model` in Claude Code's settings. OpenCode and Claude Code run
non-interactively (`opencode run`, `claude -p`); a custom command runs through
`sh` with `{prompt}` (a temp file holding the issue and the review
instructions), `{model}` and `{issue}` (the issue path) filled in, and the
review is read from its stdout. An older `tool = "<any command>"` keeps working
as a custom command.

## Development

```sh
cargo run -- --list ~/code/moon-cull   # exercise discovery without a GUI
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Layout:

| File | Responsibility |
| --- | --- |
| `src/main.rs` | CLI entry point and `--list` output |
| `src/discovery.rs` | Locate the issues, build columns, order issues |
| `src/issue.rs` | Load a markdown file, derive title and summary |
| `src/config.rs` | Optional `iboard.toml` |
| `src/agent.rs` | The configured AI tool, the review prompt and its answer |
| `src/titles.rs` | Names a new issue from its description (on-device, agent or off) |
| `macos/titles.swift` | Swift shim over Foundation Models, built and weak-linked by `build.rs` |
| `src/filter.rs` | The board filter: suggestion tokens and free text |
| `src/recents.rs` | The recent-projects cache behind the header menu |
| `src/dialog.rs` | The OS folder picker (macOS `NSOpenPanel`) |
| `src/app.rs` | egui board, quick-add composer, reader and editor |
| `src/theme.rs` | Design tokens, fonts and line icons mapped onto egui |
| `src/widgets.rs` | Buttons, checkbox, pills, key hints, breadcrumbs |
