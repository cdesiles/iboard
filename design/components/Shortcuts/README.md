# Shortcuts

The cheat sheet that appears with the key hints: while ⌘ alone has been held for `hint-hold` 0.5s, a card sits centred over the screen and lists every key that works there. It fades in with the hints over `motion-hint` 120ms, and is gone in one frame when ⌘ is released or any key is pressed with it.

## When it shows

- On the **Board**, the **Reader**, the **Editor**, the **Diff** and **Stats**.
- **Never over a modal, a popover or a menu**: the foot of a modal and the rows of a menu already carry their keys, and the sheet would cover them.
- **Never at rest**: no key cap is on screen until ⌘ is held.
- It takes no pointer or keyboard focus. The screen under it keeps working; a click or a key goes through, and hides the sheet as it does.

## Anatomy

- **Frame**: `surface`, 1px `line`, `radius-md`, `shadow-modal`, 24pt padding, at most 600pt wide. It sits in the middle of the window and rises 4pt as it fades in.
- **Head**: "Keyboard shortcuts" in `ui-strong` `ink`, and on the right "Release ⌘ to hide" in `meta` `ink-faint`.
- **Sections**, in `ui-label` uppercase `ink-muted`: **Anywhere** (`⌘N`, `⌘,`, `⌘[`, `esc`), the screen's own keys, the **Agent** keys (`⌘R`, `⌘⇧R`, `⌘U`, `⌘K`), and **Stats**' Agenda.
- **Rows**: a key cap (`Kbd`) then what it does in `meta` `ink-muted`, 4pt apart, 20pt tall. VoiceOver reads the pair together ("⌘N New issue").
- **Two columns**, sections balanced between them by height. Narrower than 600pt, the card clamps to the window minus 48pt.

## Copy and truth

- The action is the verb phrase the status bar uses ("open", "edit", "next column"), so the two never disagree.
- A key appears only where it works: Refresh only with Auto off, Refine only on Todo and only in the Reader and Editor, Review only on Todo, Update only where an issue can be updated, Ask only with an agent configured, Archive only on Done, and the stats Agenda keys only under Stats.
- Stats takes the keyboard for itself (`esc`, `⌘R` and the Agenda's arrows and ranges), so its card lists those alone.
