# StatusBar

A 24pt strip at the bottom of the window in `meta` (Inter 12/16), `ink-muted` on `bg`, with a `line` rule above. It says where you are, once, and stays the same height on every screen.

| Screen | Left | Right |
| --- | --- | --- |
| Board | The issues folder (`~/workspace/perso/iboard/issues`; it left the header). While filtering: "4 of 9 issues match “bug”". After a delete: "Moved … to the Trash · ⌘Z" | The project's spend at rest. The subscription rings sit to its left. While ⌘ is held, the key hints slide in to their left |
| Reader | The file path · "modified 15d ago" · checklist progress (a 48pt `status-done` meter and "1/3 done"), or the word count when there are no checkboxes. The sheet no longer repeats any of these. | The subscription rings at rest |
| Editor | The file path | The subscription rings, then the save state pill (Unsaved, Saved, the write error) |

- On the board, a click on the issues folder copies its full path. In the Reader and the Editor, a click on the file path copies the file's full path. Hovering says "Click to copy the full path"; after the click the tooltip says "Copied the full path" until the pointer leaves.
- Separators are `·` in `ink-faint`.
- No total issue count on the board: the column heads already count.
- Keep it boring and always present: no animation at rest, no colour but the save state and the rings' `accent`.
- **Key hints** are hidden at rest. While ⌘ is held, at most three shortcuts that apply to the screen slide in on the right (`↩ open  E edit  ⌘↩ next column` on the board; `E edit  esc board  ⌘⇧R review` in the Reader on Todo; `⌘S save  esc leave` in the Editor). While the NewIssueModal is open they are its keys: `↩` add, `⌘↩` add & edit, `esc` close. The subscription rings stay where they are; the hints slide in to their left.

## Board spend

On the board only, the right side starts with what the project spent. A group board sums its projects.

- **The figure** is the usage log's finished metered spend for today plus what the sessions running now have cost so far: `$0.42`. While an agent works it names the build: `Building · $0.54`. The live cost comes from a light feed polling `/api/session` every 30 s while the board shows, not the runs' 700 ms poller, so the figure lives with no agent running.
- **Only metered dollars count.** A subscription run's API-equivalent is never added, so a board whose runs all came out of a subscription shows `$0.00`.
- **The colour** is `ink` once anything was spent, `ink-muted` at `$0.00`.
- **The tooltip** holds the week and the last finished session: `This week · $3.10` then `Last session · $0.42`.

## Subscription meters

A plan that `[usage]` declares a subscription gets one small ring at the right of the status bar, its name in `meta` beside it. A metered plan gets none: its money is a plain dollar figure, never a share. A provider declared a subscription in `[usage]` gets a ring too, named by its key.

- **The ring** is 14pt, a `line-strong` track with the used share in `accent`, filling clockwise from the top. It only fills from a known cap: a token cap fills by tokens, a money cap by API-equivalent dollars. Without a cap the track stays empty; nothing invents a percentage.
- **The name** is `meta` in `ink-muted` (`Claude.ai`, `Copilot`). The ring, the name and the whole row are one click target.
- **The tooltip** is the row in one line, and VoiceOver reads the same: `Claude.ai · 42% of the week used · resets in 2d · 1,200 of 5,000 tokens`.

Clicking any ring opens the one **subscriptions popover**, above the rings and right-aligned to them:

1. A `BAR_H` head, "Subscription usage" in `ui-title`.
2. One row per subscription plan: its ring, its name in `ui-strong`, then in `meta` `ink-muted`:
   - the share: "42% of the week used", or "unknown" with no cap;
   - the reset: "resets in 2d" (the plan window's next start);
   - the tokens: "210 of 1,000 tokens" for a token cap, "$4.20 of $10.00 API-equivalent · 210 tokens burnt" for a money cap, or "210 tokens burnt" with no cap.
3. A rule, then a `Settings › AI` link, which opens Settings on the AI review tab and closes the popover.

The popover is a `popover_frame` on `surface` with a `line` edge, `radius-md`, `shadow-popover`, and closes on a click outside or `esc`.
