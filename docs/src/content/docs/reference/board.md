---
title: The board
description: The local web board that primum serve opens.
---

`primum serve` opens the board at `http://127.0.0.1:8777/`, one tab per
project.

## The board page

- **Next up** above the table: the top unblocked item, or, while items are
  in progress, those first. Finish what is started before starting more.
- **The table**: items in priority order, with scores, priority, needs,
  documents and status. Scores and status are edited in place; closed items'
  scores are not editable. Blocked items show what they wait for.
- **Lanes as filters** in the toolbar, with counts; **hide done**; and
  **Sort** by priority, id, any score, title or status (or click a column).
- **+ new** adds an item.
- A document badge opens the item on that document.
- Keys: `j` or down and `k` or up move between rows, Enter opens one.

## The item page

The item's text, rendered, with its documents as tabs beside it; every field
editable; its history with each change's diff; and, after a sync that
could not decide something, each conflicting field's two sides to choose
between.

## Light and dark

The board follows the system's theme.
