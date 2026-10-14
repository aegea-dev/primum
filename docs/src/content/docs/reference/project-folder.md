---
title: A project folder
description: What primum keeps in a project's folder, and what the history tracks.
---

```text
my-app/
  project.json             the project's name and settings
  BOARD.md                 a read-only summary, rewritten on every change
  items/
    TODO-87.md             an item
    TODO-87.spec.md        its documents
  conflicts/TODO-87.json   what a sync could not decide
  github/TODO-87.json      the last version agreed with its issue
  .primum/history.git      the project's history, a bare git repository
  .primum.lock             held while a change runs
```

## project.json

```json
{
  "name": "my-app",
  "github": { "repo": "owner/repo", "ids": "title" }
}
```

| Key | Value |
|---|---|
| `name` | The project's name, shown on tabs and used by `-p` |
| `github.repo` | The repository its issues are kept in step with |
| `github.ids` | How ids show on issues: `"none"` (default) or `"title"` |

Written by primum, tracked in the history; unknown keys are kept. A project
from an earlier version has a `primum.toml` with `name` instead, read until
[`primum workspace migrate`](/commands/workspace/).

The project's folder sits in a [workspace](/guides/workspaces/), whose
`primum.toml` lists it. Nothing about it is kept elsewhere on the machine:
the remote and sync settings live in the history's own git config, per
machine.

## Changes

Every change takes the project's lock, writes files through a temporary file
and a rename, removes closed ids from every `needs`, rewrites `BOARD.md`, and
commits. Two commands, or a command and the board, never write over each
other.
