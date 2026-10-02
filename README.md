# roadmap

A fast, local kanban board and markdown reader for the `roadmap/` folders used
across projects. One Rust binary, no external service, no browser — it reads the
markdown you already keep in git.

Point it at any project that has a `roadmap/` directory:

```sh
roadmap                 # scan the current directory (and parents)
roadmap ~/code/moon-cull
roadmap --list          # print the board to stdout, no window
```

## What it does today

- **Board** — one column per roadmap folder, one card per markdown ticket, each
  showing the title, a summary clamped to three lines and the file name. Column
  headers carry a status dot, an uppercase name, the count and a colored rule.
  The arrow keys select a card (across columns, skipping empty ones); `↩`
  opens it and `E` edits it. Coming back from a ticket keeps it selected. Cards use a
  hand cursor and are not text-selectable: a single click opens the ticket, and
  dragging moves it.
- **Done, most recent first** — Done cards show the day they were done
  (`YYYY-MM-DD`, the full ISO datetime on hover) and are sorted newest first.
  The date is the last commit that touched the file; a ticket git does not
  know yet uses its file times, so a card just dragged into Done goes on top.
- **Ready, a queue for agents** — tickets go `todo → ready → doing → done`.
  Todo holds ideas that may still need refining; Ready holds the ones an
  agent can start on. Dropping a card on Ready numbers it after the last one
  there (`make-colour-green.md` → `001-make-colour-green.md`), so the
  Ready column, sorted by name, is the priority order: rename a file to
  reorder. Moving it out of Ready drops the number again. An agent takes the
  first Ready ticket by moving it to `doing/`. Doing, like Done, shows the
  most recently edited ticket first.
- **Reorder by dragging** — drop a card at a position inside Todo or Ready and
  the column is renumbered (`001-`, `002-`, ...) to match the new order, so the
  board order and the file-name order never drift apart. Doing and Done ignore
  position: they stay most recently edited first.
- **Delete** — the trash icon on a card, `Del` on the selected card, or the
  trash in the reader header removes a ticket. It asks first, because it cannot
  be undone; `esc` or *Cancel* backs out.
- **Archive** — Done cards and the reader header carry an archive icon that
  moves the ticket to `roadmap/.archive/`, a hidden folder discovery never
  reads. Nothing is deleted, and an existing archived file is never
  overwritten.
- **Drag and drop** — drag a card onto another column to move that ticket: a
  copy of the card follows the cursor for the whole drag, so you always see what
  you are carrying. On drop the markdown file is renamed into the target folder,
  keeping its file name; the target column highlights while you hover it. A move
  never overwrites an existing file; it reports the clash instead.
- **Reader** — click a card to open the full file rendered as markdown
  (headings, lists, code blocks, tables) on a centred page. Ticking a `- [ ]`
  item writes it back to the file. The `‹ Board` button at the left of the
  header, a breadcrumb segment, `Esc` or `⌘[` returns to the board.
- **New ticket** — `⌘N`, the *New ticket* header button or the `+` on the Todo
  column open an inline composer at the top of Todo. Type a title: `↩` writes
  `todo/<slug>.md` containing `# <title>` and stays open for the next one; `⌘↩`
  writes it and opens the editor. The line under the field shows the file it
  will write (with a `-2` suffix, in amber, when the name is taken). `Esc`
  closes it; an empty composer never creates a file.
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
- **Filter** — a live text filter over titles, summaries and file names
  (`⌘F` to focus, `Esc` to clear).
- **Status bar** — the current file, ticket count or save state (Saved,
  Unsaved, Save failed), and the shortcuts that apply to the screen.
- **Auto** — the board reloads when files change on disk, so an editor or an
  agent writing a ticket shows up immediately, and the editor saves as you
  type. Turn it off to save only on demand.
- **`--list`** — a text rendering of the board, useful for scripts and for
  checking discovery without opening a window.

Writes are atomic (hidden temp file + rename), so a crash cannot leave a ticket
half-written. AI-assisted refinement is planned; see `roadmap/`.

## Design

The look follows the design system in [`design/`](design/README.md): tokens in
`design/tokens.json`, the brand book in `design/README.md`, one guideline and
HTML preview per component under `design/components/`. `src/theme.rs` maps the
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

This installs a single binary named `roadmap`. Requires a recent Rust toolchain.

## How the roadmap is discovered

Discovery is intentionally lenient, because the format is intentionally free.

1. Starting from the given path (or the current directory), it looks for a
   `roadmap/` directory, walking up through parent folders. A config file below
   can override the location.
2. Inside the roadmap root, it builds columns from:
   - **Known sub-directories**, always shown even when empty, in kanban order:
     `backlog`, `todo`, `ready`, `doing`/`in-progress`/`wip`, `review`, `later`,
     `blocked`, `done`.
   - **Other sub-directories**, shown only if they directly contain markdown
     files.
   - **Markdown files at the roadmap root**, which form the first column
     (`Todo`, or `Backlog` when a `todo/` directory also exists).
3. Files are ordered naturally, so `2-foo.md` comes before `10-bar.md`.
   Doing and Done are the exception: most recently edited first.

### Ticket title and summary

Nothing about the markdown is required:

- **Title**: first `# Heading`, else the first non-empty line, else a prettified
  file name (`2026-05-29-merge-nut-exporter` → `Merge nut exporter`).
- **Summary**: the first prose paragraph (or first list item if there is no
  prose), stripped of markdown syntax and truncated to 180 characters with
  `...`.

## Optional configuration

Drop a `roadmap-viewer.toml` (or `roadmap.toml`) at the project root to override
discovery:

```toml
root = "roadmap"                        # where the roadmap lives
columns = ["backlog", "doing", "done"]  # explicit order; omit to auto-detect
root_column_name = "Backlog"            # name for the root-level files column
title = "Moon Cull"                     # window title
```

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
| `src/discovery.rs` | Locate the roadmap, build columns, order tickets |
| `src/ticket.rs` | Load a markdown file, derive title and summary |
| `src/config.rs` | Optional `roadmap-viewer.toml` |
| `src/app.rs` | egui board, quick-add composer, reader and editor |
| `src/theme.rs` | Design tokens, fonts and line icons mapped onto egui |
| `src/widgets.rs` | Buttons, checkbox, pills, key hints, breadcrumbs |
