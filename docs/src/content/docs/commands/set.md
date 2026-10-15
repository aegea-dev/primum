---
title: primum set
description: Change an item's title, scores, status, needs or text.
---

```bash
primum set 87 --impact 8 --effort 2
primum set 87 --title "Export open and closed items as CSV"
primum set 87 --needs 4,2
primum set 87 --needs ""            # needs nothing
primum set 87 --status wontfix
primum set 87 --body-file new.md
```

Only the fields given change. Prints the id and the fields changed.

| Option | Effect |
|---|---|
| `--title TEXT` | The title |
| `--impact`, `--urgency`, `--risk`, `--effort N` | Scores, 1-10 |
| `--status S` | `idle`, `doing`, `done` or `wontfix` |
| `--needs IDS` | Replace the ids it waits for; `""` for none |
| `--body-file FILE` | Replace its text with a markdown file; `-` reads standard input |

To change the lane, use [`primum kind`](/commands/kind/).
