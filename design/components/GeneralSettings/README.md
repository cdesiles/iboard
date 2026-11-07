# GeneralSettings

The **General** tab of the Settings dialog (`⌘,`), first of five: *General*, *AI review*, *AI build*, *Titles*, *Labels*. It holds the theme, Auto, what reaches the notification bell, then whether deleting asks first.

## Theme

- A segmented control under the label "Theme": **Light** (the default), **Dark**, **System**.
- **Light** and **Dark** draw every window in that theme, whatever macOS uses. **System** follows the system's appearance, and switches with it.
- The help line under it says what the choice does: "Light, whatever the system's appearance.", "Dark, whatever the system's appearance.", "Follows the system's appearance, light or dark."
- Applied on Save, at once: the board and Settings itself redraw in the new theme. `idesign` reads the same setting when it opens.
- Both themes draw from the `light` and `dark` values of `design/tokens.json`; nothing is tuned per theme in code.

## Auto

- A checkbox under the label "Auto": "Reload and save automatically", **checked by default**. It lives only here; the board's header has no Auto toggle.
- **On:** the board reloads when files change on disk, and the Editor saves as you type and on leave. Help line: "The board reloads when files change on disk, and edits save as you type."
- **Off:** the header shows Refresh, and the Editor asks before leaving unsaved edits. Help line: "Refresh (⌘R) reloads the board, and the editor asks before leaving unsaved edits."

## Notifications

- Five checkboxes under the label "Notifications", all **checked by default**: "A build finishes", "A build fails", "A review is ready", "An ask finishes", and "Open the agent log on a Doing issue". The help line under them says questions always reach the bell, whichever boxes are on.
- **Questions are not optional.** A build waiting on its `question` tool or a tool approval always reaches the bell and waits in its ticket: a setting that silenced it would leave the build stuck with nothing to see.
- **Auto log.** On (the default), opening a Doing issue that has an agent log opens it beside the reader in Conversation mode. Off, the log is opened from the card, the `…` menu or a notification only. A waiting question opens it whatever the setting says.
- Saved with the dialog's Save (`⌘↩`).

## Ask before deleting an issue

- A checkbox under the label "Deleting issues", **checked by default**.
- **On:** every delete (header Delete button, `Del` on a card or in the Reader, the card's trash, a drop on the Delete zone) opens ConfirmDialog first. Help line: "Delete opens a confirmation first: Del then ↩."
- **Off:** the same actions delete straight away; no dialog appears. The file still goes to the macOS Trash (the freedesktop Trash on Linux), never a permanent delete. Help line: "Delete moves the file to the Trash at once. ⌘Z puts it back, or restore it from the Trash."
- **Safety net when off:** the status bar says "Moved done/sidebar-collapse-toggle.md to the Trash" with `⌘Z undo` until the next key or pointer press. `⌘Z` moves the file back to its folder and the card back to its place.
- Labels that promised a confirmation follow the setting: the Delete drop zone hint reads "asks to confirm" when on and "moves to the Trash" when off, and the Delete button tooltip drops "(asks to confirm)".
- Saved with the dialog's Save (`⌘↩`), like the other tabs.

## Stored as

```toml
[appearance]
theme = "light"  # "dark", or "system" to follow macOS

[auto]
enabled = true   # false: no reload from disk, no saving as you type

[delete]
confirm = true   # false: delete without asking

[notifications]
build_done = true    # a build finishes
build_failed = true  # a build fails
review_ready = true  # a review is ready
ask_done = true      # an ask finishes
auto_log = true      # open the agent log on a Doing issue
```

An unknown `theme` is reported in Settings and reads as light.

The consumer provides the current value and a `trash(path) -> Result<TrashedItem>` plus `restore(TrashedItem)` for undo. Emits SettingsOpened, DeleteConfirmToggled, SettingsSaved.
