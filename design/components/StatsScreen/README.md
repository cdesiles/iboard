# Stats screen

The maintenance view of one project. It answers "is this codebase getting harder to change, and where?" so you can decide when to refactor, instead of finding out from a slow sprint.

## Layout

Top to bottom, on `bg`, with a 16pt gutter:

1. **Header bar**: crumbs "iboard / Stats", when the last scan ran and how many weekly samples it has, and Refresh.
2. **Four stat tiles**: Median cycle time, Complexity, Test coverage and Lines of code. Each shows the value, the change against the previous 12 weeks, and what it measures in `meta`.
3. **Trends** (3 of 4 columns): four small multiples on one shared time axis. Each has its own y-scale, never two scales on one chart.
4. **Refactor watch** (1 of 4 columns): the verdict and the four signals behind it.
5. **Cost**: what the agents cost over the trends' days, a bar per day and a row per provider.
6. **Biggest files**: a table of the files, sortable.
7. **Agenda**: commits per day (see Agenda).

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

## Releases

Release tags drawn on the trends, so a jump in complexity or a dip in coverage can be read against what shipped.

- **Which tags**: tags matching `release_tags`, `v*` by default (`v0.4.0`, `v1.2`). It is edited by hand in `~/.config/iboard/iboard.toml` for now: there is no Settings › Stats tab yet (ticket 006). A tag that doesn't parse as a version still shows, labelled with its name. Annotated tags use the tagger date, lightweight ones the commit date.
- **Marker**: a 1pt dotted line in `ink-faint`, full height, through all four charts at the tag's day (placed inside its week, not snapped to the sample). Until weekly samples land (ticket 006) the trends are per commit, so a release sits on its tag's commit. Dotted so it never reads as the solid crosshair or the dashed `status-doing` limits. No colour: a release is context, not a state.
- **Lane**: one 16pt row above the first chart holds the tag names, 11pt Inter 500 in `ink-muted`, centred on their line with a 3pt tick. Patch releases (`x.y.Z` with Z > 0) are a tick only, and their line is drawn at half strength. When two names would touch, the newer keeps its name and the older becomes a tick.
- **Hover** a name, a tick or a line within 4pt: the line turns solid `ink-muted`, its name `ink`, and a tooltip reads "v0.4.0 · 14 Aug · 37 commits since v0.3.1". The shared crosshair moves to that week as usual. Clicking does nothing.
- **Opt out**: a **Releases** checkbox in the Trends head, left of the range, checked by default. Unchecked, the lane and every line go, and the charts move up by 16pt. Saved at once in `~/.config/iboard/iboard.toml` as `[stats] show_releases = false`, next to `release_tags = "v*"`. With no matching tag, the checkbox is disabled with the tooltip "No release tags match v*".
- **Ranges**: on `1y` and `All`, only minor and major releases are drawn; patches come back on `12w` and `26w`.

## Refactor watch

Four signals, each with its numbers and its limit, marked with an icon and a word as well as the colour (`status-doing` warning triangle, or `status-done` check):

| Signal | On when (defaults, Settings › Stats) |
| --- | --- |
| Complexity grows faster than code | complexity per kLOC up more than 5% over 12 weeks, or above 130 |
| Coverage is sliding | down 3 points or more over 12 weeks, or under the 70% floor |
| Hotspots | files with complexity ≥ 60 and ≥ 8 changes in 12 weeks |
| Cycle time drifts | 12-week median more than 50% above the 12 weeks before |

- With 0 or 1 signals on, the verdict reads "No refactor needed now" on `surface-sunken`, with no button.
- With 2 or more on, it reads "**Time to plan a refactor.** 3 of 4 signals are on, mostly from 3 files." and offers **Draft a refactor issue**. That opens the NewIssueModal, its notes prefilled with the signals and the hotspot files, labelled `refactor`. Nothing is written until you confirm.

## Cost

Under the trends, what the agents cost over the days the trends cover: the range's total, a bar per day stacked by provider, and a row per provider.

