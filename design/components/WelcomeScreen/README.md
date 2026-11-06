# WelcomeScreen

What a bare `iboard` shows, from the Dock most of all, when there is nothing to open: no recent project still has its issues, the current folder is in no project and the configuration lists no group. It replaces a stderr message nobody sees.

## Layout

The whole window on `bg`, with no HeaderBar and no StatusBar. A `bar-h` strip across the top stays free for the traffic lights (on its centre line, as on every screen) and drags the window. Below it, centred, top to bottom:

1. **Wordmark**: 48pt tall, from `components/Logo/files` (`iboard-wordmark-light.svg` or `iboard-wordmark-dark.svg`, with the theme).
2. **One sentence** in `ui-body`, `ink-muted`, `space-6` under the wordmark: "Open a project with an issues/ folder, or create one in any folder."
3. **Open a folder…**: the primary Button (`primary`, the screen's one fill), `space-6` under the sentence.
4. **Create issues in a folder…**: a secondary Button with the `line-soft` hairline of a content block, `space-3` under it.

Nothing else: no recent list (there is none that opens), no tips, no illustration.

## Behaviour

- Both buttons open the OS folder picker ("Choose a project folder", button "Choose"). Cancelling it stays here.
- **Open a folder…** opens the folder's project, as Add project… does in the ProjectMenu. A folder with no `issues/` raises the usual error strip under the top strip, with its **Create issues/** button.
- **Create issues in a folder…** creates `issues/` with `todo`, `ready`, `doing` and `done`, as `iboard init` does, and opens the board.
- No recent projects: this screen only shows once none of them can be opened.
- Settings stay reachable from the macOS menu. Board keys do nothing here, so holding ⌘ reveals no hints.
- `iboard --list`, `iboard init` and `iboard <path>` outside a project report on stderr and exit with 1.

| Screen | Command (user) | Event | Next screen |
| --- | --- | --- | --- |
| (launch) | Bare `iboard`, nothing to open | WelcomeShown | WelcomeScreen |
| WelcomeScreen | Open a folder…, pick a project | ProjectAdded → BoardOpened | Board |
| WelcomeScreen | Open a folder…, pick a folder without `issues/` | ProjectOpenFailed | WelcomeScreen with the error strip |
| WelcomeScreen | Create issues in a folder…, or Create issues/ in the strip | IssuesFolderCreated → BoardOpened | Board, four empty columns |
| WelcomeScreen | Cancel the picker | (none) | WelcomeScreen |

The consumer provides the native folder picker, `open_project(path)` and `init_issues(path)`.
