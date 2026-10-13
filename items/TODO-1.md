---
title: "Configurable lanes per project"
status: open
impact: 6
urgency: 3
risk: 3
effort: 6
needs: []
source: "primum"
---

## Why

The five lanes (URGENT, BUG, DEBT, KILLER, TODO) are fixed. A project should be able to name its own lanes, their order and colors, in its `primum.toml`.

## Notes

- Default to the current five, so existing projects do not change.
- Item file names and ids follow the lane names, so a renamed lane renames files.
- The page, the CLI colors and BOARD.md all read the same list.
