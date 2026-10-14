---
title: primum delete
description: Delete an item and its documents.
---

```bash
primum delete 87
primum delete 87 --yes     # do not ask
```

Removes the item's file and its own documents, and its id and documents from
every other item. On a terminal it asks first; anywhere else it needs `--yes`.

The item stays in the [history](/guides/history-and-sync/). To close an item and keep it, set its status
to `wontfix` instead.
