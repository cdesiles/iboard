# iboard

A fast, local kanban board and markdown reader for the `issues/` folders used
across projects. One Rust binary, no external service, no browser — it reads the
markdown you already keep in git.

Point it at any project that has an `issues/` directory:

```sh
iboard                     # scan the current directory (and parents)
iboard ~/code/my-project   # scan the given directory
iboard init ~/new-app      # create issues/ (todo, ready, doing, done) and open it
iboard --list              # print the board to stdout, no window
```

## What for?

iboard is suitable for every small projects that does not require a complex integration workflow with 

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
engine = "apple"    # "apple" (on-device), "agent" or "manual"

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
