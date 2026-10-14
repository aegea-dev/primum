# primum

Prioritized work items as one markdown file per item, for any number of
projects, with a clap command line and a local web board (axum, askama,
plain JavaScript). Every ticket follows a lifecycle (the steps it goes
through, from created to closed), and a lifecycle engine that moves tickets
through them by itself is being built around this ledger. This file is for
contributing to primum.

## Working here

- **Look before you change shared code.** Search the code and tests for
  every caller first, and extend the existing helper instead of writing a
  second one.
- **Test first when fixing a bug.** Write the test, run it, watch it fail.
- **Verify before claiming done.** Run the commands below and read their
  output.
- **Fixtures cover non-ASCII text.** Titles, bodies and paths in tests
  include letters such as `ı`, `ş`, `ğ`, `ü` and `é`, so Unicode handling
  stays tested. Keep them in neutral titles and words, not personal names.
- **Nothing machine-wide.** primum finds everything from the folder it runs
  in: the project above it, and the workspace above that.
- **Every change leaves a project consistent.** A change takes the project
  lock, writes files through a temporary file and a rename, removes closed
  ids from every `needs`, and rewrites `BOARD.md` before it returns.
- **Every `git` call goes through `history`**, with primum's fixed flags
  (no hooks, no signing, no prompts). Merging items is `merge`'s job, never
  git's line merge.
- **Browser JavaScript is ES6+**: `const`/`let`, arrow functions, template
  literals; never ES5 style.
- **Item files have one form.** `item_file::write` writes it and
  `item_file::parse` reads only it; anything else is an error naming the
  file and line, never a silent default.
- **Dependencies** use `~x.y.z` requirements; `=` only for prereleases.
- **One worktree per change.** Never work in the main checkout: start every
  change in its own worktree under `.claude/worktrees/` (ignored by git), on a new
  branch from `origin/main`, and remove it once the PR is merged:

  ```bash
  git fetch && git worktree add .claude/worktrees/my-change -b my-change origin/main
  git worktree remove .claude/worktrees/my-change    # after the merge
  ```

  Each worktree builds into its own `target/`; run every command from inside
  it.
- **Getting a change in.** Stage files by name, never `git add -A`. Open a
  PR against `main`; checks must be green.

## The lifecycle engine (being built)

Three processes in the one binary, around the ledger. These rules hold for
all engine code:

- **The ledger stays simple.** The engine uses the ledger (its library and
  files), never the other way round.
- **The daemon decides nothing.** It stores every event in the project's
  journal (SQLite, local, never synced), starts managers and workers, and
  saves their answers exactly as returned, one transaction per answer.
- **The lifecycle manager is pure.** JSON in, JSON out: no IO, no clock,
  no randomness. The same input always gives the same output.
- **A worker runs one action and exits.** No action ever runs inside the
  daemon.
- **Every event handler copes with the same event twice** (delivery is at
  least once), keyed by the event's id.
- **Secrets never appear** in events, the journal, logs or command lines.
- **Outside the workspace**, primum keeps only what the person asks for:
  the daemon's service registration and secrets in the OS secret store.

## Build and test

```bash
cargo build
cargo test --no-fail-fast
make fmt          # format; use this, not bare `cargo fmt`
make fmt-check    # what CI runs
make lint-check   # clippy with -D warnings, as CI runs it
make e2e          # Playwright against target/debug/primum
cd docs && yarn build   # the documentation site, when docs/ changed
```

## Layout

```
src/model.rs       items, ids, lanes, score, priority order
src/item_file.rs   an item's file: names, front matter
src/store.rs       a project's items on disk: load and every change
src/projects.rs    the workspace (primum.toml) and each project's project.json
src/history.rs     the project's bare git repository; every git call
src/merge.rs       three-way merge of one item, field by field
src/migrate.rs     converting a board to one counter; rewriting references
src/lifecycle/     lifecycle files (JSON): format, expressions, validation,
                   versions, built-in and project actionables
src/engine/        the lifecycle engine: the manager's pure decision
                   (eval: CEL and templates; builtins; manager), the
                   journal (SQLite) and the daemon's pass
src/conflicts.rs   conflicts/<n>.json: what a sync could not decide
src/sync.rs        fetch, renumber, merge, settle, push
src/import.rs      one-time import of a board.py board
src/cli.rs         the command line
src/server.rs      axum routes, JSON API, askama pages
templates/         askama templates (nav.html shared)
static/            CSS and JavaScript, embedded with include_str!
tests/             integration tests; tests/common/ is shared
e2e/               Playwright tests for the board
docs/              the documentation site (Astro + Starlight); see docs/README.md
hooks/             the commit guard (below)
.claude/worktrees/ one worktree per change (ignored)
```

## The commit guard

`hooks/pre-commit` (enable with `make hooks`) refuses a commit of more than
30 files, a path outside the repository, or a file that is not source,
config, docs or fixtures. `PRIMUM_ALLOW_BIG_COMMIT=1` and
`PRIMUM_ALLOW_ANY_FILE=1` override the first and last.
