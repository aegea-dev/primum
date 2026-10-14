---
title: Items, lanes and scores
description: What an item is, the five lanes, the four statuses, and how priority is computed.
---

## Items

An item is one markdown file in `items/`, named for its id: `items/87.md`
is `#87`. It has a title, a kind, a status, four scores, the ids it needs,
and a markdown text. See [item files](/reference/item-files/) for the exact format.

## Lanes

Every item has a kind, and kinds are lanes, in this order:

| Lane | For |
|---|---|
| `URGENT` | must happen now |
| `BUG` | something broken |
| `DEBT` | work that makes later work cheaper |
| `KILLER` | features that set the project apart |
| `TODO` | everything else |

## Numbers

One counter numbers every item in a project, in any lane: `1`, `2`, `3`...
The number is the item's address: its file is `items/3.md`, the command
line takes `3` or `#3`, the board's page is `item/3`, and `#3` in any item's
text or document links to it. A number is never reused, even after a
delete. Moving an item to another lane ([`primum kind`](/commands/kind/))
changes its `kind:` line and nothing else.

## Statuses and lifecycles

An item's status is its state in its [lifecycle](/reference/lifecycles/).
Items follow the built-in `manual` lifecycle unless attached to another:
`idle`, `doing`, `done` and `wontfix`, moved by a person. `done` and
`wontfix` are closed: they leave `primum list` and the board's default
view, and their ids are removed from every other item's `needs`. An item
attached to another lifecycle has that lifecycle's states, and is closed in
the states of its closing steps.

## Scores and priority

| Score | Question |
|---|---|
| impact | How much does it matter? |
| urgency | How soon does it matter? |
| risk | What goes wrong while it waits? |
| effort | How much work is it? |

Each runs from 1 to 10. An item's priority is

```text
(impact + urgency + risk) * 10 / effort
```

rounded to one decimal, half to even. Items are ordered by lane, then by
priority, highest first. Scores left out when adding default to impact 5,
urgency 3, risk 3, effort 5.

## primum and WSJF

primum's priority is a form of **Weighted Shortest Job First** (WSJF), the
ordering rule from Don Reinertsen's *The Principles of Product Development
Flow*, later made popular by SAFe. WSJF divides the cost of delay by the size
of the job, so small valuable work goes first:

```text
WSJF     = cost of delay / job size
         = (business value + time criticality + risk reduction) / job size

priority = (impact + urgency + risk) * 10 / effort
```

| WSJF | primum |
|---|---|
| business value | impact |
| time criticality | urgency |
| risk reduction and opportunity enablement | risk |
| job size | effort |

The differences are deliberate:

- **Lanes come first.** WSJF ranks one backlog; primum ranks within each lane,
  so an `URGENT` item is never outranked by a high-scoring `TODO`.
- **Plain 1 to 10 scores** instead of relative Fibonacci estimates compared
  across the backlog: each item can be scored on its own, by one person, in
  seconds.
- **Blocked items wait.** An item whose `needs` are open is skipped by
  `primum next`, whatever its score.
- **Times ten** keeps the priority readable (`32.0` rather than `3.2`); the
  order is the same.

primum's statuses (`idle`, `doing`, `done`) form a Kanban-style pull flow, and
the board shows items in progress before the next one: finish what is
started before starting more. There are no sprints, story points or
velocity; primum is a backlog and a flow, not a sprint planner.

## Needs and blocked items

`needs: [#1, #2]` says an item waits for others. While any of them is
open, the item is blocked: the board marks it and `primum next` skips it.
Closing an item removes its id from every `needs`, so nothing stays blocked
by finished work.

## BOARD.md

Every change rewrites `BOARD.md`, a read-only summary of the project: counts,
then a table per lane. It is for reading on GitHub or in a pull request; do
not edit it.
