---
title: Conflicts
description: What a sync could not decide, and how to settle it.
---

A conflict is one field of one item changed differently on two sides of a
sync: you set impact to 8 while someone else set it to 6. Everything else
merges by itself.

A sync never stops for a conflict. It keeps the item with one side's value,
records both sides in `conflicts/<ID>.json`, and goes on. The board marks the
item, and `primum list` and `primum sync` name it.

## Settling

On the item's page in the board, each conflicting field shows both sides with
who changed it and when; keep one.

On the command line:

```bash
primum resolve 87                           # asks per field
primum resolve 87 --field impact --keep 2   # without asking
```

For a document changed on both sides, `--keep both` keeps both texts, one
after the other, to merge by hand.

Settling a field is a change like any other: one commit, synced.
