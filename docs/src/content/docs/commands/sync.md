---
title: primum sync
description: Pull, merge and push now.
---

```bash
primum sync
```

Fetches the remote, merges, pushes, and then syncs with GitHub when the
project is connected. With a remote set (and not `--manual`), every change
already syncs by itself; this is for a manual remote or to check right away.

Prints what came in and went out, conflicts left, and the GitHub result. See
[history and sync](/guides/history-and-sync/).
