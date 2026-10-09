# GeneralSettings

The **General** tab of the Settings dialog (`⌘,`), first of four: *General*, *AI review*, *Titles*, *Labels*. Today it holds one choice.

## Ask before deleting an issue

- A checkbox under the label "Deleting issues", **checked by default**.
- **On:** every delete (header Delete button, `Del` on a card or in the Reader, the card's trash, a drop on the Delete zone) opens ConfirmDialog first. Help line: "Delete opens a confirmation first: Del then ↩."
- **Off:** the same actions delete straight away; no dialog appears. The file still goes to the macOS Trash (the freedesktop Trash on Linux), never a permanent delete. Help line: "Delete moves the file to the Trash at once. ⌘Z puts it back, or restore it from the Trash."
- **Safety net when off:** the status bar says "Moved done/sidebar-collapse-toggle.md to the Trash" with `⌘Z undo` until the next action. `⌘Z` moves the file back to its folder and the card back to its place.
- Labels that promised a confirmation follow the setting: the Delete drop zone hint reads "asks to confirm" when on and "moves to the Trash" when off, and the Delete button tooltip drops "(asks to confirm)".
- Saved with the dialog's Save (`⌘↩`), like the other tabs.

## Stored as

```toml
[delete]
confirm = true   # false: delete without asking
```

The consumer provides the current value and a `trash(path) -> Result<TrashedItem>` plus `restore(TrashedItem)` for undo. Emits SettingsOpened, DeleteConfirmToggled, SettingsSaved.
