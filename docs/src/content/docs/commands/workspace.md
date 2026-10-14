---
title: primum workspace
description: Make a folder a workspace, or migrate boards made by earlier versions.
---

```bash
primum workspace init       # this folder holds projects
primum workspace migrate    # convert earlier boards in this folder's subfolders
```

| Subcommand | Does |
|---|---|
| `init` | Writes `primum.toml` listing the projects in this folder's subfolders |
| `migrate` | Turns each subfolder's legacy `primum.toml` into `project.json` (one commit in its history), and makes this folder a workspace in the old central list's order. Prints what changed; run twice, it changes nothing |

See [workspaces](/guides/workspaces/).
