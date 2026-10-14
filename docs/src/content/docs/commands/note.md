---
title: primum note
description: Append a dated note to an item's text.
---

```bash
primum note TODO:87 --file note.md
echo "Sam confirmed the CSV columns." | primum note TODO:87 --file -
```

Appends the note under a heading with today's date (`## 2026-10-14`),
leaving the rest of the text as it was.

| Option | Effect |
|---|---|
| `--file FILE` | The note, from a markdown file; `-` reads standard input |
