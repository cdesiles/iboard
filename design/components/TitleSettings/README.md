# TitleSettings

The **Titles** tab of the Settings dialog: which engine names new issues from their description.

## Where it lives

The one dialog becomes **Settings** (`⌘,`), with four tabs in its header: *General* (GeneralSettings), *AI review* (ReviewSettings), *Titles* and *Labels*. A second dialog would need its own shortcut and menu entry for a single radio choice. A tab keeps `⌘,` as the only door, and the Review agent choice can point straight at the tab next door. `⌘,` reopens the last tab used; the Review menu's "AI settings…" opens AI review, and clicking the engine tag in the composer opens Titles.

Tabs are a segmented control (`surface-sunken` track, active tab on `surface` with a `line-strong` edge), 24pt tall, right of the "Settings" title.

## The choice

"Name new issues with", a radio list framed in `line-strong`. The selected row takes `accent-soft`. Each row has the name in `ui-strong`, a state in `meta` on the right (dot plus word), and one line of `ui-small` help:

| Engine | Help line | State when ready |
| --- | --- | --- |
| On-device (Apple), default | Offline, private and free. Nothing leaves this Mac. | `Ready` in `status-done` |
| Review agent | Uses the tool and model from AI review. Any OS; needs that tool set up and a network. | the tool and model, e.g. `opencode · default` |
| Off | You type every title. The title line is always shown in the composer. | (none) |

## Availability, said honestly

Checked each time the dialog opens, never cached across launches. An unavailable engine stays selectable. Its state turns `status-doing` (fixable) or `ink-muted` (not on this machine), and a line in `ink` under the help says why, with the one button that fixes it:

| Situation | State | Why line | Button |
| --- | --- | --- | --- |
| Linux | Not on Linux | Needs macOS 26 or later with Apple Intelligence. | none |
| macOS before 26 | Needs macOS 26 | This Mac runs macOS 15.6. | none |
| Mac not eligible (Intel) | Not on this Mac | This Mac can't run Apple Intelligence. | none |
| Apple Intelligence off | Apple Intelligence off | It is turned off on this Mac. Turn it on, then come back. | Open System Settings |
| Model still downloading | Preparing | Apple Intelligence is still downloading its model. | none |
| Review agent with no tool | Set up first | No tool is set up for AI review yet. | Set up AI review (switches tab) |

On macOS these map to the Foundation Models availability reasons (device not eligible, Apple Intelligence not enabled, model not ready).

**Languages.** When On-device is ready, a `meta` line lists the languages macOS reports for the model. It shows the first four, `+N` for the rest (the full list in a tooltip), and says whether the Mac's own language is one of them: "yours: Français". The list is read from the system, never hard-coded. A description in an unsupported language goes to the composer's Failed state with the text kept. The Review agent handles whatever its model handles, so it shows no list.

**No silent fallback.** While the chosen engine is unavailable, the composer behaves as Off and says so with its "auto title off" tag. iboard never switches to the Review agent on its own, because that would send text over the network the user did not choose.

## Stored as

```toml
[titles]
engine = "apple"   # "apple" | "agent" | "off"
```

The consumer provides the current engine, an `availability(engine) -> Ready | Unavailable{reason}` and, for On-device, the supported languages. Emits SettingsOpened, TitleEngineChosen, SettingsSaved.
