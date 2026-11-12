# StatusBar

A 24pt strip at the bottom of the window in `meta` (Inter 12/16), `ink-muted` on `bg`, with a `line` rule above. It says where you are, once, and stays the same height on every screen.

| Screen | Left | Right |
| --- | --- | --- |
| Board | The issues folder (`~/workspace/perso/iboard/issues`; it left the header). While filtering: "4 of 9 issues match “bug”". After a delete: "Moved … to the Trash · ⌘Z" | The project's spend and the closest subscription window, as one trigger. While ⌘ is held, the key hints slide in to their left |
| Reader | The file path · "modified 15d ago" · checklist progress (a 48pt `status-done` meter and "1/3 done"), or the word count when there are no checkboxes. The sheet no longer repeats any of these. | The usage trigger at rest |
| Editor | The file path | The usage trigger, then the save state pill (Unsaved, Saved, the write error) |

A newer release adds one item before the usage trigger, on every screen; see [Update offer](#update-offer).

- On the board, a click on the issues folder copies its full path. In the Reader and the Editor, a click on the file path copies the file's full path. Hovering says "Click to copy the full path"; after the click the tooltip says "Copied the full path" until the pointer leaves.
- Separators are `·` in `ink-faint`.
- No total issue count on the board: the column heads already count.
- Keep it boring and always present: no animation at rest, no colour but the save state and the usage trigger.
- **Key hints** are hidden at rest. While ⌘ is held, at most three shortcuts that apply to the screen slide in on the right (`↩ open  E edit  ⌘↩ next column` on the board; `E edit  esc board  ⌘⇧R review` in the Reader on Todo; `⌘S save  esc leave` in the Editor). While the NewIssueModal is open they are its keys: `↩` add, `⌘↩` add & edit, `esc` close. The usage trigger stays where it is; the hints slide in to their left.

## Board spend

On the board only, the right side starts with what the project spent. A group board sums its projects. The spend is not an item of its own: it is the left half of the one usage trigger (`components/UsageMeter`), which also carries the share.

- **The figure** is the usage log's finished metered spend for today plus what the sessions running now have cost so far: `$28.61`, with no word in front of it. A build in progress is named in the popover (`Building`), never in the bar. The live cost comes from a light feed polling `/api/session` every 30 s while the board shows, not the runs' 700 ms poller, so the figure lives with no agent running.
- **Only metered dollars count.** A subscription run's API-equivalent is never added, so a board whose runs all came out of a subscription shows `$0.00`.
- **The colour** is `ink` once anything was spent, `ink-muted` at `$0.00`.
- **The week and the last session** are the popover's first row: `This week · $3.10` then `Last session · $0.42`. Nothing on the bar names them on hover.

## Update offer

When a newer GitHub release exists, one more item sits between the key hints and the usage trigger, on every screen that shows the bar: a `Ghost` text button in `meta`.

- **At rest** it reads `Update to 0.5.0` in `ink-muted`, turning `ink` under the pointer; no edge, no colour, no animation.
- **While the download runs** it reads `Downloading update…`; once the bundle is staged, `Restart to update`. A click always opens the UpdateDialog, which owns the states and the restart.
- **Only an installed app** shows it: a copy run from a checkout has no bundle to replace.
- It does not take the usage trigger's place: the trigger stays the rightmost item on the board and Reader, and the save state after it in the Editor.

## Subscription meters

A plan that `[usage]` declares a subscription feeds one small pill at the right of the status bar, after the spend. A metered plan has none: its money is a plain dollar figure, never a share. The pill takes about 50pt instead of a ring and a name per provider.

- **The pill** is a 14pt ring and the share of the window closest to its limit (`62%`), turning `status-doing` when that window is over pace. With no cap on any window it is a dashed ring and `Usage`. It only fills from a known share; nothing invents a percentage.
- **A click** opens the one usage popover, above the trigger and right-aligned to it: today's spend, then a row per window with its share, an even-pace tick and a verdict. Claude.ai's session and week are two rows.
- **Nothing on hover.** The pill has no tooltip: hovering shows nothing, and the popover names each window with its share and reset.

The trigger, the popover, pace, units and the no-cap state are specified in `components/UsageMeter`.
