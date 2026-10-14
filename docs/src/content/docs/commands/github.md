---
title: primum github
description: Keep a project's items and a GitHub repository's issues in step.
---

```bash
primum github connect owner/repo         # open items become issues, open issues become items
primum github connect owner/repo --all   # closed ones too
primum github status
primum github disconnect                 # stop; links in items stay
```

| Subcommand | Does |
|---|---|
| `connect REPO` | Connect and run the first sync. `--all` also files closed items and takes in closed issues, so references to old items resolve |
| `status` | The repository, whether `gh` is ready, and the last sync |
| `disconnect` | Stop syncing with GitHub; items keep their issue numbers |

Needs [`gh`](https://cli.github.com), logged in. See
[GitHub Issues](/guides/github-issues/).
