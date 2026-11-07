# [feat] [ux] A build that needs an answer asks from the board

An unattended build stops on its `question` tool and waits, burning nothing.
`GET /api/form` lists those pending forms with the session, a title and typed
fields (options, free text); answering is `POST /api/session/{id}/form/{id}/reply`.
Show the question on the ticket and answer it there, so a build never waits on a
terminal nobody is looking at.

## Acceptance criteria

- [x] A pending form shows on its issue's card, and in a badge on the board.
- [x] Answering it from iboard resumes the run; the form disappears.
- [x] Tool approvals (`GET /api/permission/request`) use the same feed.
- [x] Claude Code and custom commands keep the child-process path.
