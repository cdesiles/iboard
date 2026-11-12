# Usage screen

The full view of what the AI providers cost and consume, one provider at a time. **Tools › Usage…** opens it as a screen of its own, like Stats: the header bar stays, the board is not behind it.

It completes the UsageMeter and does not repeat it. The status-bar pill and its popover answer one question about now: will I make it to the reset? This screen answers the other: how much did each provider burn, day by day, over the last week, month or quarter, and on what. The popover keeps no chart and no model split; its foot links here (see UsageMeter).

## Layout

Top to bottom, on `bg`:

1. **Header bar**: **Board** (`esc`), a `line` divider, the crumb `Usage`, then at the right `Read 14:02 · from the usage log` in `meta`, **Refresh**, and the bell last. Same bar and height as every screen.
2. **Rail** (left, `208pt`, a `line` rule on its right) and **main** (the rest, `space-6` padding, `space-5` between blocks).
3. **Status bar** as everywhere. The usage pill shows only when a provider reports a plan window.

### The rail

Only with **two or more providers**. A vertical tablist (`role="tablist"`, `aria-orientation="vertical"`), 28pt rows:

- **All providers** first, with a bars icon, then a `line-soft` rule, then one row per connected provider, in the order the tool lists them.
- A provider row is its **swatch** (8pt rounded square in its own `label-*` hue) and its name. Nothing else: the numbers live in the main pane, so a fact is said once.
- Selected: `accent-soft` fill, `ink` text. Hover: `surface-hover`. `↑ ↓` move between rows; `Tab` enters the rail once and leaves it.
- A provider the tool lists but that never ran still has its row.

Provider hues are fixed by the provider and shared with Stats' Cost block: Anthropic `label-orange`, DeepSeek `label-teal`, Copilot `label-pink`. A new provider takes the next free label hue; never `accent` (selection) or `review` (agent).

### One provider

No rail, no **All providers**: the main pane shows that provider directly, under its own head. Adding a second provider brings the rail in, with the selection kept.

### The main pane, per provider

1. **Head**: the swatch and provider name in `ui-title`; under it, in `meta`, how it is paid and what the numbers cover (`Metered · billed per token · counted from the tool's sessions in the Perso group`, `Subscription · Copilot Business · … in iboard`). At the right, the **range**: `7d` `30d` `90d`, a segmented control, `30d` by default. The range governs every block below it, except the plan window.
2. **Plan window** (only a provider that reports one, today Copilot): a panel `Premium credits · This month, whatever the range`. It is the popover's row at full width: the ring, `9,212 of 30,000 AI credits · from GitHub`, the share, the gauge with its pace tick, `Under pace` or `Over pace`, the reset (`resets 1 Dec (in 19d)`) and what is unlimited. Pace rules are UsageMeter's, unchanged.
3. **Four tiles** (`statcard`): **Tokens** with its change against the previous range of the same length (`−3% vs the previous 30 days`, never coloured: more is not worse), **Spent** or **API-equivalent**, **Runs** with the daily average, **Busiest day**.
4. **Per-day chart**, the point of the screen (below).
5. **By model** and, in a group, **By project**, side by side (a lone panel takes the full width), wrapping under each other when narrow: a name (models in `edit-body` 12pt), a 4pt share bar in the provider's hue, tokens, and for a metered provider the dollars. A subscription shows tokens alone.

### All providers

Same head and range. The tiles total every provider. **Spent** is what metered providers charged, and the meta line adds `plus $21.83 API-equivalent, never charged` when a subscription ran: the two never merge. The chart is **stacked by provider** with a legend in its head. Under it, **By provider** (a row opens that provider; swatch, share bar, tokens, spend) and, in a group, **By project** across all providers.

## The per-day chart

- **A bar per day**, today last. `7d` labels every day (`Thu 12`), `30d` every fifth, `90d` every fifteenth, always ending on today in `ink`. Bars are `radius-sm` rounded at the top; stacked bars are split by a 1pt `surface` line. A day without a run is an empty slot, never a zero bar.
- **Y axis**: `0`, half, and a rounded maximum (`1.5M`, `$10`); grid at `line-soft`, the baseline in `line`. One scale per chart.
- **Readout above the chart, always one place**: the selected day (`Today · Thu 12 Nov`), its value in `read-h2`, and a line under it: runs and API-equivalent for one provider, the provider split with swatches for All. At rest it reads today; hovering a bar, or `←` `→` `Home` `End` on the focused chart, moves it, and a `surface-hover` wash marks the day. Leaving puts it back on today. There is no tooltip: the readout does that job, and it never covers the bars.
- **Tokens or Spend**: a segmented control in the head of a **metered** provider's chart. Spend is real dollars. A subscription and All have tokens only, since a subscription's dollars are API-equivalent and must not be charted as spend.
- The chart is `role="img"` with a label naming the range and the busiest day, and is focusable for the arrow keys.

