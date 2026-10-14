---
title: primum clone
description: Make a project from a remote, on another machine.
---

```bash
primum clone git@github.com:me/board.git
primum clone git@github.com:me/boards.git my-lib --branch my-lib
```

Creates `FOLDER` (default: the remote's name without `.git`), fetches the
project's history into it, checks out the items and `project.json`, and
sets the remote. Run in a [workspace](/guides/workspaces/), the new project
joins it.

| Option | Effect |
|---|---|
| `FOLDER` | Where to put it |
| `--branch NAME` | The remote branch; default `main` |