- **Range**: the trends'. Until the trends' `12w` `26w` `1y` `All` control lands, that is the page's days, from its oldest commit to its newest, and Newer and Older move both. The caption names the span and the runs: `14 Sep – 11 Nov · 6 runs`.
- **The total** is the money metered runs were charged: `$4.20` in 18pt, then `spent` and the range's tokens in `meta`. A subscription's API-equivalent never joins it. Hovering the total separates the two: `Paid · $4.20` then `API-equivalent · $9.80, never charged`.
- **The chart** is tokens per day, stacked by provider: every run reports tokens, while a subscription's dollars are often zero. Each provider keeps one label-palette colour across the days, matched by the swatch in its row, and its segments use `radius-sm` rounded only at the top of the bar. A day without a run is a gap, never a zero bar. Hovering a bar names the day, its tokens by provider, and its paid and API-equivalent dollars.
- **A row per provider**: its swatch and name, then its tokens. A metered provider's dollars are the rightmost figure: `DeepSeek · $3.10 · 1.2M tokens`. A subscription shows its tokens alone, `Copilot · 2.3M tokens`, and its API-equivalent stays in the tooltip, never in the spend: `Copilot · 2.3M tokens · 4 runs · $1.50 API-equivalent, never spend`.
- **Empty**: no run in the range reads `No agent run in this range.` and `A finished run's cost lands here.`, with no chart and no zero bar.
- **Data**: `~/.local/share/iboard/usage.jsonl`, read on the block's own job. There is no cost per ticket: the log is keyed by session, and nothing joins it to an issue.

## Biggest files

Columns: File (directory in `ink-faint`, name in `ink`), Lines (number plus a bar scaled to the largest file), Complexity, Coverage (`status-doing` under the floor), Changes in 12 weeks, Hotspot (flame icon plus "hotspot" in `status-doing`, else "–"). Sort with the segmented control or a column head: Lines (default), Complexity, Changes. It shows the top 8, with "Show all 64". Clicking a row opens the file's history.

## Behaviour

| Screen | Command (user) | Event | Next screen |
| --- | --- | --- | --- |
| Board | Stats in the ProjectMenu footer, `⌘I` | StatsOpened | Stats; cached samples show at once, the current week rescans |
| Stats | Refresh, or the repo changes | MetricsScanned | Same screen; tiles, trends and watch update in place |
| Stats | Hover a trend | (none) | Shared crosshair; row values show that week |
| Stats | Pick a range | StatsRangeChanged | Trends redrawn for that range |
| Stats | Newer, Older | StatsPageChanged | The trends and the Cost block read the page's days |
| Stats | Hover a cost bar | (none) | Its day's tokens, paid and API-equivalent |
| Stats | Toggle Releases | ReleaseMarkersToggled | Trends redrawn with or without the release lines |
| Any | A new tag matching `release_tags` | TagsScanned | Its line appears on the next scan |
| Stats | Sort the files | FilesSortChanged | Table reordered |
| Stats | Draft a refactor issue | RefactorIssueDrafted → NewIssueOpened | NewIssueModal, notes prefilled |
| Stats | A tool is missing (no lcov, no complexity tool) | MetricUnavailable | That tile and row say what is missing and how to add it; the rest still shows |

Read model: `WeeklyMetrics` holds one row per week with loc, by_language, complexity, coverage (optional) and cycle median/p85, plus `FileMetrics` for the current commit. Both are fed by `MetricsScanned`. `Releases` holds (tag, date, is_patch) from `TagsScanned`, read with `git for-each-ref refs/tags`. `Cost` holds one day per day of the page, each with a part per provider (tokens, paid, API-equivalent), and a row per provider; it is read from the usage log by the canonical project path.

## States

- **Loading**: Skeleton rules. Tiles and charts keep their labels and axes, with pulsing placeholders. Each block loads on its own, started once it is on screen, with a 60s timeout; a block that fails says why and offers Retry (ghost), and the others still show.
- **A metric without data**: its tile says "No coverage report found" and names the file it looks for (`lcov.info`). Its row in Trends shows the same message instead of a chart.
- **No cost in range**: the Cost block keeps its place and says it has no run; the other blocks still show.

In egui, the trends are hand-painted, each row on its own `Painter`: `Shape::line` for a series, `Shape::dashed_line` for limits in `p.status_doing` and for p85, and a `Mesh` of per-segment quads for the area, since epaint fills only convex polygons. Releases are `Shape::dashed_line` with a 1pt dash and 3pt gap in `p.ink_faint` (half alpha for patches); the lane is one more `Painter` row above the first chart, sharing its x transform. The screen keeps the hovered commit and every row draws its crosshair from it.
