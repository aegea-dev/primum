---
title: primum list
description: Items in priority order, filtered by lane or status.
---

```bash
primum list                       # open and in progress
primum list --kind BUG
primum list --status doing
primum list --all                 # closed items too
primum list --json
```

Prints a table: id, title, the four scores, priority, needs and status, in
priority order (lane, then score). Blocked items are marked.

| Option | Effect |
|---|---|
| `--kind KIND` | One lane only |
| `--status LIST` | Statuses to show, like `idle,doing` (any state of a lifecycle) |
| `--all` | Every item, closed ones too |
| `--json` | The items as a JSON array, with `score`, `blocked` and `blockers` |
