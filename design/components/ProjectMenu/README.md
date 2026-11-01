# ProjectMenu

The menu under the header's project name and path, top left of the Board: pick a group (each group is its own board: its projects seen as one) or a single project, and reach the three actions that manage them. Lists on top, actions in a footer of their own, so choosing and managing never mix. The bar and the board do not change.

## Trigger

- The project name (`ui-title`) and its path or group name (`meta` mono, `ink-muted`) with a chevron. Transparent at rest; `surface-hover` fill and a `line-strong` edge on hover and while the menu is open.
- Opens on click, `↩` or `↓` when focused. `esc` or a click outside closes.

## Layout

360pt wide, `surface`, `line-strong` edge, `radius-md`, `shadow-popover`. Top to bottom, split by `line` rules:

1. **Groups.** Head "Groups" in `ui-label` with the count in `meta`. One two-line row per `[[workspace.group]]`, in file order, each opening that group's own board: the group name in `ui-body` 500, "6 projects" in `meta` mono. The board being shown is `accent`. Scrolls after 4 rows (168pt). **No group:** the section is not drawn at all.
2. **Projects.** Head "Projects" with the count. A filter field ("Filter projects", full width), then one two-line row per project, **in the user's order**: folder name in `ui-body` 500, path in `meta` mono, current one in `accent`.
   - **The list scrolls inside the menu**, never past the window: at most 336pt (8 rows), or less when the window is short, so the footer is always on screen. Overlay Scrollbar, no track. On open, the current project is scrolled into view.
   - Under 9 projects the filter field is hidden at rest and nothing scrolls; it appears on the first key you type.
3. **Footer**, on `bg` (the window ground, so it reads as a separate strip, not more list), 8pt padding, three secondary buttons that never move:
   - **Add project…** (plus): the native picker, "Choose a project containing an issues/ folder". Replaces "Open folder…": you add a project to your list, then it opens.
   - **Edit groups…** (layers): opens GroupsDialog. With no group yet, the dialog opens on its "No groups yet" state.
   - **Reorder** (grip): switches the menu into reorder mode.

Every row is at least 24pt tall and its full width is the hit area.

## Finding a project among 50

- **Filtering starts on its own when you type.** The menu opens with the focus on the list, not in the field; the first printable key moves focus into the filter with that character already in it, so opening the menu and typing "moon" is the whole gesture. No click, no `⌘F`. `⌫` back to empty, or `esc`, returns to the full list. It matches the folder name and the path, case-insensitive; the matched part of the name is marked in `accent-soft`.
- While a filter is set, the Groups section hides, the count reads "2/48", the first match is active (`accent-soft`), and a note says "↩ opens moon-cull · esc clears the filter". `esc` clears first, closes second.
- No match: "No project matches moon. Add project… to add one." in `ink-faint`.
- `↑ ↓` move through groups and projects as one list; `↩` opens.

## Reorder mode

Rows in a menu open on click, so dragging them directly would fight that. Reorder is an explicit mode instead:

- The projects head reads "Reorder projects", with the note "Drag a project, or select it and press ⌥↑ ⌥↓. The board does not change." Groups and the filter hide; every row shows a 6-dot grip in `ink-faint` (`ink-muted` on hover) and a `grab` cursor. Clicking a row selects it; it never opens.
- **Drag:** the row lifts (`surface`, `line-strong` edge, `shadow-popover`, a slight tilt, like a dragged card), its slot stays as a faded placeholder, and a 2pt `accent` line shows where it will land. Near the top or bottom edge the list auto-scrolls.
- **Keyboard:** `↑ ↓` select, `⌥↑ ⌥↓` move the selected project one place, `⌥⇧↑ ⌥⇧↓` to the top or bottom.
- The footer becomes "Saved as your project order" in `meta`, **Cancel** `esc` and **Done** `↩` (primary). Done writes the order and returns to the normal menu; Cancel puts it back.

## Stored as

The order is a user preference, kept in the user config next to the groups:

```toml
[workspace]
projects = ["~/workspace/perso/mucus-labs", "~/workspace/perso/ansible-playbooks", "~/sunboat"]
```

A project opened or added for the first time goes to the top. Removing one from the list is not offered here (the folder is never touched anyway); it drops out when its folder is gone. The key name is a suggestion; keep whatever the app already writes.

## Events

| Command | Event | Next |
| --- | --- | --- |
| Click the trigger, `↩`, `↓` | ProjectMenuOpened | Menu under the trigger, current project in view |
| Type any character (focus anywhere in the menu) | ProjectFilterStarted → ProjectFilterChanged | Field focused with that character, matches only, first one active, groups hidden |
| Pick a group or project | WorkspaceSwitched | Board for that group or project |
| Add project… | FolderPickerOpened → ProjectAdded → WorkspaceSwitched | Project at the top of the list, its board open |
| Edit groups… | GroupsEditorOpened | GroupsDialog over the board |
| Reorder | ProjectReorderStarted | Reorder mode |
| Drag, `⌥↑ ⌥↓` | ProjectMoved | Same mode, new position |
| Done (`↩`) / Cancel (`esc`) | ProjectOrderSaved → UserConfigWritten / ProjectReorderCancelled | Normal menu |
| `esc`, click outside | ProjectMenuClosed | Board |

The consumer provides the groups (name, repo paths), the ordered projects, the current workspace, the native folder picker and a `write_project_order(paths)` that only replaces `workspace.projects`.
