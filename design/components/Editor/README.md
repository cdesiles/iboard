# Editor

The same page in plain text: the raw file, in `edit-body` mono, inside an `accent` 2pt frame that says "you are editing".

- Same width and position as the Reader, so switching modes does not move the text block.
- Above it, the header (see HeaderBar): the breadcrumb with the title on the left; on the right Delete as a dynamic icon (without a key, since `Del` edits text here), which deletes the issue after the ConfirmDialog, the AI split button, `…` (Move to…, Refresh with Auto off, Discard changes) and Save (`⌘S`, primary).
- Below it, the status bar: the file path and the save state pill, which is shown nowhere else.
- Markdown markers (`#`, `-`, `**`) may be tinted `ink-muted`; everything else stays `ink`. No preview pane.
- Leaving with unsaved changes asks inline in the bar: "Save changes to ai-assist.md?" Save · Discard · Keep editing. With Auto on (Settings › General, the default), leaving saves.
- If the file changed on disk while editing, show a `danger` line in the bar with Reload and Keep mine.
- Applying an agent's proposal over unsaved edits asks first, in the same place: Replace mine or Keep mine. Unsaved text is never lost silently.
