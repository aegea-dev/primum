---
title: "History and sync through a bare git repository"
status: open
impact: 9
urgency: 5
risk: 4
effort: 8
needs: []
source: "primum"
---

## Why

Every change to a board should be versioned without anyone committing, and several people or machines should be able to change the same board with primum merging for them. A person sees a merge only when the same field of the same item changed differently on two sides.

A bare git repository inside each project (`.primum/history.git`) keeps the files plain markdown and needs no git repository around them. See the spec and plan beside this item.
