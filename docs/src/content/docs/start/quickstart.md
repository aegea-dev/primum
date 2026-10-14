---
title: Quickstart
description: A project, three items, the next one to do, and the board, in five minutes.
---

## Make a project

Any folder can hold a project. It does not need to be a git repository.

```bash
mkdir -p ~/boards/my-app && cd ~/boards/my-app
primum init
```

`init` writes `primum.toml` and an empty `items/`, starts the project's
history, and lists the project on this machine.

## Add items

```bash
primum add BUG "Empty board crashes on + new" --urgency 8
primum add TODO "Export open items as CSV" --impact 6 --effort 3
primum add DEBT "Split server.rs into routes and pages" --effort 6 --needs TODO:1
```

Each prints its id: `BUG:1`, `TODO:1`, `DEBT:1`. `DEBT:1` waits for `TODO:1`
and is blocked until that is closed.

## Decide what to do

```bash
primum list      # open items, highest priority first
primum next      # the top unblocked items
primum start BUG:1
primum done BUG:1
```

## Open the board

```bash
primum serve
```

The board opens in your browser at `http://127.0.0.1:8777/`. Click a row to
open an item, edit its scores in place, or add one with **+ new**.

## Next

- [Items, lanes and scores](/start/concepts/): how the order is computed.
- [History and sync](/guides/history-and-sync/): the same board on two machines.
- [GitHub Issues](/guides/github-issues/): items and issues in step.
