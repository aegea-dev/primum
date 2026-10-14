---
title: primum resolve
description: Settle a conflict a sync left.
---

```bash
primum resolve TODO:87                               # asks per field
primum resolve TODO:87 --field impact --keep 2
primum resolve TODO:87 --field body --keep 1
primum resolve TODO:87 --field items/TODO-87.spec.md --keep both
```

On a terminal with no options, shows each conflicting field's two sides and
asks which to keep. With `--field` and `--keep` it decides one field without
asking.

| Option | Effect |
|---|---|
| `--field F` | A field (`status`, `impact`, ...), `body`, or a document's path |
| `--keep K` | `1` or `2` (the sides as shown), or `both` for documents |

See [conflicts](/guides/conflicts/).
