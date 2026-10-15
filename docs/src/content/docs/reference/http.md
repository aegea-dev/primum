---
title: Board HTTP interface
description: The JSON routes behind the board, for scripts on the same machine.
---

The board's pages use a small JSON interface, served on 127.0.0.1 by
`primum serve`. `{project}` is the project's name.

| Route | Does |
|---|---|
| `GET /p/{project}/api/items` | Every item, as `list --json` |
| `POST /p/{project}/api/items` | Change an item: `{"id": "87", "impact": 8}` |
| `POST /p/{project}/api/new` | Add an item: `{"kind": "TODO", "title": "...", ...}`; returns `{"id", "stem"}` |
| `POST /p/{project}/api/items/kind` | Move to a lane: `{"id", "kind"}`; returns `{"old", "new", "stem"}` |
| `POST /p/{project}/api/delete` | Delete: `{"id"}`; returns `{"removed": [...]}` |
| `POST /p/{project}/api/resolve` | Settle a conflict: `{"id", "field", "keep": 1 \| 2 \| "both"}` |
| `GET /p/{project}/api/doc?path=items/87.spec.md` | A document's text |
| `GET /p/{project}/api/history?id=#87` | History entries, as `log --json` |
| `GET /p/{project}/api/history/diff?commit=C&path=P` | One change's diff |

Fields for changes and new items: `title`, `status`, `impact`, `urgency`,
`risk`, `effort` (numbers or numeric text), `needs` (`"#1, #2"` or a
list) and `body`. Errors are a 4xx status with a message.

Changes made here are the same as on the command line: locked, committed and
synced.
