# Stats screen

The maintenance view of one project. It answers "is this codebase getting harder to change, and where?" so you can decide when to refactor, instead of finding out from a slow sprint.

## Layout

Top to bottom, on `bg`, with a 16pt gutter:

1. **Header bar**: crumbs "iboard / Stats", when the last scan ran and how many weekly samples it has, and Refresh.
2. **Four stat tiles**: Median cycle time, Complexity, Test coverage and Lines of code. Each shows the value, the change against the previous 12 weeks, and what it measures in `meta`.
3. **Trends** (3 of 4 columns): four small multiples on one shared time axis. Each has its own y-scale, never two scales on one chart.
4. **Refactor watch** (1 of 4 columns): the verdict and the four signals behind it.
5. **Biggest files**: a table of the files, sortable.
6. **Agenda**: commits per day (see Agenda).

## Metrics

| Metric | Definition | Source | Chart |
| --- | --- | --- | --- |
| Lines of code | Code lines, no blanks or comments; language split in the tile | `cloc`/`tokei` on the week's last commit | Area from 0 |
| Complexity | Total cyclomatic complexity per 1,000 code lines, so growth in size alone doesn't raise it | `rust-code-analysis`, `lizard` or similar, same commit | Line, 100-based axis, refactor limit dashed |
| Test coverage | Covered lines / instrumented lines | The newest `lcov.info` (from `cargo llvm-cov`, `c8`…) committed or in CI artifacts that week | Line, 50–100% axis, floor dashed. Weeks without a report are a `surface-sunken` "no report" band, never interpolated |
| Cycle time | Days from an issue entering `doing/` to entering `done/`, median and p85 per week | iboard's own file moves, from git history | Median in `accent`, p85 dashed in `ink-faint`, labelled |

One sample per week: the last commit of the week (Monday start). Earlier weeks are scanned once and cached, so only the current week is recomputed on Refresh.

## Trends

- Each row: name, the value at the hovered week (18pt), "now …" in `meta`, then the chart.
- Hovering any chart moves one crosshair across all four. The panel head says "week of 14 Sep" and every row's value follows, so a jump in complexity can be read against coverage and size for the same week.
- Range: `12w` `26w` `1y` `All` for the trends. The Agenda keeps its own range, since days need a shorter window than trends.
- Limits (refactor above 130, floor 70%) are dashed `status-doing` lines with their label on the left, under the line when the series runs above it.
- Marks follow the chart rules: 2pt lines, 4pt hover dot ringed in `surface`, `line` grid at the axis bounds only, axis text in `meta`.

## Refactor watch

Four signals, each with its numbers and its limit, marked with an icon and a word as well as the colour (`status-doing` warning triangle, or `status-done` check):

| Signal | On when (defaults, Settings › Stats) |
| --- | --- |
| Complexity grows faster than code | complexity per kLOC up more than 5% over 12 weeks, or above 130 |
| Coverage is sliding | down 3 points or more over 12 weeks, or under the 70% floor |
| Hotspots | files with complexity ≥ 60 and ≥ 8 changes in 12 weeks |
| Cycle time drifts | 12-week median more than 50% above the 12 weeks before |

- With 0 or 1 signals on, the verdict reads "No refactor needed now" on `surface-sunken`, with no button.
- With 2 or more on, it reads "**Time to plan a refactor.** 3 of 4 signals are on, mostly from 3 files." and offers **Draft a refactor issue**. That opens the QuickAdd composer in Todo, prefilled with the signals and the hotspot files, labelled `refactor`. Nothing is written until you confirm.

## Biggest files

Columns: File (directory in `ink-faint`, name in `ink`), Lines (number plus a bar scaled to the largest file), Complexity, Coverage (`status-doing` under the floor), Changes in 12 weeks, Hotspot (flame icon plus "hotspot" in `status-doing`, else "–"). Sort with the segmented control or a column head: Lines (default), Complexity, Changes. It shows the top 8, with "Show all 64". Clicking a row opens the file's history.

## Behaviour

| Screen | Command (user) | Event | Next screen |
| --- | --- | --- | --- |
| Any | Open Stats | StatsOpened | Stats; cached samples show at once, the current week rescans |
| Stats | Refresh, or the repo changes | MetricsScanned | Same screen; tiles, trends and watch update in place |
| Stats | Hover a trend | (none) | Shared crosshair; row values show that week |
| Stats | Pick a range | StatsRangeChanged | Trends redrawn for that range |
| Stats | Sort the files | FilesSortChanged | Table reordered |
| Stats | Draft a refactor issue | RefactorIssueDrafted → ComposerOpened | Board, composer prefilled in Todo |
| Stats | A tool is missing (no lcov, no complexity tool) | MetricUnavailable | That tile and row say what is missing and how to add it; the rest still shows |

Read model: `WeeklyMetrics` holds one row per week with loc, by_language, complexity, coverage (optional) and cycle median/p85, plus `FileMetrics` for the current commit. Both are fed by `MetricsScanned`.

## States

- **Loading**: Skeleton rules. Tiles and charts keep their labels and axes, with pulsing placeholders.
- **A metric without data**: its tile says "No coverage report found" and names the file it looks for (`lcov.info`). Its row in Trends shows the same message instead of a chart.

In egui, draw the trends with `egui_plot` (one `Plot` per row, `link_axis` and `link_cursor` on a shared group, `allow_zoom(false)`). Use `HLine` for limits with `p.status_doing` and a dashed `LineStyle`.
