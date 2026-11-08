# StatusBar

A 24pt strip at the bottom of the window in `meta` (Inter 12/16), `ink-muted` on `bg`, with a `line` rule above. It says where you are, once, and stays the same height on every screen.

| Screen | Left | Right |
| --- | --- | --- |
| Board | The issues folder (`~/workspace/perso/iboard/issues`; it left the header). While filtering: "4 of 9 issues match “bug”". After a delete: "Moved … to the Trash · ⌘Z" | Nothing at rest |
| Reader | The file path · "modified 15d ago" · checklist progress (a 48pt `status-done` meter and "1/3 done"), or the word count when there are no checkboxes. The sheet no longer repeats any of these. | Nothing at rest |
| Editor | The file path | The save state pill (Unsaved, Saved, the write error) |

- On the board, a click on the issues folder copies its full path. In the Reader and the Editor, a click on the file path copies the file's full path. Hovering says "Click to copy the full path"; after the click the tooltip says "Copied the full path" until the pointer leaves.
- Separators are `·` in `ink-faint`.
- No total issue count on the board: the column heads already count.
- Keep it boring and always present: no animation at rest, no colour but the save state.
- **Key hints** are hidden at rest. While ⌘ is held, at most three shortcuts that apply to the screen slide in on the right (`↩ open  E edit  ⌘↩ next column` on the board; `E edit  esc board  ⌘⇧R review` in the Reader on Todo, `⌘U update` in its place on Ready and Doing; `⌘S save  esc leave` in the Editor). While the NewIssueModal is open they are its keys: `↩` add, `⌘↩` add & edit, `esc` close.
