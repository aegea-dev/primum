---
title: primum log
description: Changes to the project, or to one item, newest first.
---

```bash
primum log
primum log 87
primum log -n 50 --json
```

Each change is one commit in the project's history: its date, author and
message (`#87: impact 5 -> 8`). Edits made outside primum show as their
own entry.

| Option | Effect |
|---|---|
| `ID` | Only changes to that item, across renames |
| `-n N` | How many; default 20 |
| `--json` | As JSON |
