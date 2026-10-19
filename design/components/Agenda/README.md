# Agenda

Commits per day across every project iboard supervises, so you can see at a glance when work happened, on what, and open any day to read its commits. It replaces the 26-week grid that was mostly empty grey.

## Anatomy

- **Head**: "Agenda", a project filter token when one is set, and the range as a segmented control: `4w` `12w` `26w` `1y` (default 12w, remembered).
- **Summary** in `meta`: "**105** commits · 3 projects · 23 active days · 9-day streak". The streak only shows from 2 days.
- **Heatmap**: one column per week, Monday on top. Month names above the week where the month starts, `Mon` `Wed` `Fri` on the left, all in `meta`. Days are 18pt squares with a 3pt gap and 4pt radius.
- **Legend** under the grid, with the counts each shade stands for ("1–4 5–8 9–12 13+") and the today mark.
- **By project**, last 12 weeks: one 28pt row per project, sorted by commits. Each row shows the name, 12 weekly bars in `accent` (an empty week is a 1pt `line` tick), the total, and the date of the last commit.
- **Day panel** on the right (min 260pt): the selected day, "15 commits · 2 projects", then the commits grouped by project, busiest first. Each commit shows its time, short sha in `accent` and subject, newest first. A project with more than 5 commits that day ends with "6 more in iboard".

## Colour

- One hue, light to dark, for magnitude. 0 commits is `surface-sunken`. Levels 1–4 are `accent` mixed into `surface` at 30%, 52%, 76% and 100% (the same steps in dark, where the mix goes towards the light accent). Projects have no colours of their own: their names are the identity.
- The scale is rebuilt from what is shown. `step = ceil(busiest day / 4)`, so level 1 is 1…step, level 2 is up to 2×step, and so on. A quiet project filtered on its own still shows its rhythm. The legend always prints the real counts.
- **Today**: a 2pt `ink` ring inside the cell, whatever its shade. **Selected**: the 2pt `focus` ring outside. **Hover**: a 1pt `line-strong` outline. Days to come are not drawn, so the current week's column is as tall as the days so far.

## Range and folding

- Columns are always 18pt; a short range just takes less width and leaves room for the day panel.
- In `26w` and `1y`, a run of 4 or more empty weeks is folded into one dashed `line-strong` column. The summary names each fold: "Folded, no commits: 11 May to 2 Aug". Short gaps stay, because a quiet week is information.
- Below 640pt of window width the day panel moves under the grid.

## Behaviour

| Screen | Command (user) | Event | Next screen |
| --- | --- | --- | --- |
| Stats | Open Stats (the Agenda sits at its foot) | StatsOpened | Grid for the remembered range; today selected |
| Agenda | Hover a day | (none) | Tooltip: day, total, per-project counts |
| Agenda | Click a day, `←↑→↓` then `↩` | AgendaDaySelected | Day panel lists that day's commits |
| Agenda | Click a range | AgendaRangeChanged | Grid rebuilt, scale and legend recomputed, selection kept if still in range |
| Agenda | Click a project row | AgendaProjectFiltered | Grid, summary and day panel for that project only; other rows go `ink-faint` |
| Agenda | Click the row again, the token's ×, `esc` | AgendaFilterCleared | All projects |
| Any | Refresh, or a project's repo changes | CommitsScanned | Same screen; counts update in place |

Read model: `AgendaDays` is a map from (day, project) to a commit count, built from `CommitsScanned` per supervised repo using author dates in local time. The day panel reads the commits for one day on demand.

## Keyboard

Arrow keys move the selected day; `[` and `]` go to the previous or next week, `1`–`4` pick a range, and `esc` clears the project filter. Each day is a button labelled "Wed 14 Oct, 15 commits", and the grid is a `grid` role, so screen readers read days, not colours.

## States

- **Loading**: the grid draws at its final size with real labels, and the days pulse (Skeleton rules: nothing before 300ms, the Thinking i and a clock from 2s). The summary reads "Reading git history in 3 projects…".
- **Empty**: "No commits in the last 4 weeks. The last one was on 16 Sep in site." plus a button that widens the range.
- **A repo that can't be read**: its row says why in `danger` with Retry. The other projects still count.

The consumer provides the supervised repos and today's date. In egui, `theme::agenda_fill(p, commits, step)` returns a day's shade.
