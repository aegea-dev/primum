---
title: Working with AI agents
description: "primum as a work queue for a coding agent: plain commands, files, and JSON."
---

primum suits an agent as well as a person: items are files it can read, and
every command takes plain arguments.

## Reading

```bash
primum next                   # what to do
primum show TODO:87 --json    # one item, with its documents
primum list --json            # everything open, ranked
primum log TODO:87 --json     # what happened to it
```

## Writing

Text goes through files or standard input, never through quoting:

```bash
primum start TODO:87
primum doc TODO:87 plan --file plan.md
primum note TODO:87 --file - <<'END'
Implemented the CSV export; the date column follows the item's `finished`.
END
primum done TODO:87
```

`primum delete` refuses to run without a terminal unless given `--yes`.

## In a project's instructions

A line in a repository's `CLAUDE.md` or `AGENTS.md` is enough:

```markdown
Work items live in ~/boards/my-app (primum). Take the next one with
`primum next -p my-app`, record specs and plans with `primum doc`, and mark
items done with `primum done`.
```

Every change an agent makes is a commit in the project's history, so
`primum log` shows exactly what it did.
