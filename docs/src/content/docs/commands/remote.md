---
title: primum remote
description: This machine's remote for the project.
---

```bash
primum remote                                   # the remote, branch and mode
primum remote set git@github.com:me/board.git
primum remote set git@github.com:me/boards.git --branch my-app
primum remote set https://example.com/board.git --manual
primum remote remove                            # stop syncing; history stays
```

| Option | Effect |
|---|---|
| `--branch NAME` | The remote branch; default `main`. Several projects can share a remote on different branches |
| `--manual` | Sync only with `primum sync`, not after every change |

The remote is kept in the project's history settings, per machine: `ssh` on
one and `https` on another both work. See
[history and sync](/guides/history-and-sync/).
