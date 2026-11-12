# UpdateDialog

The small modal behind the status bar's `Update to 0.5.0`: it says which release is newer, then
downloads it, then restarts iboard in its place.

- **Only an installed app updates itself.** A copy run from a checkout (`target/debug`, `make run`)
  never offers one: it has no bundle to replace, and its toolchain already knows.
- **The check is quiet.** iboard asks `GET /repos/cdesiles/iboard/releases/latest` when it opens and
  every six hours, off the UI thread. A draft, a prerelease, an older tag, a release with no
  `iboard-<version>-aarch64-apple-darwin.zip`, or a failed request offers nothing and says nothing.

## The offer

- The status bar carries a `Ghost` text button before the usage pill, on every screen that shows the
  bar: Board, Reader, Editor, Diff and Stats. `Update to 0.5.0` in `meta`, `ink-muted`, turning `ink`
  under the pointer. It is the only new thing the bar ever gains, and only while a newer release
  exists.
- While the download runs it reads `Downloading update…`; once staged, `Restart to update`.
  A click always reopens this dialog.
- Closing the dialog does not cancel the download: the status bar keeps saying where it is, and
  the staged bundle waits for the next click.

## The dialog

A Modal S: `modal-s` 360pt on `surface`, `line` edge, `radius-md`, `shadow-modal`, 20pt padding,
over the `scrim`.

1. **Title** "Update iboard?" in `ui-title`.
2. **Subject**, a `surface-sunken` box: `iboard 0.5.0` in `ui-strong`, then `You're on 0.4.0.`
   in `meta` `ink-muted`.
3. **Body** in `ui-small` `ink-muted`: "It replaces this copy and reopens it on this project.
   Your issues and settings stay where they are."
   - Downloading: "Downloading iboard 0.5.0…" with the primary disabled.
   - Staged: "iboard 0.5.0 is ready. Restarting replaces this copy and reopens it on this project."
   - A failure adds its reason below in `danger`; the primary reads "Try again".
4. **Foot**, right to left: the primary, then a secondary.
   - At rest: `Download` (`↩`) then `Later` (`esc`).
   - Downloading: disabled `Downloading…`, then `Cancel`, which stops the download.
   - Staged: `Restart to update` (`↩`) then `Later`.

## Behaviour

| Do | Result |
| --- | --- |
| `Download`, `↩` | The zip lands in the cache, unpacks to `iboard.app`, and the dialog says it is ready |
| `Restart to update`, `↩` | The staged bundle replaces this copy after iboard quits, and `open` reopens it on the project, or `--all` for a group board |
| `Later`, `esc`, a scrim click | The dialog closes; a running download keeps going in the background |
| `Cancel` while downloading | The download stops; the status bar offers the update again |

The swap itself is a shell script, written to the cache and started before iboard quits: it waits
for this process to exit, moves the old bundle aside, puts the staged one in its place, removes
the old one, and opens the app with the project path. If the move fails it restores the old bundle,
so a failed update never leaves no app behind.

The workspace comes back because the relaunched app is handed the same project path; a group board
reopens with `--all`. The Reader's exact screen is not restored.

## Accessibility

- The modal is an `AlertDialog` named "Update iboard?"; `↩` confirms it, `esc` cancels it.
- The status bar button's name is its label; its tooltip names the release, or what the download
  is doing.
- Downloading is not colour alone: the primary says "Downloading…".

## What it deliberately does not have

- **No progress percentage.** The zip is a few megabytes on a normal connection; the state is
  downloading, ready, or a reason it failed.
- **No automatic restart.** Replacing the bundle and reopening the window is the user's click.
- **No release-note rendering.** The dialog answers what changes for this copy, not what shipped.
