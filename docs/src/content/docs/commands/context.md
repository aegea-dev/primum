---
title: primum context
description: A project's context variables, stored and computed.
---

```bash
primum context                          # every variable
primum context --json                   # {stored, computed, lifecycle}
primum context set start 2026-11-01     # store one (JSON, else text)
primum context set budget 1500
primum context set budget --unset
```

**Stored** variables live in `project.json`'s `context`, beside the
project's other settings, and travel with its history. **Computed** ones
come from the board: `items`, `open`, `closed`, and a count per status
(`status.idle`, `status.doing`, ...). Lifecycles and the board read both;
the board's API serves them at `/p/<project>/api/context`.
