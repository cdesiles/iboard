# Editor

The same page in plain text: the raw file, in `edit-body` mono, inside an `accent` 2pt frame that says "you are editing".

- Same width and position as the Reader, so switching modes does not move the text block.
- Above it: the file path, the save state pill, Discard and Save (`⌘S`, primary). The header also carries the labelled Delete button (danger outline, without the `Del` hint since that key edits text here), which deletes the issue after the ConfirmDialog.
- Markdown markers (`#`, `-`, `**`) may be tinted `ink-muted`; everything else stays `ink`. No preview pane.
- Leaving with unsaved changes asks inline in the bar: "Save changes to ai-assist.md?" Save · Discard · Keep editing. With Auto on, leaving saves.
- If the file changed on disk while editing, show a `danger` line in the bar with Reload and Keep mine.
