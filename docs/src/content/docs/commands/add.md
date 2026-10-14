---
title: primum add
description: Add an item to a lane, with scores, needs and text.
---

```bash
primum add KIND [TITLE] [options]
```

Adds an item to the lane `KIND` (`URGENT`, `BUG`, `DEBT`, `KILLER` or `TODO`)
with the project's next number (one counter for every lane), and prints it. Without a title, the
text's first heading (any level, without its `#` marks) is the title, and
the heading stays in the text; with neither, nothing is added.

```bash
primum add TODO "Export open items as CSV" --impact 6 --effort 3 --body-file csv.md
primum add BUG "Empty board crashes on + new" --urgency 8 --body-file crash.md
primum add URGENT "Login fails after the 2.3 upgrade" --impact 9 --urgency 10 --risk 8 --effort 2
primum add DEBT "Split server.rs into routes and pages" --effort 6 --needs 12
primum add KILLER "Offline mode" --impact 9 --effort 8 --needs 3,7
primum add TODO "Dark mode for print" --body-file - < notes.md
primum add TODO --body-file csv.md        # the title is the text's first heading
primum add TODO "Weekly digest" -p my-lib
```

| Option | Effect |
|---|---|
| `--impact N` | How much it matters, 1-10; default 5 |
| `--urgency N` | How soon it matters, 1-10; default 3 |
| `--risk N` | What goes wrong while it waits, 1-10; default 3 |
| `--effort N` | How much work it is, 1-10; default 5 |
| `--needs IDS` | Ids it waits for, like `#1,#2` |
| `--body-file FILE` | Its text, from a markdown file; `-` reads standard input |
| `-p NAME` | The project |

The item is written to `items/<n>.md`, `BOARD.md` is rewritten, and
the change is one commit in the history. To add many at once, see
[adding many items](/guides/adding-many-items/).
