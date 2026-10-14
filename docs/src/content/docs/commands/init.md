---
title: primum init
description: Make the current folder a project.
---

```bash
primum init
primum init --name my-app
```

Writes `primum.toml` and an empty `items/` in the current folder, starts the
project's [history](/guides/history-and-sync/) when git is available, and
adds the project to this machine's list so `serve` shows it.

| Option | Effect |
|---|---|
| `--name NAME` | The project's name, shown on tabs and used by `-p`; default: the folder's name |

The folder does not need to be a git repository, and should not be your
code repository if that is open source: keep boards in their own folder.
