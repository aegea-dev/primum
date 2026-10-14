---
title: primum doc
description: Add or replace a document beside an item, like a spec or a plan.
---

```bash
primum doc TODO:87 spec --file spec.md
primum doc TODO:87 plan --file plan.md
primum doc TODO:87 report --file - < report.md
```

Writes `items/TODO-87.spec.md` (any type: `spec`, `plan`, `report`, ...),
replacing it if it exists. The board shows it as a tab beside the item's text.

| Argument | Meaning |
|---|---|
| `ID` | The item the document belongs to |
| `DOC_TYPE` | The document's type, part of its file name |
| `--file FILE` | Its text; `-` reads standard input |

See [documents](/guides/documents/).