## Scope

The screen measures what the board on screen holds, as the status bar's spend does:

- **A group's board**: the runs of every project of the group. The head names it (`in the Perso group`) and **By project** splits it.
- **A single project, no group**: the runs of that project alone. The head names the project (`in iboard`) and **By project** is not drawn: one row of 100% says nothing.
- The scope follows the board. Opening a group or a project in the ProjectMenu while Usage is open redraws the screen for it; nothing on the screen picks a scope.
- Providers are those the tool lists, whatever the scope. A provider with no run in the scope keeps its rail row and reads `No agent run in this range.`.
- The plan window is the account's, not the scope's: Copilot's month stays whole in any scope. Its panel says so (`This month, whatever the range`).

## Data

- Tokens, runs, dollars, the model and the project come from the usage log (`~/.local/share/iboard/usage.jsonl`), read on its own job and filtered by the project paths of the scope. **This is what the tool's finished sessions in these projects burnt**, not everything the provider billed (a key shared with other machines or apps is invisible): the head says `counted from the tool's sessions in …`, on every provider.
- Metered dollars are the log's cost. A subscription's cost is API-equivalent and is only ever labelled so.
- The plan window comes from the provider (Copilot's account endpoint), as UsageMeter says. A provider with no plan window has no plan block, never a made-up one.
- The range's previous period needs the log from twice as far back. With no run in it, the tile says `No run in the 30 days before`.

## States

| State | What shows |
| --- | --- |
| Two or more providers | Rail with All providers, then each provider |
| One provider | No rail; that provider's pane |
| A single project, no group | The head names the project; **By project** is not drawn |
| A provider that never ran | Its rail row stays; its pane says `No agent run yet.` and `A finished run through DeepSeek lands here.` under the head; in All it reads `No run yet` where its share bar would be |
| A provider with no run in the range | Its tiles read 0 and the chart says `No agent run in this range.` |
| **No tool configured** | No rail. `Configure a tool first`, one sentence (`Usage reads the runs of the tool iboard drives. Choose OpenCode or Claude Code in Settings › AI build. Each provider it connects gets its own tab here, with its usage by day.`) and **Open Settings** (`⌘,`), the screen's one filled button. No Refresh, no read time |
| Tool set, no provider connected | The same blank, headed `OpenCode has no provider connected yet`, with `Connect one in OpenCode.`; no button, since nothing to set in iboard |
| Loading | Skeleton rules: tiles and chart keep their labels and axes. The log read has a 60s timeout; a failure names the file and offers Retry |

## Behaviour

| Screen | Command (user) | Event | Next screen |
| --- | --- | --- | --- |
| Any | Tools › Usage… | UsageOpened | Usage; cached figures at once, the log rereads |
| Usage | `esc`, Board | UsageClosed | The board it came from |
| Usage | Pick a rail row, `↑ ↓` | UsageProviderChosen | Same screen, that provider |
| Usage | Pick a range | UsageRangeChanged | Tiles, chart and lists redrawn |
| Usage | Tokens / Spend | UsageMeasureChanged | Chart redrawn; the choice is kept per session |
| Usage | Hover or arrow on the chart | (none) | The readout follows the day |
| Usage | A row in By provider | UsageProviderChosen | That provider's pane |
| Usage | Refresh, or a run finishes | UsageRead | Same screen, figures in place |
| Usage | Open another group or project in the ProjectMenu | BoardOpened | Same screen, new scope |
| Usage (no tool) | Open Settings | SettingsOpened | Settings › AI build |
| Popover | Full usage | UsageOpened | Usage |

Read model: `UsageView` holds its scope (a group's project paths, or one path), the providers the tool lists (id, name, billing kind, colour), a `DayUsage` per provider and day for 180 days (tokens, runs, dollars), per-model and per-project totals for the range, and an optional `PlanWindow`. The range, the selected provider and the measure are screen state. In egui the chart is hand-painted like Stats' trends: a `Mesh` of quads per segment, the readout drawn from the hovered index.
