---
title: History and sync
description: Every change is a commit; with a remote, every change syncs, and primum merges for you.
---

## History

Every change is a commit in a git repository primum keeps inside the project,
`.primum/history.git`. The project folder does not need to be a git
repository, and if it is one, primum's history is separate from it.

Edits made in a text editor are kept too: before its next change, primum
commits them as their own entry, "edited outside primum".

```bash
primum log               # the project
primum log TODO:87       # one item, across renames
```

The history tracks `project.json`, `BOARD.md`, `items/*.md`, and the
`conflicts/` folder; anything else in the folder is ignored.

History needs git 2.28 or later. Without git, everything else works.

## Sync

```bash
primum remote set git@github.com:me/board.git    # this machine
primum clone git@github.com:me/board.git         # another machine
```

With a remote, every change syncs by itself, and `primum serve` syncs every
minute. A sync fetches, merges, and pushes, retrying if someone pushed in
between.

Each machine sets its own remote, so `ssh` here and `https` there both work.
Several projects can share one remote on different branches:

```bash
primum remote set git@github.com:me/boards.git --branch my-lib
```

`--manual` syncs only when you run [`primum sync`](/commands/sync/).

## Merging

primum merges items itself, field by field, never with git's line merge:

- Changes to **different items** never meet.
- Changes to **different fields** of one item are both kept: one side's new
  impact and the other's new title.
- **The same field changed the same way** on both sides is one change.
- **The same field changed differently** is a [conflict](/guides/conflicts/).

Two machines adding items to one lane at once would take the same number. A
sync renumbers the local new items to the next free numbers, rewriting every
reference to them, and prints what moved.
