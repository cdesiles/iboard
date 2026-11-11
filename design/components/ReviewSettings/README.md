# ReviewSettings

The *AI review* tab of the Settings dialog: which agent runs the AI review, and with which model. It opens from `⌘,` or from the menu on the Review button, and saves to `~/.config/iboard/iboard.toml`. The other tabs are *General* (GeneralSettings), *AI build*, *Titles* (TitleSettings) and *Labels*.

The *AI build* tab has the same Tool, Command, Worktree and Model fields, a Timeout and the number of build agents, for the agent that implements Ready issues (Ready's play, see Column). It is saved as `[build]`; until it is, the build uses the AI review's agent. A custom command runs from the project folder and may edit it. A build runs in its tool's background service, so it outlives the window: opencode runs the turn, and Claude Code is started with `claude --bg`, the model and `--permission-mode acceptEdits`, then watched with `claude agents --json` and stopped with `claude stop`. A Claude Code without `--bg` builds as a child process. A Claude build that stops on a question shows it on its card with the `claude attach <id>` to run: only that terminal answers it, and the build takes no reply from the board.

## Layout

- A Modal of size M: `modal-m` (480pt) wide on `surface` with a `line` edge, `radius-md` and `shadow-modal`. The body fits its tab and scrolls past 520pt.
- Header: "Settings" in `ui-title`, left-aligned, then the tabs (*General*, *AI review*, *AI build*, *Titles*, *Labels*) and a ghost close button. No tinted header bar. Tabs have no edge; the active one sits on `surface` with a soft shadow.
- Body: one column of fields, label above control (`ui-strong` 12pt), `space-5` between fields, one line of `ui-small` help under each.
- Footer: the config path in `meta` on the left, Cancel and Save (primary, `⌘↩`) on the right.

## Tool

A select with three options, always in this order:

1. **OpenCode** (`opencode`)
2. **Claude Code** (`claude`)
3. **Custom command…**

- Each option shows the detected version in `meta` on the right (`v0.14.2`), or "not found" in `ink-muted` when the binary is not on `PATH`. A missing tool stays selectable, but its help line then says so in `status-doing`: "opencode is not on your PATH. Install it or pick another tool."
- Under the select, the help line confirms what will run: "Found /opt/homebrew/bin/opencode · v0.14.2" in `status-done`.
- Choosing Custom command reveals a **Command** field in mono, with the placeholders iboard fills in: `{prompt}` (a temp file holding the issue and the review instructions), `{model}` and `{issue}` (the issue path). The review is read from stdout.

## Worktree

- OpenCode only, under Tool: a checkbox **Start each scripted run in a new worktree**, on by default.
- It passes `--worktree` to `opencode run`, so a scripted run works in a worktree of its own and the project's checkout stays untouched. A run that continues a session keeps the worktree it started in; a project that is not a git checkout runs in place. A build keeps running in the checkout: it goes through the tool's background service, not a scripted run.
- Claude Code and a custom command do not offer it: neither has the flag. A Claude background build may still move into its own worktree on its own.

## Model

- For OpenCode and Claude Code, a select whose first option is always **Default**, with the model it resolves to shown in `meta` beside it (for example `anthropic/claude-sonnet-4-5`). Default means "the tool's first favorite model", so it follows the tool's own settings.
- Below a separator, a **Favorites** group (★ in `meta`), then **All models** with its count. A filter field at the top of the menu narrows the list as you type; the model IDs are set in mono.
- The list comes from the tool itself (for example `opencode models`), loaded when the dialog opens and cached for the session. While loading, the menu shows "Loading models from opencode…"; if listing fails, it shows the error in `danger` with Retry, and Default stays selectable.
- For Custom command, the model is a plain mono text field (empty = default), because iboard cannot list models for an arbitrary command.
- Changing the tool resets the model to Default.

## Review scope

- AI review only, under Model: a segmented control, **Ticket only** then **Ticket + project context**, with one line of help under it.
- Ticket only, the default for new and existing files, is the quick path: the agent reads the issue alone, without opening files or running tools.
- Ticket + project context lets the agent also read the repository and its design system to check what the issue claims.
- "Review this issue" (`⌘⇧R`) and the card's Review run in this scope; the Review button's menu offers the other one as a one-off (*Deep review* or *Quick review*).

## Usage plans

- AI review only, under Review scope: one block per known provider, **Anthropic API**, **DeepSeek**, **Claude.ai** and **Copilot**, then the rule for one iboard hasn't seen.
- Each block starts with **Metered** or **Subscription**. Metered means the log's dollars are money spent; a subscription means they are API-equivalent and never counted as spend.
- A subscription also has a **Window** (**Day**, **Week** or **Month**), a **Price** per window, and a **Cap** of **No cap**, **Money** or **Tokens**. A cap shows its amount; without one the share stays unknown.
- The defaults are the four known providers: both APIs metered, Claude.ai weekly, Copilot monthly. Price and cap stay empty until you fill them, because iboard has no provider API that reports them.
- A provider iboard has not seen is metered with no cap. Editing a plan rereads the usage log; it never rewrites it.

## Timeout

- AI build only, under Model: a select with **5**, **10**, **20**, **30**, **60** and **180 minutes**, then **No limit** with `∞` in `meta`. The default is 30 minutes.
- It stops a build and an Ask, read-only or not. No limit leaves stopping to the user.
- The other runs have a fixed limit: a review, the Challenger's rounds and a split stop at 120s (see Thinking), a title at 15s (see QuickAdd), and a new-issue draft, a design mockup and a component's preview at 10 minutes.

## Build agents

- AI build only, under Timeout: a select from **None** and **1 agent** to **16 agents**, 1 by default.
- It is the same count as Doing's header (see Column): the header and Settings show and set each other, and the count is saved in `[build] agents`, so the next launch opens with it.
- Each agent takes the next Ready issue of the run. None stops the build; Play starts it again with one agent.

## Behaviour

- `⌘,` opens it from any screen; `esc` or Cancel closes without saving; `⌘↩` or Save writes the file and closes.
- Select menus open on click, `space` or `↓`; `↑ ↓` move, `↩` picks, typing filters.
- Validation runs on Save only: a custom command without `{prompt}` gets a `danger` help line "The command needs {prompt} to receive the issue." and the dialog stays open.
- After saving, the Review button's menu shows what will run, for example "opencode · default".

## Entry point

The AI split button (see HeaderBar and ReviewButton) ends its menu with the current tool and model, which opens these settings (`⌘,`).

## Stored as

```toml
[review]
tool = "opencode"        # "opencode" | "claude" | "custom"
model = "default"        # or a model ID from the tool's list
command = ""             # custom only, e.g. "llm -m {model} < {prompt}"
scope = "ticket"         # "ticket" | "project"; absent means "ticket"
worktree = true          # OpenCode only; absent means true

[build]
tool = "opencode"        # the same keys, without scope
model = "default"
timeout = "30m"          # "5m" | "10m" | "20m" | "30m" | "60m" | "180m" | "none"; absent means "30m"
agents = 4               # 0 to 16; absent means 1
worktree = true          # OpenCode only; absent means true

[usage.claude]           # one block per provider; the four known ones have defaults
kind = "subscription"    # "metered" | "subscription"
window = "week"          # "day" | "week" | "month"
price = 20.0             # what the plan costs over its window, when known
token_cap = 1000000      # or money_cap = 20.0; never both
```

The key names are a suggestion; keep whatever the app already writes.

The consumer provides the detected tools (name, binary, version or missing), a `list_models(tool) -> Result<Vec<Model{id, favorite}>>`, and the current settings. Emits SettingsOpened, ReviewToolChosen, ReviewModelChosen, SettingsSaved.
