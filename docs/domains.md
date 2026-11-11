# The domain map

iboard is one crate behind two binaries. This page names the ten product
domains, the vocabulary and code each one owns, and the direction the code is
allowed to point. It is also where a ticket's topic label comes from.

A domain owns its words and the modules that define them. The owner column
below is therefore code, not a person: the modules today and the folder the
extraction tickets build.

## The rule

```text
shell → domains → kernel → platform
```

| Layer | Owns | May name |
| --- | --- | --- |
| shell | the egui application: windows, screens, widgets, theme, menus, dialogs | every domain, the kernel, platform |
| domains | the product rules and the words for them | the kernel, platform, and the domain edges below |
| kernel | the vocabulary every domain agrees on | platform |
| platform | the OS and the tools: files, git, processes, jobs, markdown, formatting | nothing above it |

Only the shell draws: egui and the `gui` feature stay there. A lower layer
never names a higher one, and a domain names another domain only along the
edges listed in [Domain edges](#domain-edges).

`tests/architecture.rs` holds this line. It maps every module to a layer and
fails when a lower layer names a higher one, or a non-shell module uses egui.
`make check` also builds the lib without the `gui` feature. When a module
moves, classify it in that test: the test is the ratchet, not a one-time audit.

## The shared kernel

The kernel is the vocabulary two or more domains need to agree on. It is
egui-free and names no domain.

| Kernel vocabulary | Lives in | What it is |
| --- | --- | --- |
| `Issue` | `src/kernel/issue.rs` | an issue file: path, title, labels, summary, markdown, modified |
| board types | `src/kernel/board.rs` | `Board`, `Column`, `ColumnKind`, `Repo`, `display_name`, `project_name` and the `is_*_column` predicates |
| label names | `src/kernel/labels.rs` | `normalize`, `split_leading_labels`, `display_title`, `front_matter_labels`, `most_used`, the origin labels, `Hue` and its palette order |
| config sections | the two `iboard.toml` files | the section names and their owner, below |

`Issue` carries the file projection only. `built_in` and `done_at` leave it in
018; the card and the reader get those facts from a shell `CardFacts` view
model instead.

The section names are kernel vocabulary even though the file I/O belongs to
Preferences: a domain owns its section's type, Preferences loads, merges and
saves them.

| File | Section | Owner |
| --- | --- | --- |
| project `iboard.toml` | `root`, `columns`, `root_column_name`, `title`, `[reader] sections` | board |
| user `iboard.toml` | `[ai]`, `[build]`, `[titles]` | agent |
| user `iboard.toml` | `[filter]` | board |
| user `iboard.toml` | `[stats]` | insights |
| user `iboard.toml` | `[release]` | delivery |
| user `iboard.toml` | `[[workspace.group]]` | workspace |
| user `iboard.toml` | `[appearance]`, `[delete]`, `[auto]`, `[notifications]`, `[labels]`, `[label_style]`, `[direnv]`, `[idesign]`, `[splits]` | preferences |

`iboard::issue`, `iboard::discovery` and the other root re-exports are
compatibility aliases for the binaries and the tests during the extraction. A
domain imports from `kernel` or from the owning domain directly, never through
another domain's alias.

## The ten domains

The numbers in the last column are the extraction tickets in `issues/`.

| Domain | Vocabulary it owns | Code it owns (today → target) |
| --- | --- | --- |
| **board** | `Board`, `Column`, `ColumnKind`, `Repo`, discovery and `Located`, priority, trash, filter, commits, history, the project `iboard.toml` | `src/domains/board/` (done) |
| **agent** | `Agent`, `AgentConfig`, `ReviewMode`, `Verdict`, `Report`, `Leash`, `Challenge`, `Refinement`, the build queue, Ask, `Plan`, `Draft`, `Engine`, prompts, `BuildLog`, review sidecars | `src/domains/agent/{runtime,workflows,artifacts}` (done) |
| **authoring** | `Sheet`, `Section`, checklist progress, word count, `Prose`/`Block`, `Diff`/`File`/`Hunk`/`Line`; the draft and parked notes of `new_issue` | `src/domains/authoring/` (done) |
| **workspace** | `Workspace`, `Group`, `WorkspaceSection`, `Order`, the group draft, a project's repo paths | `src/domains/workspace/` (done) |
| **delivery** | `Release`, `ReleaseChoices`, `ReleaseSection`, the tag, the changelog note and the push | `src/domains/delivery/` (done) |
| **insights** | `Snapshot`, `Measures`, `Cycle`, `Trend`, the release line, `Agenda`/`DayCommit`/`Range` and the watch | `src/domains/insights/` (done) |
| **design** | `DesignSystem`, `Token`, `Theme`, the `Catalog` and its pages, components, assets, CSS, Rust tokens, scaffold and validation | `src/domains/design/` (done); `design_ui` stays shell; `design/` on disk stays the shared format |
| **preferences** | `Preferences`, `UserConfig`, `Section`, appearance, delete confirmation, notifications, label colours and presets, splits, filter suggestions, the idesign switch | `src/domains/preferences/` (done); the bell stays shell |
| **shell** | `App`, screen, frame, dialog and widget vocabulary, `Theme`, `Intent`, `Action`, the keymap, `CardFacts` and the other view models | `src/shell/` (done); `settings_ui`, `design_ui` and `pasteboard` stay at the crate root and are shell by the same rule |
| **platform** | `Job`/`Cancel`, `Process`, `Git`/`Batch`, `File` and atomic writes, the markdown structure, paths/times/numbers formatting | `src/platform/` (done) |

## Domain edges

These are the only domain-to-domain names the map allows. Each one is a
composition or a read of a lower domain, never a callback upward.

| From → to | Why |
| --- | --- |
| preferences → agent, board, delivery, insights, workspace | it composes their config `Section` types, and its own; no domain imports `Preferences` or `UserConfig` |
| workspace → board | a group is a board of boards |
| agent → board | workflows move issues, read their priority and key the build log and reviews by issue |
| agent → design | prompts embed the project's design system |
| delivery → board | `commits` is a Board read model: it says what closed an issue, not how to release |
| insights → board | stats and the agenda read columns, issues and the dates in git |
| insights → agent | the stats cost block reads the project's usage log to report the agents' spend |

Every domain may also name the kernel and platform, as the rule says;
authoring and design have no domain edge of their own. Any other edge is a
leak. Add it here and file the ticket before writing the call.

## Known boundary leaks

These are the leaks the extraction tickets cut. The ticket's own acceptance
criteria are the source of truth for the fix.

None is open: every module in the map above sits in the code that owns it.

## Ticket labels

The domain name is the ticket's topic label, from now on: `[refactor] [board]`,
`[bug] [shell]`, `[feat] [insights]`. A ticket that cuts a leak names both
sides (`[board] [agent]`). `[ux]` and `[quality]` are cross-cutting tags that
follow the domain.

A ticket about a screen is shell; a ticket about a rule is its domain; a
ticket about a word two domains share is the kernel, and only then.

## Keeping the map true

- Classify a module in `tests/architecture.rs` when it moves or appears.
- A new cross-domain name needs a ticket and a row in Domain edges.
- Put a rule in its domain, a screen in the shell, shared vocabulary in the
  kernel, and OS or tool access in platform.
- Ten domains is the product's shape: a change that fits none is probably
  shell or platform.
