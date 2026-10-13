# primum

Prioritized work items for any number of projects, kept as plain markdown
files, with a command line and a local web board.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/erhangundogan/primum/main/docs/board-dark.png">
  <img alt="The primum board: items in priority order, lanes as filters, next up above the table" src="https://raw.githubusercontent.com/erhangundogan/primum/main/docs/board-light.png">
</picture>

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
cd ~/projects/my-app
primum init                                  # primum.toml and an empty items/
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

## Adding many items at once

Write each item's text as a markdown file in a scratch folder, then add
them with one `primum add` each. `--body-file` reads the text from a file
(`-` reads standard input), so nothing has to be quoted on the command line.

```bash
dir=$(mktemp -d)

cat > "$dir/csv.md" <<'EOF'
## Why

Işıl wants the open items as CSV for the weekly review.
EOF

cat > "$dir/crash.md" <<'EOF'
## Steps

1. Open the board with an empty `items/` folder.
2. Click "+ new".
EOF

primum add TODO "Export open items as CSV" --impact 6 --effort 3 --body-file "$dir/csv.md"
primum add BUG "Empty board crashes on + new" --urgency 8 --body-file "$dir/crash.md"
rm -r "$dir"
```

Each `add` takes the next number in its lane, writes `items/<ID>.md`,
rewrites `BOARD.md` and is one commit in the history. Specs and plans go
beside an item the same way: `primum doc TODO:7 spec --file "$dir/spec.md"`.

## Documents beside items

An item's documents sit next to it: `items/TODO-87.spec.md`,
`items/TODO-87.plan.md`, any `items/<ID>.<type>.md`. The board shows them
as tabs beside the item's text. An item can also show another item's
document by listing it: `docs: [items/TODO-80.spec.md]`.

```bash
primum doc TODO:87 spec --file spec.md
```

## Many projects

Each project describes itself in its `primum.toml`. Your machine's list of
projects, in tab order, is `~/.config/primum/projects.toml`:

```toml
projects = ["~/projects/my-app", "~/projects/my-lib"]
```

`primum projects` lists them, `primum projects add PATH` and
`primum projects remove NAME` edit the list. The board shows one tab per
project. On the command line, `-p NAME` picks a project; without it primum
uses the project you are in, else the only one listed.

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

## GitHub Issues (optional)

A project can keep its items and a GitHub repository's issues in step, both
ways, through the [`gh`](https://cli.github.com) CLI (log in once with
`gh auth login`; primum never stores a token):

```bash
primum github connect owner/repo     # open items become issues, open issues become items
primum github status
primum github disconnect             # stop; links in items stay
```

From then on every sync carries changes both ways: title, text, status
(`done` is closed as completed, `wontfix` closed as not planned, `doing` a
label), kind (a `kind:bug` label) and needs. Scores and needs live in a
hidden block at the end of the issue body. A field changed on both sides is
a conflict, settled like any other.

GitHub's closing keywords work as always: a pull request saying
`Fixes #123` closes the issue when it merges, and the item becomes `done`.
primum's ids work too: `Fixes TODO:87` in a merged pull request closes
TODO:87. Deleting an item closes its issue as not planned.

## License

Apache-2.0
