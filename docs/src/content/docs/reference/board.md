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
- **Lanes as filters** in the toolbar, with counts of what the table can
  show (closed items count only while **hide done** is off); **hide done**;
  and **Sort** by priority, id, any score, title or status (or click a
  column).
- **+ new** adds an item.
- A document badge opens the item on that document.
- Keys: `j` or down and `k` or up move between rows, Enter opens one.

## The item page

The item's text, rendered, on the left, with its documents as tabs and its
history (each change with its diff) on the right; after a sync that could
not decide something, each conflicting field's two sides to choose between.

- **edit**, under the item's fields, makes its title, status, scores, kind
  and text editable; the score follows the scores as they change. **save**
  keeps every change, **cancel** drops them. A closed item has no **edit**:
  its status changes on the board.
- A document's own **edit** does the same for the document shown.
- The line between the two sides drags them wider or narrower, and is
  remembered per project; a double-click puts them back to half and half.
  Each side scrolls on its own, its head staying in view. On a narrow
  window the documents go below the text.

## Light and dark

The board follows the system's theme.
