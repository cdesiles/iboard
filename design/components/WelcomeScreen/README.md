# WelcomeScreen

What a bare `iboard` shows, from the Dock most of all, when there is nothing to open: no recent project still has its issues, the current folder is in no project and the configuration lists no group. It replaces a stderr message nobody sees.

## Layout

The whole window on `bg`, with no HeaderBar and no StatusBar, as there is no project to name. A `bar-h` strip across the top stays free for the traffic lights and drags the window. Below it, centred in the window, top to bottom:

1. **Wordmark**: 48pt tall, from `components/Logo/files`, the light or the dark file with the theme. See Logo for its clear space.
2. **One sentence** in `ui-body`, `ink-muted`, `space-6` under the wordmark: "Open a project with an issues/ folder, or create one in any folder."
3. **Open a folder…**: the primary Button, the one main action, `space-6` under the sentence.
4. **Create issues in a folder…**: a secondary Button, `space-3` under it.

## Behaviour

- Both buttons open the OS folder picker. Cancelling it stays here.
- **Open a folder…** opens the folder's project, as Add project… does in the ProjectMenu. A folder with no `issues/` raises the usual error strip under the top strip, with its Create issues/ button.
- **Create issues in a folder…** creates `issues/` with todo, ready, doing and done, as `iboard init` does, and opens the board.
- No recent projects: this screen only shows once none of them can be opened.
- Settings stay reachable from the macOS menu. Board keys do nothing here.
- Anything asked for a real path keeps failing in the terminal: `iboard --list`, `iboard init` and `iboard <path>` outside a project report on stderr and exit with 1.
