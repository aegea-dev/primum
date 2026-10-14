# primum

A command-line tool for prioritized work items. Each item is a plain
markdown file, ranked by impact, urgency, risk and effort, for any number of
projects. Everything runs from your terminal; `primum serve` adds an
optional local web board.

Documentation: <https://erhangundogan.github.io/primum/>

## Install

```bash
brew install erhangundogan/tap/primum
```

or with the install script, which verifies the download's checksum:

```bash
curl -fsSL https://raw.githubusercontent.com/erhangundogan/primum/main/install.sh | sh
```

or, with a Rust toolchain:

```bash
cargo install primum --locked
```

## Quick start

```bash
mkdir ~/boards && cd ~/boards
primum workspace init                        # this folder holds your boards
primum init my-app && cd my-app              # a project: project.json and items/
primum add TODO "Export to CSV" --body-file why.md
primum list                                  # open items, highest priority first
primum next                                  # what to work on now
primum serve                                 # the board in your browser
```

## Items are files

Each item is one markdown file in `items/`, named for its id
(`items/TODO-87.md` is `TODO:87`):

```markdown
---
title: "Adoption metrics: daily snapshot job and a Metrics page"
status: doing
impact: 7
urgency: 6
risk: 2
effort: 4
needs: [TODO:64]
---

The item's text, as markdown.
```

Kinds are lanes, in this order: `URGENT`, `BUG`, `DEBT`, `KILLER`, `TODO`.
Statuses are `open`, `doing`, `done` and `wontfix`. Scores run from 1 to 10;
an item's priority is `(impact + urgency + risk) * 10 / effort`, and items
are ordered by lane, then priority. An item whose `needs` name an open item
is blocked.

Edit the files with primum, the board, or any text editor. Every change
rewrites `BOARD.md`, a read-only summary of the whole project.

## Adding items

`primum add KIND TITLE` prints the new item's id. Scores left out default
to impact 5, urgency 3, risk 3, effort 5.

```bash
primum add TODO "Export open items as CSV" --impact 6 --effort 3 --body-file csv.md
primum add BUG "Empty board crashes on + new" --urgency 8 --body-file crash.md
primum add URGENT "Login fails after the 2.3 upgrade" --impact 9 --urgency 10 --risk 8 --effort 2
primum add DEBT "Split server.rs into routes and pages" --effort 6 --needs TODO:12
primum add KILLER "Offline mode" --impact 9 --effort 8 --needs DEBT:3,BUG:7
primum add TODO "Dark mode for print" --body-file - < notes.md
primum add TODO "Weekly digest" -p my-lib
```

| argument | what it sets |
|---|---|
| `--impact`, `--urgency`, `--risk`, `--effort` | scores, 1 to 10 |
| `--needs TODO:1,BUG:2` | items it waits for |
| `--body-file FILE` | its text, from a markdown file; `-` reads standard input |
| `-p NAME` | the project, when not the one you are in |

To add many items, write each text to a file first and run one `add` per
item; nothing has to be quoted on the command line, and each `add` is one
commit in the history.

## Documents beside items

An item's documents sit next to it: `items/TODO-87.spec.md`,
`items/TODO-87.plan.md`, any `items/<ID>.<type>.md`. The board shows them
as tabs beside the item's text. An item can also show another item's
document by listing it: `docs: [items/TODO-80.spec.md]`.

```bash
primum doc TODO:87 spec --file spec.md
```

## Workspaces

primum finds everything from the folder it runs in; nothing lives in
`~/.config`. A workspace is a folder with a `primum.toml` and one subfolder
per project, and each project carries its own settings in `project.json`:

```text
~/boards/
  primum.toml          projects = ["my-app", "my-lib"]   (optional; tab order)
  my-app/
    project.json       {"name": "my-app"}
    items/
  my-lib/
```

`primum init NAME` makes a project in the workspace, `primum projects` lists
them, and `primum projects add FOLDER` and `primum projects remove NAME`
edit the list. `primum serve` shows one tab per project. On the command line,
`-p NAME` picks a project; without it primum uses the project you are in,
else the workspace's only one. A project with no workspace above it works
on its own.

Boards made by earlier versions keep working. To convert them, run
`primum workspace migrate` in the folder that holds them.

## History and sync

Every change is a commit in a git repository primum keeps inside the
project, `.primum/history.git`: the project folder does not need to be a
git repository, and edits made in a text editor are kept too, as their own
commit before the next change.

```bash
primum log TODO:87                             # what happened to an item
primum remote set git@github.com:me/board.git  # sync this project
primum clone git@github.com:me/board.git       # the same board on another machine
```

With a remote, every change syncs by itself, and `primum serve` syncs every
minute. primum merges for you: different items, or different fields of one
item, never ask. Only the same field changed differently on two sides is a
conflict; the board marks it, and you keep one side on the item's page or
with `primum resolve TODO:87`. Each machine sets its own remote, so `ssh`
here and `https` there both work; several projects can share one remote on
different branches (`--branch`).

History needs git 2.28 or later; without git, everything else works.

## License

Apache-2.0
