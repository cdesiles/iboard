# Changelog

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
