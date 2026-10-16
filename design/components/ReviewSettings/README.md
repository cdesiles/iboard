# ReviewSettings

The *AI review* tab of the Settings dialog: which agent runs the AI review, and with which model. It opens from `⌘,` or from the menu on the Review button, and saves to `~/.config/iboard/iboard.toml`. The other tabs are *General* (GeneralSettings), *AI build*, *Titles* (TitleSettings) and *Labels*.

The *AI build* tab has the same Tool, Command and Model fields, for the agent that implements Ready issues (Ready's play, see Column). It is saved as `[build]`; until it is, the build uses the AI review's agent. A custom command runs from the project folder and may edit it; Claude Code runs with `--permission-mode acceptEdits`.

## Layout

- 440pt wide on `surface` with a `line-strong` edge, `radius-md` and `shadow-popover`. The body keeps one height on every tab, 420pt, and scrolls when a tab needs more.
- Header: "Settings" in `ui-title`, left-aligned, then the tabs (*General*, *AI review*, *AI build*, *Titles*, *Labels*) and a ghost close button. No tinted header bar.
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

## Model

- For OpenCode and Claude Code, a select whose first option is always **Default**, with the model it resolves to shown in `meta` beside it (for example `anthropic/claude-sonnet-4-5`). Default means "the tool's first favorite model", so it follows the tool's own settings.
- Below a separator, a **Favorites** group (★ in `meta`), then **All models** with its count. A filter field at the top of the menu narrows the list as you type; the model IDs are set in mono.
- The list comes from the tool itself (for example `opencode models`), loaded when the dialog opens and cached for the session. While loading, the menu shows "Loading models from opencode…"; if listing fails, it shows the error in `danger` with Retry, and Default stays selectable.
- For Custom command, the model is a plain mono text field (empty = default), because iboard cannot list models for an arbitrary command.
- Changing the tool resets the model to Default.

## Behaviour

- `⌘,` opens it from any screen; `esc` or Cancel closes without saving; `⌘↩` or Save writes the file and closes.
- Select menus open on click, `space` or `↓`; `↑ ↓` move, `↩` picks, typing filters.
- Validation runs on Save only: a custom command without `{prompt}` gets a `danger` help line "The command needs {prompt} to receive the issue." and the dialog stays open.
- After saving, the Review button's menu shows what will run, for example "opencode · default".

## Entry point

The Review button becomes a split button: the main part runs the review (`⌘⇧R`), the caret opens a short menu with "Review this issue", a "Runs with" line naming the tool and model, and "AI settings…" (`⌘,`).

## Stored as

```toml
[review]
tool = "opencode"        # "opencode" | "claude" | "custom"
model = "default"        # or a model ID from the tool's list
command = ""             # custom only, e.g. "llm -m {model} < {prompt}"
```

The key names are a suggestion; keep whatever the app already writes.

The consumer provides the detected tools (name, binary, version or missing), a `list_models(tool) -> Result<Vec<Model{id, favorite}>>`, and the current settings. Emits SettingsOpened, ReviewToolChosen, ReviewModelChosen, SettingsSaved.
