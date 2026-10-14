---
title: Many projects
description: Several projects on one machine, as tabs on one board.
---

Each project is a folder with its own `primum.toml`, `items/` and history.
This machine's list of projects, in tab order, is
`~/.config/primum/projects.toml`:

```toml
projects = ["~/boards/my-app", "~/boards/my-lib"]
```

`primum init` and `primum clone` add to it; `primum projects add PATH` and
`primum projects remove NAME` edit it.

```bash
primum projects
```

`primum serve` shows one tab per project. A project whose folder is missing
shows as unavailable without stopping the others.

## Picking a project

On the command line, `-p NAME` picks a project. Without it, primum uses the
project containing the current folder, else the only one listed.

```bash
cd ~/boards/my-app && primum next     # my-app
primum next -p my-lib                 # from anywhere
```

## Where to keep boards

Keep boards in their own folders, for example `~/boards/<project>`, not in a
code repository: a board's history and sync are separate from the code's.
