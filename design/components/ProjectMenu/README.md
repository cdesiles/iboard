# ProjectMenu

The menu under the header's project name and path, top left of the Board. It switches what the board shows: one group (each group is its own board), one recent project, or a folder opened from disk. It gains one row, **Edit groups…**, so groups can be found and changed without reading the docs. The bar and the board do not change.

## Trigger

- The project name (`ui-title`) and its path or group name (`meta` mono, `ink-muted`) with a chevron. Transparent at rest; `surface-hover` fill and a `line-strong` edge on hover and while the menu is open, so it reads as a control.
- Opens on click, `↩` or `↓` when focused. `↑ ↓` move, `↩` picks, `esc` or a click outside closes.

## The menu

340pt wide, `surface`, `line-strong` edge, `radius-md`, `shadow-popover`, 4pt padding. Three sections split by a `line` separator:

1. **Groups.** One row per `[[workspace.group]]`, in file order, each opening that group's own board: the group name in `ui-body` 500, its project count under it in `meta` mono ("3 projects"). The board being shown is `accent`. Then the quiet row:
   - **Edit groups…** One 24pt line, `ui-small` in `ink-muted`, the number of groups in `meta` on the right ("2 groups"). Hover gives the `surface-hover` fill and turns the label `ink`. The ellipsis is there because a dialog follows. Opens GroupsDialog.
   - **No group yet:** the section is only the quiet row, reading **New group…** with "several projects, one board" on the right. It opens GroupsDialog straight into a new group with its name field focused. Same place, same look, so the entry point never moves.
2. **Recent projects.** Two-line rows: folder name in `ui-body` 500, path in `meta` mono. Current one in `accent`.
3. **Open folder…** with "Choose a project containing an issues/ folder" under it.

Every row is at least 24pt tall and the full menu width is the hit area. No icons: the rows are words, like every other menu.

## Events

| Command | Event | Next |
| --- | --- | --- |
| Click the trigger, `↩`, `↓` | ProjectMenuOpened | Menu under the trigger |
| Pick a group or project | WorkspaceSwitched | Board for that group or project |
| Edit groups… | GroupsEditorOpened | GroupsDialog over the board |
| New group… (no group yet) | GroupsEditorOpened → GroupDrafted | GroupsDialog, new group's name focused |
| Open folder… | FolderPickerOpened → ProjectOpened | Board for that folder |
| `esc`, click outside | ProjectMenuClosed | Board |

The consumer provides the groups (name, repo paths), the recent projects and the current workspace.
