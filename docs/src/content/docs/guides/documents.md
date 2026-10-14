---
title: Specs, plans and other documents
description: Documents that sit beside an item and show as tabs on the board.
---

An item's documents sit next to it in `items/`:

```text
items/TODO-87.md          the item
items/TODO-87.spec.md     its spec
items/TODO-87.plan.md     its plan
items/TODO-87.report.md   any items/<ID>.<type>.md
```

A document belongs to an item by its file name: nothing else links them. The
board shows each one as a tab beside the item's text, and moving the item to
another lane renames its documents with it.

```bash
primum doc TODO:87 spec --file spec.md
```

or write the file yourself in any editor.

## Showing another item's document

An item can show a document that belongs to another item by listing it in
its front matter:

```yaml
docs: [items/TODO-80.spec.md]
```

Useful when one spec covers several items. Deleting `TODO:80` removes the
entry from every `docs` list.
