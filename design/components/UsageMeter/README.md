# UsageMeter

What the bar's two figures cost: what the board spent today, and how much of each subscription is left, kept out of the way. The status bar carries one small trigger; a click opens a popover that names today's spend and answers one question per window: will I make it to the reset? It is the quick look and nothing more: the consumption over time, by model and by project lives in the UsageScreen, one link away (Tools › Usage…).

A provider may have more than one window. Claude.ai has a rolling five-hour session and a week, each with its own share, pace and reset, so each is a row of its own. A provider with one window keeps one row.

It replaces the ring and name per provider, which took about 215pt of the bar and stayed empty until a cap was set: the pill takes about 50pt. It also absorbed the board's spend, which sat beside it as a second, separate figure: the price is now the left half of the same control.

## The trigger

One control at the right of the status bar, `meta` in `ink-muted`: the board's spend, then the ring and the share.

- **The spend**, on the board only: today's metered dollars (`$28.61`) in `ink` once anything was spent, `ink-muted` at `$0.00`. The bar never names the build; the word `Building` lives in the popover, one click away.
- **A ring and a figure**: the 14pt ring and the share of the window closest to its limit, `62%`, in `ink`. Closest means the highest share. Nothing names the window: the popover does.
- **One control**: the price and the percentage are the same button, `space-3` apart with no separator between them, so the pair takes the click, the focus ring and the fill as one. `control-h` tall, `space-3` wider than its content each side, `radius-sm`.
- **Over pace** (see Pace): the ring's arc and the figure turn `status-doing`. Nothing else in the bar is coloured, so it is the one thing that moves the eye.
- **No cap on any provider**: a dashed ring and the word `Usage`. It never shows a share it does not know.
- **No subscription plan at all**: the spend alone is the control, and its popover leads to Full usage.
- **Neither**: no spend, no plan window — the Reader and the Editor on a metered plan — draw no trigger.
- **Hovering says nothing**: no tooltip, no hover fill, only the pointing-hand cursor. The popover holds every figure the tooltip used to; a hover is not a place to read the cost.
- **While the popover is open** the trigger stays filled `surface-hover` (`aria-expanded`), so it reads as the popover's anchor.
- The hit area is `control-h` tall and wider than the content by `space-3` each side, so the figures sit where the text did and the fill shows around it.

## The popover

380pt wide, above the trigger and right-aligned to it, `space-2` clear of the bar. `surface`, `line` edge, `radius-md`, `shadow-popover`. It closes on `esc` or a click outside; it does not dim the window and does not trap focus.

1. **Head** `bar-h` less the rule: "Usage" in `ui-title`. Not "Subscription usage": the popover speaks for the spend too, and a board may have no plan window at all.
2. **The spend row**, on the board, above the windows: `Today` in `ui-strong` — `Building` while a run is burning — and today's dollars in `ui-title` at the right, `$28.61`, in `ink-muted` at `$0.00`. Under it, in `meta`: `This week · $142.30` on the left, `Last session · $12.40` at the right when there was one. The row carries no gauge: the week and the last session are the whole story here, and the UsageScreen breaks them down.
3. **One row per window**, in Settings' order and, inside a provider, the session then the week. A row is not a disclosure: it never opens. It has three lines:
   - the ring, the row's name in `ui-strong`, and the share in `ui-title` at the right: `62%`. The name is the provider's alone when it has one window (`Copilot`), and `Claude.ai · Session` or `Claude.ai · Week` when it has several, so a repeated provider still reads apart;
   - the gauge, 6pt tall, with a **pace tick**;
   - the verdict in `meta`, then `resets in 3d` at the right. A session names a time when it says its cap is reached (`cap by 15:40`); a week names a weekday and a month a date.
   The row carries no daily chart, no model split and no token counts: those moved to the UsageScreen.
4. **Foot**: at the right, `Full usage` (opens the UsageScreen) and `Settings › AI`, under a `line` rule. The left stays empty: the tick needs no legend, since each verdict already says `Under pace` or `Over pace`.

Rows are separated by `line-soft` only, the spend row from the first window too. There is no card per provider, and no card per window either.

### Several windows

A plan declares at most one window and one cap: the one the user counts from the usage log. A provider may also report windows of its own, and a report replaces the declared window of the same length while it lasts. Claude.ai reports its five-hour session and its week; each has its own reset, so each is a row.

- Each row owns its gauge, pace tick, verdict and reset. Nothing is shared between two windows of the same provider.
- The pill still carries one figure: the highest share of any window. The popover names which.
- A window the provider reports but cannot count (no reset, or a stale one) is left out entirely, never shown as current.

## Pace

The gauge's tick marks where an even pace would be: the share of the window already gone, over its length. A week that is 50% elapsed has its tick at the middle of the gauge.

