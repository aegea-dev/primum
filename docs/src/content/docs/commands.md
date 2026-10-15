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
| [`kind`](/commands/kind/) | Move an item to another lane |
| [`context`](/commands/context/) | The project's context variables |
| [`lifecycle`](/reference/lifecycles/) | Check, show and list lifecycles; attach or detach an item |
| [`secret`](/commands/secret/) | Secrets for lifecycles, in the OS secret store |
| [`delete`](/commands/delete/) | Delete an item and its documents |
| [`serve`](/commands/serve/) | Open the board in the browser |
| [`log`](/commands/log/) | Changes to the project or one item |
| [`projects`](/commands/projects/) | The workspace's projects |
| [`remote`](/commands/remote/) | This machine's remote for the project |
| [`clone`](/commands/clone/) | Make a project from a remote |
| [`sync`](/commands/sync/) | Pull, merge and push now |
| [`resolve`](/commands/resolve/) | Settle a conflict a sync left |
| [`import`](/commands/import/) | Import a videre-board board, once |
| [`migrate`](/commands/migrate/) | Bring a board made by an earlier version up to date |

## Choosing the project

Every command takes `-p NAME` (`--project NAME`) to pick a project of the
workspace by name. Without it, primum uses the project containing the
current folder, else the workspace's only project. See
[workspaces](/guides/workspaces/).

## Ids

An item's id is its number. On the command line write `87` or `#87` (quote
`#87` in shells that read `#` as a comment); output shows `#87` with the
item's kind beside it.

## Help

`primum help`, `primum COMMAND --help` and `primum --version`.
