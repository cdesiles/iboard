# IBOARD

## Concurrent contributions

Many agents may work at the same time on the project. When asked to commit, only commit what's relevant to the current work, leave alone the rest.

Commit with explicit file paths: `git commit -- <file>...`. A folder pathspec such as `issues/todo` also sweeps up other agents' changes in it.

## Definition of done

`make check` passes: formatting, clippy with `-D warnings`, swift-format and tests. Run it before moving a ticket to done.

The limits only ever go down: `too-many-lines-threshold` in `clippy.toml` and `MAX_LINES` in `tests/layout.rs`. Lower them when your change shortens the longest function or file; never raise them. Rust is pinned in `rust-toolchain.toml`; upgrade it on purpose, together with the lints it brings.

`make hooks` installs a pre-commit hook that lints the staged snapshot in a copy, so it never touches the shared working tree.

## Issue management

Every time you are asked to perform some action, check that this action is not available in the ./issues/{todo,ready,doing} folder.

If it is not present, make a very succinct 3-5 line description of the intent context as a new issue ticket and place it accordingly in the ./issue ticket board. Do not ask confirmation for it.

When a task is finished, move the ticket to done and commit your changes. The commit message must contain
a reference to the closed ticket like so:

```
closes 000-board-and-reader.md
```

A ticket's title starts with its labels (see `design/components/Label`). Give it one kind: `[bug]` (fixes too), `[feat]` (not `[feature]`), `[discovery]`, `[chore]`, `[refactor]` or `[perf]`. Topic labels such as `[ux]`, `[design]` or `[quality]` may follow.

Ticket file names must stay unique across every column: the `closes <file>.md` line is what links a ticket to its commits.

## Tests

- Temp folders come from `tempfile`; they clean up even when a test fails.
- Never start a real agent from a test: give the app a fake custom command (`printf '…' # {prompt}`).
- App tests drive frames with the shared `frame`, `draw`, `key`, `moved` and `button` helpers, or with `egui_kittest`, finding widgets by label. A custom widget must report its role and label with `widget_info`, which also lets VoiceOver read it.
- Prompts are `insta` snapshots: after a deliberate change, run `INSTA_UPDATE=always cargo test` and review the `.snap` diff.

## Design system

This repo has a design system at design/; tokens in design/tokens.json, components in design/components/.

## Comments

We don't like comments. Use them very wisely, when the case is very tricky.
