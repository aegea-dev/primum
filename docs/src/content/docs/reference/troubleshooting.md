---
title: Troubleshooting
description: Common errors and what to do.
---

## An item file is an error

`primum: TODO-87.md:4: unknown status ...` names the file and line primum could not read. An
edit outside primum left it in a form primum does not write; fix the line to
match [item files](/reference/item-files/). `primum log TODO:87` shows the
last good version.

## "several projects are listed; choose one with -p NAME"

The command ran outside a project, with more than one listed. Pass
`-p NAME`, or `cd` into the project. `primum projects` lists the names.

## History is off

Commands work but `primum log` is empty: git is missing or older than 2.28.
Install a newer git; the next change starts the history.

## Sync failed

`primum sync` prints git's error. Check that this machine can reach the
remote (`git ls-remote URL`), and `primum remote` for the URL and branch.
Changes are kept locally and go out with the next sync.

## GitHub is not syncing

`primum github status` says whether `gh` is installed and logged in. Run
`gh auth status`, and `gh auth login` if needed.

## The board does not open

Another program may hold port 8777: `primum serve --port 9000`.

## Reporting a bug

[Open an issue](https://github.com/erhangundogan/primum/issues) with the
command, its output, and `primum --version`.
