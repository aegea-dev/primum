---
title: primum import
description: Import a videre-board board into an empty project, once.
---

```bash
primum init
primum import ~/projects/old-board
```

Reads a board made with videre-board's `board.py` (`data/items.json` with its
`specs/` and `plans/`) into this empty project: items keep their ids, scores
and needs, and specs and plans become documents beside the items they belong
to. Prints what was copied and any document linked to no item, which is left
behind.
