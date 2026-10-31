# GroupsDialog

Creates and edits project groups. Each group is its own board: its projects seen as one, columns merged by kind. Today a group exists only as a table in the user config:

```toml
[[workspace.group]]
name = "Client"
repos = ["~/code/app", "~/code/site"]
```

This dialog is a friendlier way to edit those tables. **The file stays the source of truth**: the dialog reads it on open and writes the same tables back on Save. No restart; the board reloads in place.

Opened from the ProjectMenu's footer: **Edit groups…**. With no group yet it opens on the "No groups yet" state.

## Layout

- A Modal M over `scrim`: `surface`, `modal-m` (480pt, since paths are long), `line-strong` edge, `radius-md`, `shadow-modal`. Same head and foot as Settings: title "Project groups" in `ui-title` and a ghost close button; foot with the config path in `meta` on the left, an Unsaved pill once something changed, then **Cancel** `esc` and **Save** `⌘↩` (primary).
- Body scrolls past 520pt. It opens with one help line: "Each group is a board of its own projects. Removing a project here never touches its issues/ folder."
- **One bordered block per group**, in file order, `line-strong` edge, `radius-md`:
  - **Head:** the name as a text field (`ui-body` 500, always editable, so rename is just typing), the project count in `meta` ("2 projects", "1 project"), and a delete-group icon button (iboard's trash, `ib-act--danger`, tooltip "Delete group (asks first)").
  - **Project rows:** folder name in `ui-body` 500, path in `meta` mono, a `line` rule between rows. A remove button (×, `ib-act`) shows on hover, selection or focus, tooltip "Remove from this group. The folder is not touched. (Del)".
  - **Foot:** **Add project** (secondary, plus icon).
- Under the last block, **New group** (secondary, plus, `⌘N`).

## Adding projects

**Add project** opens a small menu, styled like the ProjectMenu, under the button:

1. **Your projects** (group head in `ui-label`), in your ProjectMenu order, minus those already in this group; a filter field on top once there are more than 8. One click adds one; the menu stays open so you can add several.
2. **Choose folders…** with "Choose a project containing an issues/ folder" under it: the same native picker as the ProjectMenu's Add project…, with multiple selection on (`⌘`-click). Every chosen folder is added.

- A folder without an `issues/` folder is not added. The group's help line says so in `status-doing`: "~/code/legacy has no issues/ folder. Pick the project's root."
- A folder already in the group is skipped silently; the row flashes `accent` once.
- One project can be in several groups.
- Paths are stored the way the user would write them: under the home folder they start with `~/`.

## States

- **Empty group:** under the head, the `ib-empty` well: "No projects in this group yet — Add a project", where *Add a project* is a link that opens the same menu.
- **No group at all:** the body is one dashed well: **No groups yet**, then "Each group is a board of its own projects: columns merge by kind, and each issue carries its repo: label." and a primary **New group** `⌘N`.
- **New group:** a block appended at the end with an `accent` edge, its name field focused (placeholder "Group name"). `↩` in the name field opens the Add project menu. A new group with no name and no project is dropped on Save, never written.
- **Missing folder:** a configured repo whose folder or `issues/` is gone stays listed, its path in `status-doing` with "· no issues/ folder". It is kept on Save (the folder may be on an unmounted disk) until removed by hand.

## Deleting a group

- The trash button, or `⌘⌫` while focus is inside that group, opens ConfirmDialog on top of this dialog: "Delete this group?", the subject box with the name and "2 projects · ~/code/app, ~/code/site", the consequence "Only the group is removed. The projects and their issues/ folders stay where they are. Nothing is written until you save.", then **Cancel** `esc` and **Delete group** `↩` (filled `danger`, trash icon, focused).
- It always asks, whatever the "Ask before deleting an issue" setting: that switch is about files going to the Trash; here a whole board disappears in one click and there is no `⌘Z` across Save.
- Confirming removes the block from the draft. Cancel on the groups dialog still brings it back.

## Removing a project

No confirmation: it only edits the draft, never the disk, and Cancel undoes it. `Del` on a focused project row removes it and moves focus to the next row.

## Saving

- **Save** (`⌘↩`) validates, writes, closes and reloads the board. **Cancel**, `esc` or the close button drop the draft; with unsaved changes `esc` asks first ("Discard changes to groups?", Discard / Keep editing), like the Editor.
- **Checks, on Save only:** each name is trimmed and must be non-empty ("Name the group, or delete it.") and unique, ignoring case ("Another group is already called Client. Rename one of them."). The message sits under that group's head in `danger`, the field is focused, the dialog stays open.
- **Write-back:** only the `[[workspace.group]]` tables are replaced, in dialog order; every other key, table and comment in the file is kept as it is (edit the document, e.g. with `toml_edit`, never re-serialize the whole file). A group with no projects is written with `repos = []`.
- **Conflict:** if the file changed on disk since the dialog opened, nothing is written and the body ends with "Couldn't save ~/.config/iboard/iboard.toml: the file changed on disk. Reload or keep your version." (Reload re-reads the groups and drops the draft; Keep your version overwrites the group tables only.) Other write failures name the file and the reason the same way.
- **After saving:** the status bar says "Saved 2 groups to ~/.config/iboard/iboard.toml". The board reloads in place:
  - Showing a group that was renamed: it stays on that group under its new name.
  - Showing a group that was deleted: it switches to the first remaining group, or to the most recent project if none is left.
  - A filter token naming a removed project or a renamed group is dropped; the rest of the filter stays.
- The config file is also watched: an outside edit reloads the groups (and the ProjectMenu) without a restart, exactly as a Save from here does.

## What does not change

The boards themselves: each group is its own board, its columns merge by kind, each issue carries `repo:` and `group:` labels, and the filter narrows to one project. The header bar keeps its controls. No project's `issues/` folder is ever created, moved or deleted from this dialog.

## Keys

| Key | Action |
| --- | --- |
| `⌘N` | New group (inside the dialog only) |
| `↩` in a group name | Open its Add project menu |
| `Del` on a project row | Remove it from the group |
| `⌘⌫` inside a group | Delete group (asks first) |
| `⌘↩` | Save |
| `esc` | Close a menu, else Cancel (asks if unsaved) |

## Interaction model

Spec-first, event-modeling style. The dialog works on a **GroupsDraft** read model built from the config; nothing touches the disk until `GroupsSaved`.

| Screen | Command (user) | Event | Next screen / read model |
| --- | --- | --- | --- |
| Board | Edit groups… in the ProjectMenu footer | GroupsEditorOpened | GroupsDialog; GroupsDraft = groups from the config, config hash noted |
| GroupsDialog | New group (`⌘N`, button) | GroupDrafted | New block, name focused |
| GroupsDialog | Type in a group name | GroupRenamed | Draft updated, Unsaved pill |
| GroupsDialog | Add project › pick one of your projects | ProjectAddedToGroup | Row appended; menu stays open |
| GroupsDialog | Add project › Choose folders… | FolderPickerOpened → ProjectAddedToGroup (each) / ProjectRejected (no issues/) | Rows appended, or the help line names the folder |
| GroupsDialog | Remove (×, `Del`) | ProjectRemovedFromGroup | Row gone; empty well if it was the last |
| GroupsDialog | Delete group (trash, `⌘⌫`) | GroupDeleteRequested → GroupDeleteConfirmRequested | ConfirmDialog over the dialog |
| ConfirmDialog | Delete group (`↩`) / Cancel (`esc`) | GroupDeleted / GroupDeleteCancelled | GroupsDialog, block removed or kept |
| GroupsDialog | Save (`⌘↩`) | GroupsValidated / GroupsInvalid | Write, or messages under the faulty groups |
| GroupsDialog | (valid) | GroupsSaved → UserConfigWritten → WorkspaceReloaded | Board in place; status bar "Saved N groups to …" |
| GroupsDialog | (file changed on disk) | GroupsSaveFailed | Dialog stays, error with Reload / Keep your version |
| GroupsDialog | Cancel, `esc`, close | GroupsEditCancelled (asks first if the draft changed) | Board, nothing written |
| Any | Config file changed outside iboard | UserConfigChanged → WorkspaceReloaded | Same screen; groups and ProjectMenu refreshed |

The consumer provides the groups from the config, the ordered projects, a `pick_folders() -> Vec<PathBuf>` (the Add project… picker with multiple selection), a `has_issues_dir(path) -> bool`, and `write_groups(groups, expected_hash) -> Result<(), ConfigError>` that replaces only the `[[workspace.group]]` tables.
