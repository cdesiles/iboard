# DropZones

Two large drop targets, Archive and Delete, one at a time, that slide up from the bottom of the board only while a card is being dragged.

- **When:** the dock appears as soon as a drag starts (after the pointer moves 4pt with the button down) and slides away when the card is dropped or the drag is cancelled with `esc`. It never shows at rest, so the board stays quiet.
- **Where:** a full-width band between the board and the status bar, `bg` ground with a `line` rule above, 12×16 padding. It sits on top of the bottom of the board; the board scrolls under it, so the last cards stay reachable.
- **Zones:** 56pt tall, `radius-md`. A card dragged from Done gets Archive alone; any other card gets Delete alone. Either spans the full width. Each zone has an 18pt line icon, a word (`ui-strong`) and a hint in `meta`:
  - **Archive** (Done only): dashed `line-strong` edge on `surface-sunken`, `ink-muted`. Hint "moves to .archive/". Under the pointer: 2pt `accent` edge on `accent-soft`.
  - **Delete** (never Done): dashed `danger` edge and `danger` label, so it reads as destructive before you reach it. Hint "asks to confirm", or "moves to the Trash" when asking is turned off in Settings › General. Under the pointer: 2pt `danger` edge on `danger-soft`.
- **Done is archived, never deleted.** Dragging a card from another column shows Delete alone across the dock; a card from Done shows Archive alone. Neither ever appears disabled.
- **Dropping:** on Archive, the file moves to `.archive/` at once and the status bar says "Archived sidebar-collapse-toggle.md". On Delete, ConfirmDialog opens and the card goes back to its place until you confirm; with asking turned off, the file goes to the Trash at once and the status bar offers `⌘Z` undo.
- Columns stay valid drop targets as before; the dock only adds the two exits.

The consumer provides the dragged issue's column and file path, and handlers for archive and delete. Emits IssueDragStarted, DropZonesShown, IssueArchived, DeleteConfirmRequested, IssueDragCancelled.
