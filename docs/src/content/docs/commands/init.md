---
title: primum init
description: Make a project, in a workspace or in the current folder.
---

```bash
primum init my-app                  # in a workspace: the subfolder my-app
primum init my-app --name "My app"  # with a display name
primum init                         # outside a workspace: this folder
```

Writes `project.json` and an empty `items/`, and starts the project's
[history](/guides/history-and-sync/) when git is available. In a
[workspace](/guides/workspaces/), `primum init FOLDER` makes the project in
that subfolder and adds it to the workspace's list.

| Option | Effect |
|---|---|
| `FOLDER` | The workspace subfolder to make the project in |
| `--name NAME` | The project's name, shown on tabs and used by `-p`; default: the folder's name |

The folder does not need to be a git repository, and should not be your
code repository if that is open source: keep boards in their own folder.
