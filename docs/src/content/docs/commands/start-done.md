---
title: primum start and done
description: Mark an item in progress, or done.
---

```bash
primum start 87      # status: doing
primum done 87       # status: done, finished: today
```

Shorthands for `primum set ID --status doing` and `--status done`. Marking an
item done records the date in `finished` and removes its id from every other
item's `needs`.

To close an item you will not do, use `primum set ID --status wontfix`.
