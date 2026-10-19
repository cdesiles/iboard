# GeneralSettings

The **General** tab of the Settings dialog (`⌘,`), first of four: *General*, *AI review*, *Titles*, *Labels*. It holds three choices: the theme, Auto, then whether deleting asks first.

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

## Ask before deleting an issue

- A checkbox under the label "Deleting issues", **checked by default**.
- **On:** every delete (header Delete button, `Del` on a card or in the Reader, the card's trash, a drop on the Delete zone) opens ConfirmDialog first. Help line: "Delete opens a confirmation first: Del then ↩."
- **Off:** the same actions delete straight away; no dialog appears. The file still goes to the macOS Trash (the freedesktop Trash on Linux), never a permanent delete. Help line: "Delete moves the file to the Trash at once. ⌘Z puts it back, or restore it from the Trash."
- **Safety net when off:** the status bar says "Moved done/sidebar-collapse-toggle.md to the Trash" with `⌘Z undo` until the next action. `⌘Z` moves the file back to its folder and the card back to its place.
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
```

An unknown `theme` is reported in Settings and reads as light.

The consumer provides the current value and a `trash(path) -> Result<TrashedItem>` plus `restore(TrashedItem)` for undo. Emits SettingsOpened, DeleteConfirmToggled, SettingsSaved.
