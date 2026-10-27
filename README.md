# iboard

A fast, local kanban board and markdown reader for the `issues/` folders used
across projects. Two small local binaries (`iboard` and `idesign`), no external
service, and no browser in the app's own process — it reads the markdown you
already keep in git.

Point it at any project that has an `issues/` directory:

```sh
iboard                     # scan the current directory (and parents)
iboard ~/code/my-project   # scan the given directory
iboard init ~/new-app      # create issues/ (todo, ready, doing, done) and open it
iboard --list              # print the board to stdout, no window
```

## What for?

`iboard` is suitable for every small project that does not require a complex integration workflow with GitLab, GitHub or what not. It makes ticketing fast: creation, follow-up... It's a matter of reading and writing local files, for humans and AI agents.

## Design

The look follows the design system in [`design/`](design/README.md): tokens in
`design/tokens.json`, the brand book in `design/README.md`, one guideline and
HTML preview per component under `design/components/`.

### The design-system toolchain

`idesign` creates, browses, imports and renders a project's design system; the
board's **Design** button opens the same system in a panel. Running `idesign`
alone in a terminal opens the live catalog, an interactive page of every
component in a WebKit window (`macos/catalog.swift`); `idesign --browser` opens
the board's panel in its own window instead. A component preview is turned into
a PNG by a short-lived WebKit helper (`macos/render.swift`). `build.rs` builds
both helpers, and both run as their own processes, so the board never loads a
browser itself: startup stays instant and a broken preview cannot take the
window down. The binaries carry both helpers and write them out to
`~/.cache/iboard/helpers` on first use, so an installed copy keeps them after
`cargo clean`; `IBOARD_RENDER` and `IBOARD_CATALOG` point at other builds.
Previews render with JavaScript off, from a folder of their own.

```sh
idesign                         # open the live catalog (in a terminal)
idesign --browser               # open the design panel in a window
idesign show                    # summary of the current project's system
idesign list                    # one line per component
idesign validate                # token references, missing previews, missing docs
idesign add-component Toast --group Controls --intent "Transient confirmation"
idesign import ~/packages/acme  # an Open Design package -> design/
idesign preview Button --out /tmp/button.png
idesign render page.html phone.png
```

`design/` is the canonical store; an Open Design package is an import source,
never a second format. The **New component** form can scaffold a component, and
optionally ask the configured agent to draft its preview fragment (the same
`idesign`/opencode tooling; the answer is written to `preview.html`). The source
of truth for iboard's own system is the
[iboard design system on Claude Design](https://claude.ai/artifact/G22kpooZcDnB7Dd64xsJit);
`design/` is a local copy of it. The toolchain takes inspiration from
[Open Design](https://github.com/nexu-io/open-design) (Apache-2.0); see
[`NOTICE`](NOTICE).

## UX enrichment

Opening an issue offers **Draft design**: the configured agent drafts a static
HTML mockup of the change, bound to the project's design system when there is
one (read-only), and iboard renders it beside the issue with a field to refine
it. The artifact is written to `assets/<issue-stem>/ui.html` (and `ui.png`), so
it stays with the ticket and can be linked from its markdown. Relative image
links — `![mockup](assets/<stem>/ui.png)` — render in the Reader, resolved
against the issue's folder. It works with no design system too, falling back to
restrained defaults.

## Building and installing

From this repository, `make` (or `make help`) lists the targets:

```sh
$ make

Usage:
  make <target>
  help       Show this help

Setup
  hooks      Lint the staged snapshot before each commit (installs .githooks)

Run
  run        Launch a binary, iboard by default: make run, make run iboard, make run idesign
  iboard     Launch the board directly, e.g. make iboard ARGS="~/code/moon-cull"
  idesign    Launch the design tool directly, e.g. make idesign ARGS="show"
  list       Print the board to stdout, e.g. make list ARGS="~/code/moon-cull"

Quality
  check      Everything a ticket needs before it is done: formatting, lints and tests
  test       Run all tests
  lint       Check formatting, clippy, an egui-free core and swift-format (fails on warnings)
  fmt        Format code in place

Build
  build      Build the binaries (debug)
  release    Build the binaries (optimized)
  install    Install the binaries into the Cargo bin directory
  clean      Remove build artifacts
```

`make check` is the bar for every change. The Rust version is pinned in
`rust-toolchain.toml`, and the limits on function length (`clippy.toml`) and
file length (`tests/layout.rs`) only ever go down. The agent prompts are
snapshot-tested with `insta`: after a deliberate change, run
`INSTA_UPDATE=always cargo test` and review the `.snap` diff.

## Issues format 

The format is intentionally free.

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
root = "issues"                         # where the issues live
columns = ["backlog", "doing", "done"]  # explicit order; omit to auto-detect
root_column_name = "Backlog"            # name for the root-level files column
title = "My foo project"                # window title
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

idesign is in beta and off by default: the board shows no Design button and
no Draft design, and a project's `design/` folder stays as it is on disk. It
has no Settings toggle; turn it on in the same file, then restart:

```toml
[idesign]
enabled = false     # true: the Design panel and Draft design
```

`default` runs the tool's first favorite model: OpenCode's first favorite, or
the `model` in Claude Code's settings. OpenCode and Claude Code run
non-interactively (`opencode run`, `claude -p`); a custom command runs through
`sh` with `{prompt}` (a temp file holding the issue and the review
instructions), `{model}` and `{issue}` (the issue path) filled in, and the
review is read from its stdout. The placeholders become the quoted variables
`"$IBOARD_PROMPT"`, `"$IBOARD_MODEL"` and `"$IBOARD_ISSUE"`, so a file name can
never run as shell code.

An agent is stopped, with everything it started, when its window closes, when
a new run replaces it, when iboard quits, or after 10 minutes without an
answer.
