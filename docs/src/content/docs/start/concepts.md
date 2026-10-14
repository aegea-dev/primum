---
title: Items, lanes and scores
description: What an item is, the five lanes, the four statuses, and how priority is computed.
---

## Items

An item is one markdown file in `items/`, named for its id: `items/TODO-87.md`
is `TODO:87`. It has a title, a status, four scores, the ids it needs, and a
markdown text. See [item files](/reference/item-files/) for the exact format.

## Lanes

The part of an id before the colon is its kind, and kinds are lanes, in this
order:

| Lane | For |
|---|---|
| `URGENT` | must happen now |
| `BUG` | something broken |
| `DEBT` | work that makes later work cheaper |
| `KILLER` | features that set the project apart |
| `TODO` | everything else |

Numbers count up within a lane. Moving an item to another lane
([`primum kind`](/commands/kind/)) gives it that lane's next number and
rewrites every reference to it.

## Statuses

`open`, `doing`, `done` and `wontfix`. `done` and `wontfix` are closed: they
leave `primum list` and the board's default view, and their ids are removed
from every other item's `needs`.

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

## Needs and blocked items

`needs: [TODO:1, BUG:2]` says an item waits for others. While any of them is
open, the item is blocked: the board marks it and `primum next` skips it.
Closing an item removes its id from every `needs`, so nothing stays blocked
by finished work.

## BOARD.md

Every change rewrites `BOARD.md`, a read-only summary of the project: counts,
then a table per lane. It is for reading on GitHub or in a pull request; do
not edit it.