- **Under pace** (used share at or below the tick): the verdict is `Under pace`, the fill is `accent`.
- **Over pace** (used share past the tick): the verdict is `Over pace · cap by Sat`, naming the weekday the cap is reached if usage keeps its average rate so far; the fill, the ring and the verdict are `status-doing`. A month's window names a date (`cap by Nov 24`) and a session a time (`cap by 15:40`) instead.
- Colour is never the only signal: the verdict is words, and the tick is `ink`.
- No cap, no pace: the tick, the share and the verdict go.

`status-doing` on `surface` is 5.8:1 light and 7.7:1 dark.

## No cap set

Today's state for every provider until the user declares a cap in Settings › AI, so it is designed, not an error.

- The ring and the gauge have a dashed `line-strong` outline and no fill: dashed means unknown, solid and empty means 0%.
- The share reads `No cap` in `ink-muted`; the sub-line reads `Resets in 19d`.
- The tokens burnt and the dollars are in the UsageScreen, not here. The popover foot's `Settings › AI` is the way to set a cap.

## Units

A cap is counted in whatever the provider counts: tokens for a cap the user sets (`3.1M of 5M tokens`), AI credits for Copilot (`9,212 of 30,000 AI credits`, as GitHub's own usage page writes it). The figure is `used of cap unit`. Without a cap the row has no figure: the tokens burnt are in the UsageScreen, the one place the usage log is broken down.

## Where the figures come from

Tokens, API-equivalent dollars, run counts, the per-day chart and the model split come from the usage log. A subscription's dollars are API-equivalent and never called spend.

The share and the reset come from the first of these that exists:

1. **The provider.** Copilot's account endpoint reports premium credits used, the entitlement and the reset date, so Copilot has a real share without any setup. Claude.ai's usage endpoint reports `five_hour` and `seven_day`, each a utilization and a reset, so Claude.ai has two real shares instead of one. Both are read with the token the tool already stores: opencode's GitHub OAuth token for Copilot, Claude Code's OAuth credential for Claude.ai (the macOS keychain item `Claude Code-credentials`, or `~/.claude/.credentials.json` elsewhere). The popover says so in the open row (`from GitHub`, `from Claude.ai`) so a reported figure is not mistaken for a counted one.
2. **A cap the user declared** in Settings › AI, counted from the log. It fills one window: the plan's.
3. **Nothing.** The share is unknown and drawn dashed; no percentage is made up.

A provider-reported share can be stale or missing (offline, expired token, a provider that refused): the row then falls back to the next source instead of showing an old figure as current. A provider that reports one window and not another keeps the fallback for the missing one.

## Behaviour

| Do | Result |
| --- | --- |
| Click the trigger, `↩` or `space` on it | The popover opens |
| `tab` | Moves through `Full usage` then `Settings › AI`; the rows are not controls |
| `esc`, a click outside, or the trigger again | The popover closes; focus returns to the trigger |
| `Full usage` | Tools › Usage… opens (UsageScreen); the popover closes |
| `Settings › AI` | Settings opens on the AI review tab; the popover closes |

Nothing animates: the trigger has no hover, and only the open fill changes.

## Accessibility

- The trigger is a `<button>` with `aria-haspopup="dialog"` and `aria-expanded`. Its name carries what the bar stopped showing: the spend line, then each window's name, share, pace and reset — the tooltip it no longer has.
- The popover is a labelled dialog. Each window's gauge is `role="img"` with a label that gives the share and the even-pace share. Rings are `aria-hidden`: the figure beside them says the same.
- Contrast: `accent` on `surface-sunken` is above 3:1, `ink-faint` day labels above 4.5:1, and the chart bars are not the only carrier of a value: the tokens are written above them.

## In egui

`status.rs` draws one trigger where it drew two responses: `board_spend` goes, and `meter_rings` becomes `usage_trigger`, which takes the board's figure (`Option<String>`, `None` off the board), the closest share, `warn` and `open`. `widgets::usage_pill` takes the same pieces and paints `$28.61`, the ring and `62%` inside one `Sense::click` rect, with no `on_hover_text`; its `widget_info` name is the spend line plus `pill_detail`, so VoiceOver still reads every figure. The popover's `body` takes the spend (`today`, `week`, `last`, `running`) and draws its spend row above the windows; its head reads `Usage`, and the `legend` helper and its CSS go. `meter_rings` still opens the popover through `egui::Popup` anchored `TOP_END`, with `POPOVER_W` 380. `subscription_meters_with` returns one `Meter` per window: a provider's report adds its windows (Claude.ai's `Session` and `Week`), and the plan's own window is the fallback when it reports none. The popover rows are plain, with no open state to key. `widgets::ring` draws the dashed track for an unknown share. The pace and the projected day are computed from the window's start and reset, never stored; a provider's reset anchors the start, so a session need not begin on the hour or a week on Monday.
