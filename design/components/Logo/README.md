# Logo

**An i whose dot is a card.** The stem is a column; the dot is an issue card, lifted and tilted as if it is being dragged to the next column. It says what iboard is (issues on a board) with two shapes, so it still reads at 16pt.

## Parts

- **Glyph**: a 12×27 stem with 3pt corners and a 20×13 card above it, rotated −8°, offset to the right. On a 64 grid. Stem in `ink`, card in `accent`, so it follows the theme.
- **App icon**: the glyph on a `#15181d` tile with macOS corners (22.5%) and a faint `#353b45` edge so it holds on dark docks, on Apple's 1024 grid (824 tile, 100 margin). From 64pt up, the card shows two text lines and two faint column rules frame the stem; at 32 and 16 those details drop.
- **Wordmark**: "iboard" drawn, not typeset, from the same parts: 6pt stems and rings, the i's dot is the card, centred on the i's stem (it only looks offset because of its −8° tilt). It needs no font, so it renders the same everywhere. Lowercase, always one word.
- **Wordmark spacing**: optical, not equal. Gaps are 5 between two straight sides (a|r), 3 between two rounds (b|o, o|a) and 1.5 from the r's arm to the d's bowl, because the open space under the arm already reads as gap. i|b is 8 from stem to stem, so the tilted card (which overhangs the stem by about 3) keeps the same 5 to the b's ascender. Equal gaps made straight pairs look cramped and round pairs loose. Keep these ratios if the wordmark is redrawn.

## Use

- Where it shows: the app icon in the Dock, the About window and the `.app` bundle; the wordmark on the welcome screen (no folder opened yet). The HeaderBar keeps the project name as text, since it names the user's repo, not the app.
- The card is the only coloured part. Never recolour it with a status colour, never use `review` for it, never add a gradient or shadow.
- Clear space: one stem width (6/64 of the glyph) on every side. Smallest size: glyph 14pt, wordmark 16pt tall.
- Dark tile on both themes: the app icon does not invert in light mode, as macOS icons don't.

## Files

In this folder's `files/` and in the project folder `design-system/logo/`: `iboard-icon.svg` (1024 master), `iboard-icon-small.svg` (16–32), `iboard-glyph-light|dark.svg`, `iboard-wordmark-light|dark.svg`, PNGs from 16 to 1024 and `iboard.icns` for the bundle.

In egui, load `iboard-256.png` into `egui::IconData` and pass it to `ViewportBuilder::with_icon`; the `.icns` goes in the `.app` bundle (`cargo-bundle` reads it from `[package.metadata.bundle] icon`).
