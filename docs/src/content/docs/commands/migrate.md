---
title: primum migrate
description: Bring a board made by an earlier version up to date, once.
---

```bash
primum migrate                 # in one board
primum workspace migrate       # in a workspace: every board in it
```

Boards made before one counter had ids per lane (`TODO:7`, `BUG:7`) and
files named after them (`items/TODO-7.md`). `primum migrate` converts such
a board once:

- numbers every item `1, 2, 3...` in the order the items were created
  (their first commit in the history);
- renames items, documents and conflict records (`items/TODO-7.spec.md`
  becomes `items/3.spec.md`);
- rewrites references: `needs: [TODO:7]` becomes `needs: [3]`, `TODO:7` in
  item text and documents becomes `#3`, `items/TODO-7.` paths follow;
- writes each item's `kind:`;
- commits once, `number every item`, and prints each old id with its new
  number.

Running it again changes nothing. Until a board is converted, commands that
read it stop with `this board uses ids from an earlier version: run primum
migrate`. Convert a board on one machine, sync it, and clone it afresh on
the others.
