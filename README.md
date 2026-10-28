<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/iboard-wordmark-dark.svg">
    <img src="assets/iboard-wordmark-light.svg" alt="iboard" height="64">
  </picture>
</h1>

> [!NOTE]
> **iboard** is an opinionated issue tracker that lives entirely as Markdown files right next to your code.

⚡ Why iboard?

Designed for small, high-impact teams and solopreneurs, it eliminates the friction of bouncing between Jira, Notion, and GitHub. 
It is dead simple, `iboard` is a small binary with no external services, and no browser. Navigate, create, and resolve tasks entirely from your keyboard.

* Designed for tight feedback loops. Spend your time building the product, not tinkering with the project management software. 
* Completely open-source: bring your ideas, contribute and fork it! Fit your workflow to your needs.

## 🧠 How It Works

Issues are just `.md` files. Columns are just folders. Issues flow from left to right through your file system:

```text
📁 issues/
 ├── 📁 todo/   # Unrefined ideas and raw backlog
 ├── 📁 ready/  # Refined, actionable tasks (prefix with `001-` to prioritize)
 ├── 📁 doing/  # The active work in progress
 └── 📁 done/   # Finished, verified, and ready to push
```

Because everything is file-based, iboard integrates flawlessly with your IDE, your terminal, and any AI agents arround.

## Use-it!

```sh
iboard                     # start the app on the current working directory
iboard ~/code/my-project   # start the app on the given directory
iboard init ~/new-app      # create issues/ (todo, ready, doing, done) and configuration
iboard --list              # print the board to stdout, no window
iboard --all --list        # print every workspace project's board, merged
```

## Building from sources

From this repository:

```sh
make build
make install
```

Get help with `make help`.

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

[direnv]
enabled = false     # true: agents run through `direnv exec` in projects with an .envrc
```

Issues labelled `manual` (a quote, an email: work that is not code) stay in
Ready when Play hands the column to the build agent.

idesign is in beta and off by default: the board shows no Design button and
no Draft design, and a project's `design/` folder stays as it is on disk. It
has no Settings toggle; turn it on in the same file, then restart:

```toml
[idesign]
enabled = false     # true: the Design panel and Draft design
```

### Workspace

Several projects can be seen as one board. List them by group in the same
user configuration:

```toml
[[workspace.group]]
name = "Client"
repos = ["~/code/app", "~/code/site"]
```

Their columns merge by kind (every Doing in one Doing), each issue is labelled
`repo:<folder>` and `group:<name>` so the filter narrows the board to one
project or one group, and each project keeps its own `issues/`. A folder
without its own `issues/` is skipped with a warning.
