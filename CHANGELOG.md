# Changelog

## v0.5.0 — 12 Nov 2026

Added a usage screen charting every provider's daily spend, one status-bar usage pill, and background Claude builds that ask from the board and survive a restart.

### New
#### Agent
- Keep a usage log per project
- Log a service build's messages and every tool call
- A Claude build runs in the background
- Claude builds are found again after a restart
- A Claude build asks and listens from the board
- A subscription meter knows its pace and breakdown
- Copilot's credits and reset come from GitHub
- The agent log shows what the model thought
#### Shell
- Offer a ring per subscription plan
- The Reader's actions are header buttons, not a … menu
- The ring dashes an unknown share, and a gauge with a pace tick
- One usage pill replaces the ring and name per provider
- The usage popover opens a row per provider to its insights
- Tools › Usage…: the screen, its rail and its empty states
- The usage screen's main pane: tiles, chart, models and projects
- The spend and the usage share are one status-bar trigger
#### Design
- FoldersDialog: offer a project the board folders it lacks
- Specify the UsageMeter: one status bar pill and a pace popover
- Specify how the agent's thinking reads in the AgentLog
- Claude.ai's session and week are two windows
- IssueGone: say where the issue went when the Reader loses its file
- Usage screen: every provider's consumption by day, and a slimmer popover
- The spend and the usage share are one trigger
#### Board
- Offer the projects missing standard folders at launch
- Move to… from the keyboard
- The Doing card's question is a "Waiting on you" note
- The Reader says where its issue went
#### Delivery
- Write the opening from the commits' bodies too
- Install and update from a GitHub release
- A version tag builds the macOS bundle and publishes it on GitHub
- A project's prepare hook runs before the release commit
#### Insights
- The usage read model
#### Preferences
- Declare a provider's plan

### Fixed
- Run the release off the UI thread and rewrite the opening on open
- Fold a service build's steps in the agent log
- Inset the agent log's switch, Now band and composer
- A one-day usage chart names today, and an empty one draws nothing
- A Doing card keeps room for the strip it always shows
- Align list dots and tighten item spacing
- The usage popover names the providers behind today's spend

## Unreleased

### New
#### Delivery
- Install iboard from a GitHub release, and update it from the status bar

## v0.4.0 — 11 Nov 2026

### New
#### Ux
- Save a new issue with ⌘S, and show the foot keys beside their controls
- Holding ⌘ lists every key of the screen on a Shortcuts card
- Builds run in the opencode service, so they outlive the window
- A build that needs an answer asks from the board
- The bell, the question in the ticket, and the agent's conversation
- A compact question card, a card pin, and a readier bell
- Release from one panel
- Copy the board's path from the status bar
- Apply the Question card ruleset to the build's intervention flow
#### Agent
- Plan mode files tickets from a grilled conversation
- Cut the agent log into a timeline of runs
- Run opencode scripts in a git worktree
#### Delivery
- Build the release note from kind and domain labels
- Open the note with a sentence, and group Added by domain
#### Design
- Specify the Question card and the bell's place
- Specify the AgentLog timeline component

### Fixed
- The filter's placeholder sits on its field's middle line
- The release note scrolls, groups into sections, and starts after the last tag
- Block editing while the release note groups
- Read only a session's newest page
- List build questions for the project's location
- Release from Done with no issues when commits wait
- Rebuild the bell around waiting questions
- Draw the bell from the design system
- Read opencode api replies from a file
- Keep every action icon in sight while a label shows
- Align the AgentLog outcome mark with its first line
- Name the issue from the project in build and ask prompts
- Keep inline code's background one rectangle
- Keep tests off the real opencode service
- Write the note opening with a draft, not a review
