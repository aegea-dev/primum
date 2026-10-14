---
title: GitHub Issues
description: A project's items and a GitHub repository's issues kept in step, both ways.
---

A project can keep its items and a GitHub repository's issues in step, both
ways, through the [`gh`](https://cli.github.com) CLI. Log in once with
`gh auth login`; primum never stores a token.

```bash
primum github connect owner/repo     # open items become issues, open issues become items
primum github status
primum github disconnect             # stop; links in items stay
```

`connect --all` also files closed items and takes in closed issues, so that
references between old and new items resolve on both sides.

## What is kept in step

| Item | Issue |
|---|---|
| title | title |
| text | body |
| `done` | closed as completed |
| `wontfix` | closed as not planned |
| `doing` | the `doing` label |
| kind | a `kind:bug`, `kind:todo`, ... label |
| scores, needs | a hidden block at the end of the body, and a `Needs:` line |

Every sync carries changes both ways. A field changed on both sides is a
[conflict](/guides/conflicts/), settled like any other. An item linked to an
issue has its number in `github:` in its front matter, and the board links to
it.

Deleting an item closes its issue as not planned.

## Closing keywords

GitHub's keywords work as always: a pull request saying `Fixes #123` closes
the issue when it merges, and the item becomes `done`. primum's ids work too:
`Fixes TODO:87` in a merged pull request closes `TODO:87`.

## Ids on issues

By default an issue shows its item's id only in the hidden block. To show it
in the title as well (`[TODO:87] Export open items as CSV`), set `ids` in
the project's `project.json`:

```json
{
  "name": "my-app",
  "github": { "repo": "owner/repo", "ids": "title" }
}
```

`ids` is `"title"` or `"none"`, the default.

primum never makes a label per item.

## Issues opened on GitHub

An issue opened on GitHub, without primum's hidden block, becomes a `TODO`
with default scores, which you then set on the board.
