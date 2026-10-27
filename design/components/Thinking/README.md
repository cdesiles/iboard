# Thinking

What iboard shows while an agent works, usually around 10s for a Challenger round. It is the iboard "i" itself, walking around a square: its two blocks take turns stretching and shrinking until the i stands again a quarter turn on. One shape, one rhythm, nothing to read.

## The mark

- The i on a 44×44 grid: blocks 12 thick with 3 corners, the dot 12×12 in a corner, a 5 gap, the stem 27 long. 28×28 in panels (`ib-think`), 14×14 inline (`ib-think--sm`). Stem in `ink`, dot in `accent`: no recolouring, no `review`.
- Motion, one quarter (1.6s). The beats overlap, so one block is always starting as the other settles and the mark never stops dead. Every move eases in-out (`cubic-bezier(.65,0,.35,1)`).

  | Beat | Time | What moves |
  | --- | --- | --- |
  | Stretch, then slide | 0–50% | the blue dot's leading edge runs on in one eased move; the dot grows across the stem to the stem's length (27), then keeps going as a bar without pausing, freeing the corner |
  | Tuck | 38–70% | the stem shrinks into that corner as a 12×12 block, never closer than 4.7 to the blue |
  | Hand over | 64–100% | the black block grows along the side to 27 while the blue shrinks into the next corner |

  The whole mark then turns a quarter (a jump, invisible since both poses match), so four quarters walk the square in 6.4s. The two blocks never touch: the blue is out of the corner before the stem arrives.
- Reduced motion: no animation, the i stands still; the elapsed clock still shows progress.
- Inline it sits before the label: the ReviewButton reads "[i] Reviewing… `esc`".

## In the RefinePanel

The panel opens at its final size straight away and keeps its shape, so nothing jumps when the answer lands.

- **Why pane**: placeholder bars (`surface-sunken`, 8–12pt tall, pulsing with the i; see Skeleton) for the TL;DR and two paragraphs. From round 2, a one-line receipt sits on top: "Round 1 · you chose **Split into four tickets**, with the note …", on `surface-sunken`.
- **Pick pane**: the status line (mark, a sentence, the elapsed seconds in `meta`), then four placeholder choice cards with their keys in `ink-faint` and `line` edges, then the note, disabled ("Available once the Challenger answers").
- **Foot**: `esc` cancel hint, Cancel (ghost), and Send disabled.
- The head gains "round N" in `review` from round 2.

## Copy over time

| Elapsed | Line |
| --- | --- |
| 0–20s, first round | "Challenger is reading the issue" |
| 0–20s, later rounds | "Challenger is weighing your answer" |
| 20s+ | "Still working", then in `ink-muted`: "Longer issues can take up to a minute. `esc` cancels and keeps your answer." |
| Timeout (the tool's own, or 120s) | The failed state: `danger` text naming the tool, Retry, and the answer kept in the note |

No percentage and no fake progress bar: iboard does not know how far the model is, so it only says what it is waiting for and for how long. The clock counts whole seconds.

## Behaviour

- `esc` (or Cancel) stops the agent, puts the previous round back with your choice and note still filled in, and emits RefineRequestCancelled. Nothing is written.
- The screen reader hears the status line once (`role="status"`), then the elapsed time every 10s, not every second. The panel carries `aria-busy` until the answer lands.
- When the answer arrives the placeholders are replaced in place; focus moves to the first choice so `1`–`4` work at once.

The consumer provides the start time, the round number and the agent's state. In egui, `theme::thinking_mark` paints the mark from `ui.input(|i| i.time)`; every block stays axis-aligned, so it is two `rect_filled` calls per frame.
