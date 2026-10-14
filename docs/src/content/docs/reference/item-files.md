---
title: Item files
description: The exact format of items/<ID>.md.
---

An item is `items/<KIND>-<N>.md`: a flat YAML front matter, then the text.

```markdown
---
title: "Adoption metrics: daily snapshot job and a Metrics page"
status: doing
impact: 7
urgency: 6
risk: 2
effort: 4
needs: [TODO:64]
docs: [items/TODO-80.spec.md]
github: 123
finished: 2026-10-13
source: "imported from board.py"
---

The item's text, as markdown.
```

| Field | Value | Written |
|---|---|---|
| `title` | JSON-style quoted text, so any characters are safe | always |
| `status` | `open`, `doing`, `done` or `wontfix` | always |
| `impact`, `urgency`, `risk`, `effort` | 1 to 10 | always |
| `needs` | ids it waits for | always, `[]` when empty |
| `docs` | other items' documents it shows | only when not empty |
| `github` | its issue number | only when linked |
| `finished` | the date it was done | only when set |
| `source` | where it came from: `primum`, `GitHub #45`, or an imported board's value | only when not empty |

primum writes exactly this form, and reads only it. Anything else, such as a
missing field, a score out of range, or an unknown status, is an error naming
the file and line, never a silent default. Fix the file and run the command
again.

## Documents

`items/<KIND>-<N>.<type>.md`, any markdown. See
[documents](/guides/documents/).
