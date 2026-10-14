---
title: primum projects
description: The workspace's projects, and adding or removing one.
---

```bash
primum projects                   # name, folder, and whether it opens
primum projects add my-lib        # put the subfolder my-lib on the list
primum projects remove my-lib     # take it off; the project is not touched
```

The list is the workspace's `primum.toml`, in tab order. Outside a workspace,
`primum projects` shows the project you are in. See
[workspaces](/guides/workspaces/).
