---
title: Commands
description: Every primum command, what it does, and the option they all share.
---

| Command | Does |
|---|---|
| [`init`](/commands/init/) | Make a project, in a workspace or here |
| [`workspace`](/commands/workspace/) | Make a workspace, or migrate earlier boards |
| [`add`](/commands/add/) | Add an item; prints its id |
| [`list`](/commands/list/) | Items in priority order |
| [`show`](/commands/show/) | One item: scores, needs, text and documents |
| [`next`](/commands/next/) | What to work on now |
| [`set`](/commands/set/) | Change an item's fields |
| [`start`, `done`](/commands/start-done/) | Mark an item in progress, or done |
| [`note`](/commands/note/) | Append a dated note to an item |
| [`doc`](/commands/doc/) | Add or replace an item's document |
| [`kind`](/commands/kind/) | Move an open item to another lane |
| [`delete`](/commands/delete/) | Delete an item and its documents |
| [`serve`](/commands/serve/) | Open the board in the browser |
| [`log`](/commands/log/) | Changes to the project or one item |
| [`projects`](/commands/projects/) | The workspace's projects |
| [`remote`](/commands/remote/) | This machine's remote for the project |
| [`clone`](/commands/clone/) | Make a project from a remote |
| [`sync`](/commands/sync/) | Pull, merge and push now |
| [`resolve`](/commands/resolve/) | Settle a conflict a sync left |
| [`import`](/commands/import/) | Import a videre-board board, once |

## Choosing the project

Every command takes `-p NAME` (`--project NAME`) to pick a project of the
workspace by name. Without it, primum uses the project containing the
current folder, else the workspace's only project. See
[workspaces](/guides/workspaces/).

## Ids

Ids are written `KIND:N`, like `TODO:87`. The kind is not case sensitive on
the command line: `todo:87` works.

## Help

`primum help`, `primum COMMAND --help` and `primum --version`.
