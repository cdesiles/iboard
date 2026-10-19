# Thinking

What iboard shows while an agent works, usually around 10s for a Challenger round. It is the iboard "i" itself: the stem holds still and the card-dot rocks gently, as if the issue were being weighed. One shape, one motion, nothing to read.

## The mark

- The logo glyph, cropped to the i: 14×28 in panels (`ib-think`), 7×14 inline (`ib-think--sm`). Stem in `ink`, card in `accent`, exactly as the Logo: no recolouring, no `review` on the card.
- Motion: the card turns about its bottom centre from its resting −8° to +5° while lifting 2.5pt, then back. 1.6s, ease in-out, endless. The stem never moves.
- Reduced motion: the card rests at −8°, which is simply the logo; the elapsed clock still shows progress.
- Inline it sits before the label: the ReviewButton reads "[i] Reviewing… `esc`".

## In the RefinePanel

The panel opens at its final size straight away and keeps its shape, so nothing jumps when the answer lands.

- **Why pane**: placeholder bars (`surface-sunken`, 8–12pt tall, no shimmer) for the TL;DR and two paragraphs. From round 2, a one-line receipt sits on top: "Round 1 · you chose **Split into four tickets**, with the note …", on `surface-sunken`.
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

The consumer provides the start time, the round number and the agent's state. In egui, `theme::thinking_mark` paints the mark for a given time.
