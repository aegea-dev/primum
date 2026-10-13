# primum

Prioritized work items as one markdown file per item, for any number of
projects, with a clap command line and a local web board (axum, askama,
plain JavaScript). This file is for contributing to primum.

## Working here

- **Look before you change shared code.** Search the code and tests for
  every caller first, and extend the existing helper instead of writing a
  second one.
- **Test first when fixing a bug.** Write the test, run it, watch it fail.
- **Verify before claiming done.** Run the commands below and read their
  output.
- **Fixtures** use Turkish names (`Özgür`, `Işıl`, `Gökçe`), never `Alice`
  and `Bob`.
- **Every change leaves a project consistent.** A change takes the project
  lock, writes files through a temporary file and a rename, removes closed
  ids from every `needs`, and rewrites `BOARD.md` before it returns.
- **Every `git` call goes through `history`**, with primum's fixed flags
  (no hooks, no signing, no prompts). Merging items is `merge`'s job, never
  git's line merge.
- **Every GitHub call goes through `github::Api`**; tests use
  `github::Memory`, and `tests/fixtures/fake-gh` stands in for `gh`.
- **Browser JavaScript is ES6+**: `const`/`let`, arrow functions, template
  literals; never ES5 style.
- **Item files have one form.** `item_file::write` writes it and
  `item_file::parse` reads only it; anything else is an error naming the
  file and line, never a silent default.
- **Dependencies** use `~x.y.z` requirements; `=` only for prereleases.
- **Getting a change in.** Stage files by name, never `git add -A`. Open a
  PR against `main`; checks must be green.

## Build and test

```bash
cargo build
cargo test --no-fail-fast
make fmt          # format; use this, not bare `cargo fmt`
make fmt-check    # what CI runs
make lint-check   # clippy with -D warnings, as CI runs it
make e2e          # Playwright against target/debug/primum
```

## Layout

```
src/model.rs       items, lanes, score, priority order
src/item_file.rs   an item's file: names, front matter
src/store.rs       a project's items on disk: load and every change
src/projects.rs    primum.toml and the machine's projects.toml
src/history.rs     the project's bare git repository; every git call
src/merge.rs       three-way merge of one item, field by field
src/conflicts.rs   conflicts/<STEM>.json: what a sync could not decide
src/sync.rs        fetch, renumber, merge, settle, push
src/github.rs      GitHub issues: the Api trait, gh, an in-memory GitHub, the mapping
src/github_sync.rs one GitHub sync: issues and items in step
src/import.rs      one-time import of a board.py board
src/cli.rs         the command line
src/server.rs      axum routes, JSON API, askama pages
templates/         askama templates (nav.html shared)
static/            CSS and JavaScript, embedded with include_str!
tests/             integration tests; tests/common/ is shared
e2e/               Playwright tests for the board
```

## The commit guard

`hooks/pre-commit` (enable with `make hooks`) refuses a commit of more than
30 files, a path outside the repository, or a file that is not source,
config, docs or fixtures. `PRIMUM_ALLOW_BIG_COMMIT=1` and
`PRIMUM_ALLOW_ANY_FILE=1` override the first and last.
