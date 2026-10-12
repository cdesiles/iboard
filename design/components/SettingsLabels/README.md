# SettingsLabels

The **Labels** tab of the Settings dialog (`⌘,`), last of four: *General*, *AI review*, *Titles*, *Labels*. It sets the colour of each label, renames a label across the board, and adds new ones.

## Anatomy

1. **Label colours.** A field label, then a list framed in `line-strong` with `radius-md`, 16pt inside left and right (`space-4`), 4pt top and bottom. One 28pt row per label, 4pt apart (`space-2`), scrolling past 240pt. The list holds every label with a colour, then the labels issues use without one, sorted by name.
2. **A row.** On the left, the label's Label pill in its colour. On the right, from the edge inwards: a trash icon button (`danger` tone, "Remove this label"), a ghost **Reset** when a preset's colour was changed ("Back to #b12f22"), and an 80pt mono `#rrggbb` field, "no colour" as its placeholder.
3. **Add a label.** A field label, then one row: a 140pt mono name field ("name"), a `#rrggbb` field, the pill as it will look once both are filled, and a secondary **Add** button with a plus.
4. **Help line.** `ui-small` in `ink-muted`: "A label is the [word] at the start of an issue's title. Only changes to iboard's presets are written to [labels]."

## States

| State | What shows |
| --- | --- |
| Rest | One row per label, each with its pill, colour and trash |
| No colour | The hex field is empty; the pill is neutral (`ink-muted` on `surface-sunken`) |
| Invalid colour | The hex field takes a `danger` edge; the pill keeps the last valid colour |
| Preset changed | A ghost Reset appears left of the hex field |
| Renaming | Double-click a pill: it becomes a 140pt mono field holding the name, focused; leaving it renames |
| Adding, incomplete | Add with no name: "Name the label first." in `danger`; with a bad colour: "The colour must be #rrggbb, for example #1a6e42." |
| Empty | No list frame: "No labels yet. Add one below." in `ui-small`, `ink-muted` |
| Save failed | The error in `danger` under the help line |

Everything here waits for the dialog's **Save** (`⌘↩`). Removing a label drops its colour, presets included (`bug = ""`); the issues keep their `[bug]` prefix. A rename, on Save, rewrites the `[old]` prefix of every issue that carries it to `[new]`, and moves its colour along.

## Tokens used

`surface`, `surface-sunken`, `line-strong`, `ink`, `ink-muted`, `ink-faint`, `danger`, `accent` (focus), `radius-sm`, `radius-md`, `radius-pill`, `space-2`, `space-3`, `space-4`, `space-5`, `control-h`, and the label colours themselves.

## Keyboard and accessibility

- `tab` walks the rows: each hex field, Reset, the trash, then the add row. `↩` in the add row does not submit; Add does.
- Double-click is the only way into a rename; `tab` or a click elsewhere commits it, an empty name keeps the old one.
- Every icon button carries a tooltip naming its action, read by VoiceOver as its label.
- `⌘↩` saves, `esc` closes without saving, as in every Settings tab.

## Stored as

```toml
[labels]
bug = "#ff0000"   # a preset, changed
spike = "#123456" # a label of yours
chore = ""        # a preset removed
```

Only what differs from iboard's presets is written, so a preset improved later still reaches labels left untouched. Emits SettingsOpened, LabelColourChanged, LabelRenamed, LabelRemoved, LabelAdded, SettingsSaved.
