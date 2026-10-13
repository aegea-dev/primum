# History and sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every change to a primum project is committed to a bare git repository inside it, and projects with a remote sync automatically, with primum merging items field by field and only true conflicts reaching a person.

**Architecture:** A new `history` module owns every `git` call (shelling out). `store` wraps each change in `change()`: lock, commit outside edits, change, commit. `merge` is a pure three-way merge of one item; `conflicts` reads and writes `conflicts/<STEM>.json`; `sync` fetches, renumbers colliding new items, merges with `merge`, settles and pushes. The CLI and server gain `log`, `remote`, `clone`, `sync`, `resolve`, a History tab and a conflict panel.

**Tech Stack:** Rust 1.96.0, the existing crates (no new dependency), the `git` binary (2.28 or later, for `init -b`), Playwright.

**Spec:** `items/KILLER-2.spec.md`

## Global Constraints

- The history is `<project>/.primum/history.git` (bare); the work tree is the project folder; `.primum/.gitignore` holds `*`.
- `info/exclude` is exactly the allowlist in the spec (`/*`, `!/primum.toml`, `!/BOARD.md`, `!/items/`, `/items/*`, `!/items/*.md`, `!/conflicts/`).
- Every git call: `--git-dir <dir> --work-tree <root> -c core.hooksPath=/dev/null -c commit.gpgsign=false -c core.quotePath=false -c core.autocrlf=false`, stdin null, `GIT_TERMINAL_PROMPT=0`. Author: the configured identity, else `-c user.name=primum -c user.email=primum@localhost`.
- `PRIMUM_GIT` names the git binary (default `git`). Without git, every command works and history is off.
- Network calls (fetch, push) time out after 10 seconds; clone after 60. `GIT_SSH_COMMAND` defaults to `ssh -o BatchMode=yes -o ConnectTimeout=10` when unset.
- Remote settings live in the history repo's git config: `remote.origin.url`, `primum.branch` (default `main`), `primum.sync` (`auto` default, or `manual`). Never in `primum.toml`.
- Commit messages exactly as the spec's table.
- primum's rules still hold: no em dash in anything public; stage files by name; fixtures in Turkish (`Özgür`, `Işıl`, `Gökçe`); `make fmt-check` and `make lint-check` clean before each commit; no AI attribution in commits.

## File Structure

```
src/history.rs     the bare repo: create, commit, outside edits, log, diff, remote, fetch, push, clone, stages, merge-file
src/merge.rs       merge_item: three-way merge of one item (pure)
src/conflicts.rs   Conflict, read/write/rename/remove conflicts/<STEM>.json
src/sync.rs        sync(): fetch, renumber, merge, settle, push, retry
src/store.rs       change(), commit messages, move_locked(), resolve(), conflicts on Board
src/cli.rs         log, remote, clone, sync, resolve; auto-sync after changes
src/server.rs      /api/history, /api/history/diff, /api/resolve; serve's sync loop
static/item.js     History tab, conflict panel
static/board.js    polling, conflict marker
tests/history.rs  tests/merge.rs  tests/sync.rs  (+ additions to tests/{store,cli,server,common}.rs)
e2e/tests/history.spec.ts
```

---

### Task 1: The history repository (`history`)

**Files:** Create `src/history.rs`, `tests/history.rs`. Modify `src/lib.rs` (add `pub mod history;`), `tests/common/mod.rs`.

**Interfaces:**
- Produces: `git_binary() -> String`, `git_available() -> bool`, `History { git_dir: PathBuf, work_tree: PathBuf }`, `History::dir(&Project) -> PathBuf`, `History::open(&Project) -> Result<Option<History>>`, `History::open_with(&Project, first_message: &str) -> Result<Option<History>>`, `History::git(&self, &[&str]) -> Result<String>`, `History::run(&self, &[&str]) -> Result<std::process::Output>` (does not check the exit code), `History::commit(&self, message) -> Result<bool>`, `History::record_outside(&self) -> Result<bool>`, `History::head(&self) -> Result<Option<String>>`.

- [ ] **Step 1: Test helpers** in `tests/common/mod.rs` (append):

```rust
use primum::history::History;

/// A project's history, which must exist.
pub fn history(p: &Project) -> History {
    History::open(p).unwrap().expect("git is installed")
}

/// Commit subjects, newest first.
pub fn subjects(p: &Project) -> Vec<String> {
    history(p)
        .git(&["log", "--format=%s"])
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

pub fn tracked(p: &Project) -> Vec<String> {
    let mut v: Vec<String> = history(p)
        .git(&["ls-files"])
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    v.sort();
    v
}
```

- [ ] **Step 2: Failing tests** `tests/history.rs`:

```rust
mod common;
use primum::history::History;
use std::fs;

#[test]
fn open_creates_the_bare_repo_with_a_first_commit_of_what_is_there() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    fs::write(d.path().join("primum.toml"), "name = \"deneme\"\n").unwrap();
    fs::write(d.path().join("notlar.txt"), "Gökçe'nin notu\n").unwrap();
    fs::write(d.path().join(".primum.lock"), "").unwrap();
    fs::write(d.path().join("items/TODO-1.md.tmp"), "").unwrap();
    fs::write(d.path().join("items/TODO-1.md"), "---\ntitle: x\n---\n").unwrap();
    let h = History::open(&p).unwrap().unwrap();
    assert!(d.path().join(".primum/history.git/HEAD").is_file());
    assert_eq!(fs::read_to_string(d.path().join(".primum/.gitignore")).unwrap(), "*\n");
    assert_eq!(common::subjects(&p), ["start history"]);
    assert_eq!(common::tracked(&p), ["items/TODO-1.md", "primum.toml"]);
    assert!(h.head().unwrap().is_some());
    // opening again changes nothing
    History::open(&p).unwrap().unwrap();
    assert_eq!(common::subjects(&p).len(), 1);
}

#[test]
fn commit_takes_only_primums_files_and_reports_whether_it_committed() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let h = History::open(&p).unwrap().unwrap();
    assert!(!h.commit("boş").unwrap(), "nothing to commit");
    fs::write(d.path().join("items/BUG-1.md"), "x").unwrap();
    fs::write(d.path().join("BOARD.md"), "# deneme\n").unwrap();
    fs::create_dir_all(d.path().join("conflicts")).unwrap();
    fs::write(d.path().join("conflicts/BUG-1.json"), "{}").unwrap();
    fs::write(d.path().join("başka.md"), "x").unwrap();
    assert!(h.commit("BUG:1: add \"Işıl\"").unwrap());
    assert_eq!(common::subjects(&p)[0], "BUG:1: add \"Işıl\"");
    assert_eq!(common::tracked(&p), ["BOARD.md", "conflicts/BUG-1.json", "items/BUG-1.md"]);
    fs::remove_file(d.path().join("items/BUG-1.md")).unwrap();
    assert!(h.commit("BUG:1: delete").unwrap());
    assert_eq!(common::tracked(&p), ["BOARD.md", "conflicts/BUG-1.json"]);
}

#[test]
fn outside_edits_are_committed_apart() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let h = History::open(&p).unwrap().unwrap();
    assert!(!h.record_outside().unwrap());
    fs::write(d.path().join("items/TODO-1.md"), "elle yazıldı").unwrap();
    assert!(h.record_outside().unwrap());
    assert_eq!(common::subjects(&p)[0], "edited outside primum");
}

#[test]
fn author_is_the_configured_identity_else_primum() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let h = History::open(&p).unwrap().unwrap();
    h.git(&["config", "user.name", "Özgür"]).unwrap();
    h.git(&["config", "user.email", "ozgur@example.com"]).unwrap();
    fs::write(d.path().join("items/TODO-1.md"), "x").unwrap();
    History::open(&p).unwrap().unwrap().commit("x").unwrap();
    assert_eq!(h.git(&["log", "-1", "--format=%an"]).unwrap().trim(), "Özgür");
}
```

- [ ] **Step 3:** `cargo test --test history`: compile failure, no `history` module.

- [ ] **Step 4: `src/history.rs`**

```rust
//! A project's history: a bare git repository at `.primum/history.git`
//! whose work tree is the project folder. Every `git` call primum makes is
//! in this module.
use crate::store::Project;
use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// What the history tracks: primum's files and nothing else.
const EXCLUDE: &str = "/*\n!/primum.toml\n!/BOARD.md\n!/items/\n/items/*\n!/items/*.md\n!/conflicts/\n";

pub fn git_binary() -> String {
    std::env::var("PRIMUM_GIT").unwrap_or_else(|_| "git".into())
}

pub fn git_available() -> bool {
    Command::new(git_binary())
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct History {
    pub git_dir: PathBuf,
    pub work_tree: PathBuf,
}

fn check(out: Output, what: &str) -> Result<String> {
    if !out.status.success() {
        bail!(
            "git {what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

impl History {
    pub fn dir(p: &Project) -> PathBuf {
        p.root.join(".primum").join("history.git")
    }

    /// The project's history, created when missing with a first commit
    /// (`start history`) of what is already there. None when git is not
    /// installed.
    pub fn open(p: &Project) -> Result<Option<History>> {
        History::open_with(p, "start history")
    }

    pub fn open_with(p: &Project, first_message: &str) -> Result<Option<History>> {
        if !git_available() {
            return Ok(None);
        }
        let h = History {
            git_dir: History::dir(p),
            work_tree: p.root.clone(),
        };
        if !h.git_dir.join("HEAD").exists() {
            h.create()?;
            h.commit(first_message)?;
        }
        Ok(Some(h))
    }

    fn create(&self) -> Result<()> {
        std::fs::create_dir_all(&self.git_dir)?;
        std::fs::write(self.work_tree.join(".primum/.gitignore"), "*\n")?;
        let out = Command::new(git_binary())
            .args(["init", "--quiet", "--bare", "-b", "main"])
            .arg(&self.git_dir)
            .stdin(Stdio::null())
            .output()
            .context("cannot run git")?;
        check(out, "init")?;
        self.write_exclude()
    }

    pub(crate) fn write_exclude(&self) -> Result<()> {
        std::fs::create_dir_all(self.git_dir.join("info"))?;
        std::fs::write(self.git_dir.join("info/exclude"), EXCLUDE)?;
        Ok(())
    }

    /// A git command on this history, with primum's fixed settings.
    pub fn command(&self) -> Command {
        let mut c = Command::new(git_binary());
        c.arg("--git-dir")
            .arg(&self.git_dir)
            .arg("--work-tree")
            .arg(&self.work_tree)
            .args([
                "-c", "core.hooksPath=/dev/null",
                "-c", "commit.gpgsign=false",
                "-c", "core.quotePath=false",
                "-c", "core.autocrlf=false",
            ])
            .current_dir(&self.work_tree)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null());
        if !self.has_identity() {
            c.args(["-c", "user.name=primum", "-c", "user.email=primum@localhost"]);
        }
        c
    }

    fn has_identity(&self) -> bool {
        Command::new(git_binary())
            .arg("--git-dir")
            .arg(&self.git_dir)
            .args(["config", "user.email"])
            .stdin(Stdio::null())
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false)
    }

    /// Runs git; any exit code is returned to the caller.
    pub fn run(&self, args: &[&str]) -> Result<Output> {
        self.command().args(args).output().context("cannot run git")
    }

    /// Runs git and returns its output; a failure is an error.
    pub fn git(&self, args: &[&str]) -> Result<String> {
        check(self.run(args)?, args.first().copied().unwrap_or(""))
    }

    pub fn head(&self) -> Result<Option<String>> {
        let out = self.run(&["rev-parse", "--verify", "--quiet", "HEAD"])?;
        Ok(out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()))
    }

    /// Commits everything primum tracks that changed. Returns false when
    /// there was nothing to commit.
    pub fn commit(&self, message: &str) -> Result<bool> {
        self.git(&["add", "--all", "--", "."])?;
        if self.run(&["diff", "--cached", "--quiet"])?.status.success() {
            return Ok(false);
        }
        self.git(&["commit", "--quiet", "--no-verify", "-m", message])?;
        Ok(true)
    }

    /// Commits edits made outside primum since the last commit, apart from
    /// the change that follows.
    pub fn record_outside(&self) -> Result<bool> {
        self.commit("edited outside primum")
    }
}
```

- [ ] **Step 5:** `cargo test --test history`: 4 passed. `make fmt && make lint-check`.
- [ ] **Step 6:** commit `src/history.rs src/lib.rs tests/history.rs tests/common/mod.rs`: "History: a bare git repository behind each project".

---

### Task 2: Every change is one commit (`store`, `projects`, `import`, `cli`)

**Files:** Modify `src/store.rs`, `src/projects.rs`, `src/import.rs`, `src/cli.rs`, `tests/store.rs`, `tests/cli.rs`.

**Interfaces:**
- Consumes: `History::open`, `History::open_with`, `History::commit`, `History::record_outside`.
- Produces: `store::lock(&Project) -> Result<File>` becomes `pub(crate)`; `store::settle_locked(&Project) -> Result<()>` (`pub(crate)`, no lock); `Changes::fields(&self) -> Vec<&'static str>` (pub; the CLI's `changed_fields` is removed in favour of it); `store::move_locked(p, id, kind, num) -> Result<String>` (`pub(crate)`, no lock, no closed check, used by Task 7).

- [ ] **Step 1: Failing tests** (append to `tests/store.rs`):

```rust
#[test]
fn every_change_is_one_commit_with_its_message() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Özgür'ün listesi", ch()).unwrap();
    update(&p, "TODO:1", Changes { status: Some("doing".into()), ..ch() }).unwrap();
    update(&p, "TODO:1", Changes { impact: Some(7), effort: Some(3), ..ch() }).unwrap();
    append_note(&p, "TODO:1", "Ölçüldü.").unwrap();
    set_doc(&p, "TODO:1", "spec", "# S\n").unwrap();
    set_kind(&p, "TODO:1", "BUG").unwrap();
    delete(&p, "BUG:1").unwrap();
    assert_eq!(
        common::subjects(&p),
        [
            "BUG:1: delete",
            "TODO:1 -> BUG:1",
            "TODO:1: spec document",
            "TODO:1: note",
            "TODO:1: impact, effort",
            "TODO:1: status doing",
            "TODO:1: add \"Özgür'ün listesi\"",
            "start history",
        ]
    );
}

#[test]
fn an_outside_edit_is_committed_before_the_next_change() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "x", ch()).unwrap();
    let f = d.path().join("items/TODO-1.md");
    std::fs::write(&f, std::fs::read_to_string(&f).unwrap().replace("risk: 3", "risk: 9")).unwrap();
    update(&p, "TODO:1", Changes { status: Some("done".into()), ..ch() }).unwrap();
    assert_eq!(common::subjects(&p)[..2], ["TODO:1: status done", "edited outside primum"]);
}

#[test]
fn a_failed_change_commits_nothing() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "x", ch()).unwrap();
    assert!(update(&p, "TODO:9", ch()).is_err());
    assert_eq!(common::subjects(&p).len(), 2);
}
```

and in `tests/cli.rs`, `T::run_in` passes `PATH` so `git` is found: add `.env("PATH", std::env::var("PATH").unwrap())` after `.env_clear()`, then add:

```rust
#[test]
fn history_is_kept_and_off_without_git() {
    let t = T::new();
    t.out(&["add", "TODO", "Işıl"]);
    assert!(t.dir.path().join("işler/.primum/history.git/HEAD").is_file());
    let o = Command::new(env!("CARGO_BIN_EXE_primum"))
        .args(["add", "BUG", "gitsiz"])
        .current_dir(t.dir.path().join("işler"))
        .env_clear()
        .env("HOME", t.dir.path())
        .env("XDG_CONFIG_HOME", t.dir.path().join("config"))
        .env("PRIMUM_GIT", "/yok/git")
        .output()
        .unwrap();
    assert!(o.status.success());
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "BUG:1");
    assert_eq!(
        String::from_utf8_lossy(&o.stderr).trim(),
        "primum: history is off: git not found"
    );
}
```

and in `tests/projects.rs`, `init_writes_the_project_and_lists_it` also asserts:

```rust
        let h = primum::history::History::open(&p).unwrap().unwrap();
        assert_eq!(h.git(&["log", "--format=%s"]).unwrap().trim(), "primum project videre");
```

- [ ] **Step 2:** run, expect the new tests to fail.

- [ ] **Step 3: `store.rs`.** Add, near `lock`:

```rust
use crate::history::History;

/// Runs one change under the project lock: edits made outside primum are
/// committed first, then `f` changes the board and names the change, then
/// one commit records it. Without git, the change still happens.
fn change<T>(p: &Project, f: impl FnOnce(&mut Board) -> Result<(T, String)>) -> Result<T> {
    let _l = lock(p)?;
    let history = History::open(p)?;
    if let Some(h) = &history {
        h.record_outside()?;
    }
    let mut board = load(p)?;
    let (out, message) = f(&mut board)?;
    if let Some(h) = &history {
        h.commit(&message)?;
    }
    Ok(out)
}

/// `settle_all` without taking the lock, for callers that hold it.
pub(crate) fn settle_locked(p: &Project) -> Result<()> {
    let mut board = load(p)?;
    settle(p, &mut board)
}
```

Make `lock` `pub(crate)`. Add to `impl Changes`:

```rust
impl Changes {
    /// The fields this change sets, in a fixed order.
    pub fn fields(&self) -> Vec<&'static str> {
        [
            ("title", self.title.is_some()),
            ("status", self.status.is_some()),
            ("impact", self.impact.is_some()),
            ("urgency", self.urgency.is_some()),
            ("risk", self.risk.is_some()),
            ("effort", self.effort.is_some()),
            ("needs", self.needs.is_some()),
            ("body", self.body.is_some()),
        ]
        .into_iter()
        .filter(|(_, set)| *set)
        .map(|(name, _)| name)
        .collect()
    }

    /// The commit message for this change to `id`.
    fn message(&self, id: &str) -> String {
        match (&self.status, self.fields().as_slice()) {
            (Some(s), ["status"]) => format!("{id}: status {s}"),
            (_, fields) => format!("{id}: {}", fields.join(", ")),
        }
    }
}
```

Rewrite each public change on `change` (bodies otherwise as today):

```rust
pub fn insert(p: &Project, kind: &str, title: &str, c: Changes) -> Result<String> {
    if !KINDS.contains(&kind) {
        bail!("unknown kind {kind}");
    }
    change(p, |board| {
        let title = if title.trim().is_empty() { "untitled" } else { title };
        let mut it = Item::new(kind, board.next_num(kind), title);
        it.source = "primum".into();
        apply(&mut it, c)?;
        save(p, &it)?;
        let id = it.id.clone();
        let message = format!("{id}: add {}", serde_json::to_string(&it.title)?);
        board.items.push(it);
        settle(p, board)?;
        Ok((id, message))
    })
}

pub fn update(p: &Project, id: &str, c: Changes) -> Result<()> {
    let message = c.message(id);
    change(p, |board| Ok((update_locked(p, board, id, c)?, message)))
}

pub fn append_note(p: &Project, id: &str, text: &str) -> Result<()> {
    change(p, |board| {
        let body = board.get_mut(id)?.body.trim_end().to_string();
        let note = format!("## {}\n\n{}\n", today(), text.trim());
        let body = if body.is_empty() { note } else { format!("{body}\n\n---\n\n{note}") };
        let c = Changes { body: Some(body), ..Default::default() };
        update_locked(p, board, id, c)?;
        Ok(((), format!("{id}: note")))
    })
}
```

Split `set_kind`: everything after the closed check moves to `move_locked`, which takes the target number:

```rust
/// Moves an item to `kind` and `num` without the lock: its file and own
/// documents renamed, its heading, its conflict file, and every `needs`
/// and `docs` naming it rewritten. Shared by `set_kind` and sync's
/// renumbering. Returns the new id.
pub(crate) fn move_locked(p: &Project, id: &str, kind: &str, num: u32) -> Result<String> {
    let board = load(p)?;
    let old = board.get(id).ok_or_else(|| anyhow!("no item {id}"))?.clone();
    let new_id = id_of(kind, num);
    let (old_stem, new_stem) = (stem(id), stem(&new_id));
    let mut moved = old.clone();
    moved.id = new_id.clone();
    moved.kind = kind.to_string();
    moved.num = num;
    moved.body = rewrite_heading(&old.body, id, &new_id);
    save(p, &moved)?;
    fs::remove_file(p.items_dir().join(file_name(id)))?;
    for d in board.own_docs(id) {
        let to = d.name.replacen(&old_stem, &new_stem, 1);
        fs::rename(p.items_dir().join(&d.name), p.items_dir().join(to))?;
    }
    crate::conflicts::rename(p, id, &new_id)?;   // added in Task 5; until then omit this line
    /* the loop rewriting other items' needs and docs, unchanged from set_kind */
    settle_locked(p)?;
    Ok(new_id)
}

pub fn set_kind(p: &Project, id: &str, kind: &str) -> Result<String> {
    if !KINDS.contains(&kind) {
        bail!("unknown kind {kind}");
    }
    change(p, |board| {
        let old = board.get(id).ok_or_else(|| anyhow!("no item {id}"))?.clone();
        if old.is_closed() {
            bail!("{id} is closed; reopen it before changing kind");
        }
        if old.kind == kind {
            return Ok((id.to_string(), String::new()));
        }
        let new_id = move_locked(p, id, kind, board.next_num(kind))?;
        Ok((new_id.clone(), format!("{id} -> {new_id}")))
    })
}
```

(An empty message commits nothing because nothing changed; `History::commit` returns false.)

`delete` returns `(gone, format!("{id}: delete"))` from inside `change`; `set_doc` returns `(path, format!("{id}: {doc_type} document"))`.

- [ ] **Step 4: `projects::init`** after writing `primum.toml` and before `add(dir)`:

```rust
    let p = open(dir).map_err(|e| anyhow!(e))?;
    crate::history::History::open_with(&p, &format!("primum project {}", p.name))?;
```

**`import::run`**: replace the final `store::settle_all(project)?;` with

```rust
    store::settle_all(project)?;
    if let Some(h) = crate::history::History::open(project)? {
        h.commit(&format!("import {} items from {}", report.items, board_dir.display()))?;
    }
```

- [ ] **Step 5: `cli.rs`.** Replace `changed_fields(&c)` with `c.fields()` and delete `changed_fields`. In `dispatch`, after a successful change command (`Add`, `Set`, `Start`, `Done`, `Note`, `Doc`, `Kind`, `Delete`, `Import`), call `history_notice()`:

```rust
/// Says once per command that history is off, when git is missing.
fn history_notice() {
    if !crate::history::git_available() {
        eprintln!("primum: history is off: git not found");
    }
}
```

Implement by setting `let changed = matches!(cli.cmd, Cmd::Add { .. } | Cmd::Set { .. } | ...)` before the `match` and calling `history_notice()` after it when `changed`.

- [ ] **Step 6:** `cargo test` (all): passes. `make fmt && make lint-check`.
- [ ] **Step 7:** commit `src/store.rs src/projects.rs src/import.rs src/cli.rs tests/store.rs tests/cli.rs tests/projects.rs`: "Every change is one commit".

---

### Task 3: Reading history (`log`, History tab)

**Files:** Modify `src/history.rs`, `src/cli.rs`, `src/server.rs`, `static/item.js`, `static/primum.css`, `templates/item.html`, `tests/history.rs`, `tests/cli.rs`, `tests/server.rs`.

**Interfaces:**
- Produces: `history::Entry { commit: String, short: String, author: String, date: String, message: String, path: Option<String> }` (Serialize); `History::log(&self, path: Option<&str>, limit: usize) -> Result<Vec<Entry>>`; `History::diff(&self, commit: &str, path: &str) -> Result<String>`; routes `GET /p/{project}/api/history?id=ID` (`[Entry]`, `[]` without git) and `GET /p/{project}/api/history/diff?commit=C&path=P` (text/plain); CLI `primum log [ID] [-n N] [--json]`.

- [ ] **Step 1: Failing tests.** `tests/history.rs`:

```rust
#[test]
fn log_follows_an_item_across_a_kind_change() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    use primum::store::*;
    insert(&p, "TODO", "Gökçe", Changes::default()).unwrap();
    insert(&p, "BUG", "başka", Changes::default()).unwrap();
    update(&p, "TODO:1", Changes { risk: Some(8), ..Default::default() }).unwrap();
    set_kind(&p, "TODO:1", "BUG").unwrap();
    let h = common::history(&p);
    let log = h.log(Some("items/BUG-2.md"), 50).unwrap();
    let msgs: Vec<&str> = log.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(msgs, ["TODO:1 -> BUG:2", "TODO:1: risk", "TODO:1: add \"Gökçe\""]);
    assert_eq!(log[2].path.as_deref(), Some("items/TODO-1.md"));
    let diff = h.diff(&log[1].commit, "items/TODO-1.md").unwrap();
    assert!(diff.contains("-risk: 3") && diff.contains("+risk: 8"), "{diff}");
    assert_eq!(h.log(None, 2).unwrap().len(), 2);
    assert!(h.diff("../x", "items/TODO-1.md").is_err());
    assert!(h.diff(&log[1].commit, "../primum.toml").is_err());
}
```

`tests/cli.rs`:

```rust
#[test]
fn log_lists_an_items_changes() {
    let t = T::new();
    t.out(&["add", "TODO", "Özgür"]);
    t.out(&["start", "TODO:1"]);
    let out = t.out(&["log", "TODO:1"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].ends_with("TODO:1: status doing"), "{out}");
    let json: serde_json::Value = serde_json::from_str(&t.out(&["log", "--json"])).unwrap();
    assert_eq!(json[0]["message"], "TODO:1: status doing");
}
```

`tests/server.rs`:

```rust
#[tokio::test]
async fn history_and_its_diffs() {
    let d = tempfile::tempdir().unwrap();
    let (_, app) = setup(d.path());
    post(&app, &format!("{API}/items"), json!({"id": "BUG:1", "effort": 2})).await;
    let (code, text) = get(&app, &format!("{API}/history?id=BUG:1")).await;
    assert_eq!(code, StatusCode::OK);
    let log: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(log[0]["message"], "BUG:1: effort");
    let c = log[0]["commit"].as_str().unwrap();
    let (code, diff) = get(&app, &format!("{API}/history/diff?commit={c}&path=items/BUG-1.md")).await;
    assert_eq!(code, StatusCode::OK);
    assert!(diff.contains("+effort: 2"));
    let (code, _) = get(&app, &format!("{API}/history/diff?commit={c}&path=Cargo.toml")).await;
    assert_eq!(code, StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2:** run, expect failures.

- [ ] **Step 3: `history.rs`** additions:

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Entry {
    pub commit: String,
    pub short: String,
    pub author: String,
    pub date: String,
    pub message: String,
    /// The item's file at that commit, when the log is of one item.
    pub path: Option<String>,
}

/// A path the history API may name: primum.toml, BOARD.md, or a file
/// directly in items/ or conflicts/.
fn tracked_path(path: &str) -> bool {
    let ok_dir = |d: &str| {
        path.strip_prefix(d)
            .is_some_and(|n| !n.is_empty() && !n.contains('/') && !n.contains(".."))
    };
    path == "primum.toml" || path == "BOARD.md" || ok_dir("items/") || ok_dir("conflicts/")
}

impl History {
    /// Commits newest first: all of them, or those touching `path`,
    /// following renames.
    pub fn log(&self, path: Option<&str>, limit: usize) -> Result<Vec<Entry>> {
        if self.head()?.is_none() {
            return Ok(vec![]);
        }
        let n = format!("-n{limit}");
        let mut args = vec!["log", &n, "--format=%x1e%H%x1f%h%x1f%an%x1f%aI%x1f%s"];
        if let Some(p) = path {
            if !tracked_path(p) {
                bail!("{p} is not a file primum tracks");
            }
            args.extend(["--name-only", "--follow", "--", p]);
        }
        let out = self.git(&args)?;
        Ok(out
            .split('\x1e')
            .filter(|r| !r.trim().is_empty())
            .filter_map(|r| {
                let mut lines = r.lines();
                let f: Vec<&str> = lines.next()?.split('\x1f').collect();
                let path = lines.map(str::trim).find(|l| !l.is_empty()).map(str::to_string);
                Some(Entry {
                    commit: f.first()?.to_string(),
                    short: f.get(1)?.to_string(),
                    author: f.get(2)?.to_string(),
                    date: f.get(3)?.to_string(),
                    message: f.get(4)?.to_string(),
                    path,
                })
            })
            .collect())
    }

    /// What `commit` changed in `path` (the path as it was at that commit).
    pub fn diff(&self, commit: &str, path: &str) -> Result<String> {
        if commit.len() < 7 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("{commit} is not a commit");
        }
        if !tracked_path(path) {
            bail!("{path} is not a file primum tracks");
        }
        self.git(&["show", "--format=", "--find-renames", commit, "--", path])
    }
}
```

- [ ] **Step 4: CLI.** `Cmd::Log { id: Option<String>, #[arg(short, default_value_t = 20)] n: usize, #[arg(long)] json: bool }`, documented "Changes to the project, or to one item". Implementation:

```rust
Cmd::Log { id, n, json } => {
    let p = project()?;
    let Some(h) = crate::history::History::open(&p)? else {
        bail!("history is off: git not found");
    };
    let path = id
        .as_deref()
        .map(|i| norm_id(i).map(|i| format!("items/{}", crate::item_file::file_name(&i))))
        .transpose()?;
    let log = h.log(path.as_deref(), n)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&log)?);
    } else {
        for e in &log {
            let date = e.date.get(..16).unwrap_or(&e.date).replace('T', " ");
            println!("{}  {date}  {:<12}  {}", e.short, truncate(&e.author, 12), e.message);
        }
    }
}
```

- [ ] **Step 5: server routes.**

```rust
.route("/p/{project}/api/history", get(api_history))
.route("/p/{project}/api/history/diff", get(api_history_diff))

async fn api_history(
    State(s): State<Shared>,
    Path(project): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<axum::Json<Vec<crate::history::Entry>>, ApiError> {
    let p = s.project(&project)?;
    let Some(h) = crate::history::History::open(p)? else {
        return Ok(axum::Json(vec![]));
    };
    let path = match q.get("id") {
        Some(id) => {
            let ids = crate::cli::parse_needs(id).map_err(|e| bad(e.to_string()))?;
            let [id] = ids.as_slice() else {
                return Err(bad("name one item id"));
            };
            Some(format!("items/{}", file_name(id)))
        }
        None => None,
    };
    Ok(axum::Json(h.log(path.as_deref(), 200)?))
}

async fn api_history_diff(
    State(s): State<Shared>,
    Path(project): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let p = s.project(&project)?;
    let h = crate::history::History::open(p)?.ok_or_else(|| bad("history is off"))?;
    let get = |k: &str| q.get(k).map(String::as_str).unwrap_or("");
    let diff = h.diff(get("commit"), get("path"))?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], diff).into_response())
}
```

- [ ] **Step 6: History tab** (`static/item.js`). After `drawDocs()` builds the document tabs, append a `history` tab when the item has history. `load()` also fetches `api('/history?id='+encodeURIComponent(ITEM_ID))` into `HISTORY`. The docpane shows when there are documents or history. `showHistory()` renders:

```js
function showHistory(){
  document.querySelectorAll('#doctabs button').forEach(b=>b.classList.toggle('on',b.dataset.doc==='#history'));
  $('#docpath').textContent=IT.file;
  $('#doc').innerHTML='<ol class=history>'+HISTORY.map(e=>
    `<li><details data-commit="${e.commit}" data-path="${esc(e.path||IT.file)}"><summary><span class=when>${esc(e.date.slice(0,16).replace('T',' '))}</span> <span class=who>${esc(e.author)}</span> ${esc(e.message)}</summary><pre class=diff></pre></details></li>`).join('')+'</ol>';
  document.querySelectorAll('#doc details').forEach(d=>d.ontoggle=async()=>{
    const pre=d.querySelector('pre');
    if(!d.open||pre.dataset.loaded)return;
    const r=await fetch(API+'/history/diff?commit='+d.dataset.commit+'&path='+encodeURIComponent(d.dataset.path));
    pre.innerHTML=(await r.text()).split('\n').map(l=>`<span class="${l[0]==='+'?'add':l[0]==='-'?'del':''}">${esc(l)}</span>`).join('\n');
    pre.dataset.loaded=1;
  });
}
```

The tab's `data-doc` is `#history`; `location.hash` `#history` selects it. CSS in `primum.css`:

```css
ol.history{list-style:none;padding:0;margin:0}
ol.history li{border-bottom:1px solid var(--line);padding:6px 0}
ol.history summary{cursor:pointer;font-size:13px}
ol.history .when{font:12px ui-monospace,Menlo,monospace;color:var(--mut)}
ol.history .who{color:var(--accent)}
pre.diff{font:12px/1.5 ui-monospace,Menlo,monospace;background:var(--bg);border:1px solid var(--line);border-radius:6px;padding:8px;overflow:auto;margin-top:6px}
pre.diff .add{color:#15803d}
pre.diff .del{color:var(--danger)}
```

- [ ] **Step 7:** `cargo test`; then build and check the tab by eye (`primum serve` on the demo board, an item with three changes). `make fmt && make lint-check`.
- [ ] **Step 8:** commit "primum log and the item's History tab".

---

### Task 4: The three-way item merge (`merge`)

**Files:** Create `src/merge.rs`, `tests/merge.rs`. Modify `src/lib.rs`.

**Interfaces:**
- Produces: `merge::FieldConflict { field: String, values: [String; 2] }`, `merge::Merged { item: Item, fields: Vec<FieldConflict>, body: Option<[String; 2]> }`, `merge::merge_item(base: Option<&Item>, ours: &Item, theirs: &Item, merge_text: &dyn Fn(&str, &str, &str) -> Option<String>) -> Merged`. The item in `Merged` carries `ours`'s value for every conflicting field.

- [ ] **Step 1: Failing tests** `tests/merge.rs`:

```rust
use primum::merge::*;
use primum::model::Item;

fn base() -> Item {
    let mut it = Item::new("TODO", 88, "Özgür'ün listesi");
    it.needs = vec!["BUG:1".into(), "BUG:2".into()];
    it.body = "bir\niki\nüç\n".into();
    it
}

/// A stand-in for git merge-file: clean only when one side kept the base.
fn text(b: &str, o: &str, t: &str) -> Option<String> {
    if o == b { Some(t.into()) } else if t == b { Some(o.into()) } else { None }
}

#[test]
fn different_fields_merge_cleanly() {
    let b = base();
    let mut o = b.clone();
    o.status = "doing".into();
    let mut t = b.clone();
    t.impact = 9;
    t.title = "Işıl'ın listesi".into();
    let m = merge_item(Some(&b), &o, &t, &text);
    assert!(m.fields.is_empty() && m.body.is_none());
    assert_eq!((m.item.status.as_str(), m.item.impact, m.item.title.as_str()), ("doing", 9, "Işıl'ın listesi"));
}

#[test]
fn the_same_change_on_both_sides_is_no_conflict() {
    let b = base();
    let mut o = b.clone();
    o.effort = 2;
    let t = o.clone();
    let m = merge_item(Some(&b), &o, &t, &text);
    assert!(m.fields.is_empty());
    assert_eq!(m.item.effort, 2);
}

#[test]
fn needs_merge_as_sets() {
    let b = base();
    let mut o = b.clone();
    o.needs = vec!["BUG:1".into(), "BUG:2".into(), "DEBT:3".into()];
    let mut t = b.clone();
    t.needs = vec!["BUG:2".into()];
    let m = merge_item(Some(&b), &o, &t, &text);
    assert_eq!(m.item.needs, vec!["BUG:2".to_string(), "DEBT:3".to_string()]);
}

#[test]
fn status_carries_finished_and_conflicts_keep_ours() {
    let b = base();
    let mut o = b.clone();
    o.status = "done".into();
    o.finished = Some("2026-10-14".into());
    let mut t = b.clone();
    t.status = "wontfix".into();
    t.finished = Some("2026-10-15".into());
    let m = merge_item(Some(&b), &o, &t, &text);
    assert_eq!(m.fields, vec![FieldConflict { field: "status".into(), values: ["done".into(), "wontfix".into()] }]);
    assert_eq!((m.item.status.as_str(), m.item.finished.as_deref()), ("done", Some("2026-10-14")));
    let mut t2 = b.clone();
    t2.status = "done".into();
    t2.finished = Some("2026-10-15".into());
    let mut o2 = b.clone();
    o2.risk = 7;
    let m = merge_item(Some(&b), &o2, &t2, &text);
    assert_eq!(m.item.finished.as_deref(), Some("2026-10-15"), "finished follows the status it came with");
}

#[test]
fn bodies_merge_as_text_or_conflict() {
    let b = base();
    let mut o = b.clone();
    o.body = "bir\nİKİ\nüç\n".into();
    let t = b.clone();
    assert_eq!(merge_item(Some(&b), &o, &t, &text).item.body, "bir\nİKİ\nüç\n");
    let mut t = b.clone();
    t.body = "bir\nikki\nüç\n".into();
    let m = merge_item(Some(&b), &o, &t, &text);
    assert_eq!(m.body, Some(["bir\nİKİ\nüç\n".to_string(), "bir\nikki\nüç\n".to_string()]));
    assert_eq!(m.item.body, "bir\nİKİ\nüç\n");
}

#[test]
fn without_a_base_differences_are_conflicts() {
    let o = base();
    let mut t = base();
    t.urgency = 8;
    let m = merge_item(None, &o, &t, &text);
    assert_eq!(m.fields[0].field, "urgency");
}
```

- [ ] **Step 2:** run, expect failure.

- [ ] **Step 3: `src/merge.rs`**

```rust
//! The three-way merge of one item, field by field, against the common
//! ancestor. Pure: the text merge of bodies is passed in.
use crate::model::Item;

#[derive(Debug, Clone, PartialEq)]
pub struct FieldConflict {
    pub field: String,
    /// This side's value, then the other side's.
    pub values: [String; 2],
}

#[derive(Debug, Clone)]
pub struct Merged {
    /// The merged item; a conflicting field holds this side's value.
    pub item: Item,
    pub fields: Vec<FieldConflict>,
    /// This side's body and the other side's, when they could not merge.
    pub body: Option<[String; 2]>,
}

enum Pick<T> {
    Clean(T),
    Conflict(T, T),
}

fn pick<T: PartialEq + Clone>(base: Option<&T>, ours: &T, theirs: &T) -> Pick<T> {
    if ours == theirs || base == Some(theirs) {
        Pick::Clean(ours.clone())
    } else if base == Some(ours) {
        Pick::Clean(theirs.clone())
    } else {
        Pick::Conflict(ours.clone(), theirs.clone())
    }
}

/// Both sides' additions and removals applied to the base.
fn merge_set(base: Option<&Vec<String>>, ours: &[String], theirs: &[String]) -> Vec<String> {
    let empty = vec![];
    let base = base.unwrap_or(&empty);
    let removed = |x: &String| {
        base.contains(x) && (!ours.contains(x) || !theirs.contains(x))
    };
    let mut out: Vec<String> = vec![];
    for x in ours.iter().chain(theirs) {
        if !removed(x) && !out.contains(x) {
            out.push(x.clone());
        }
    }
    out
}

pub fn merge_item(
    base: Option<&Item>,
    ours: &Item,
    theirs: &Item,
    merge_text: &dyn Fn(&str, &str, &str) -> Option<String>,
) -> Merged {
    let mut item = ours.clone();
    let mut fields = vec![];
    let mut conflict = |name: &str, o: String, t: String| {
        fields.push(FieldConflict { field: name.into(), values: [o, t] });
    };

    match pick(base.map(|b| &b.title), &ours.title, &theirs.title) {
        Pick::Clean(v) => item.title = v,
        Pick::Conflict(o, t) => conflict("title", o, t),
    }
    match pick(base.map(|b| &b.source), &ours.source, &theirs.source) {
        Pick::Clean(v) => item.source = v,
        Pick::Conflict(o, t) => conflict("source", o, t),
    }
    // finished goes with the status it was stamped with
    match pick(base.map(|b| &b.status), &ours.status, &theirs.status) {
        Pick::Clean(v) => {
            let theirs_won = v == theirs.status && v != ours.status;
            item.finished = if theirs_won { theirs.finished.clone() } else { ours.finished.clone() };
            item.status = v;
        }
        Pick::Conflict(o, t) => conflict("status", o, t),
    }
    for (name, get) in [
        ("impact", (|i: &Item| i.impact) as fn(&Item) -> u8),
        ("urgency", |i: &Item| i.urgency),
        ("risk", |i: &Item| i.risk),
        ("effort", |i: &Item| i.effort),
    ] {
        match pick(base.map(get).as_ref(), &get(ours), &get(theirs)) {
            Pick::Clean(v) => match name {
                "impact" => item.impact = v,
                "urgency" => item.urgency = v,
                "risk" => item.risk = v,
                _ => item.effort = v,
            },
            Pick::Conflict(o, t) => conflict(name, o.to_string(), t.to_string()),
        }
    }
    item.needs = merge_set(base.map(|b| &b.needs), &ours.needs, &theirs.needs);
    item.docs = merge_set(base.map(|b| &b.docs), &ours.docs, &theirs.docs);

    let mut body = None;
    match pick(base.map(|b| &b.body), &ours.body, &theirs.body) {
        Pick::Clean(v) => item.body = v,
        Pick::Conflict(o, t) => {
            let b = base.map(|b| b.body.as_str()).unwrap_or("");
            match merge_text(b, &o, &t) {
                Some(merged) => item.body = merged,
                None => body = Some([o, t]),
            }
        }
    }
    Merged { item, fields, body }
}

```

- [ ] **Step 4:** `cargo test --test merge`: 6 passed. `make fmt && make lint-check`.
- [ ] **Step 5:** commit `src/merge.rs src/lib.rs tests/merge.rs`: "Three-way merge of one item".

---

### Task 5: Conflicts on disk and on the board (`conflicts`)

**Files:** Create `src/conflicts.rs`. Modify `src/lib.rs`, `src/store.rs` (Board gains conflicts; BOARD.md lists them; `move_locked` renames, `delete` removes), `src/server.rs` (`ItemView.conflict`), `tests/store.rs`.

**Interfaces:**
- Produces: `conflicts::Side { author, date }`, `conflicts::FieldValues { field, values: [String; 2] }`, `conflicts::BodyValues { values: [String; 2] }`, `conflicts::DocConflict { path, other }`, `conflicts::Conflict { id, sides: [Side; 2], fields: Vec<FieldValues>, body: Option<BodyValues>, documents: Vec<DocConflict> }` (Serialize, Deserialize, Clone, Debug, PartialEq), `Conflict::is_empty()`, `Conflict::add(&mut self, other: Conflict)` (later entries for the same field or document replace earlier ones), `conflicts::read_all(&Project) -> Result<BTreeMap<String, Conflict>>`, `conflicts::write(&Project, &Conflict) -> Result<()>` (removes the file when empty), `conflicts::rename(&Project, old_id, new_id) -> Result<()>`, `conflicts::remove(&Project, id) -> Result<()>`; `Board.conflicts: BTreeMap<String, Conflict>`; `ItemView.conflict: Option<Conflict>`.

- [ ] **Step 1: Failing test** (`tests/store.rs`):

```rust
#[test]
fn conflicts_show_on_the_board_and_follow_their_item() {
    use primum::conflicts::*;
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Özgür", ch()).unwrap();
    let side = |a: &str| Side { author: a.into(), date: "2026-10-14T09:00:00+01:00".into() };
    write(&p, &Conflict {
        id: "TODO:1".into(),
        sides: [side("Özgür"), side("Işıl")],
        fields: vec![FieldValues { field: "status".into(), values: ["done".into(), "wontfix".into()] }],
        body: None,
        documents: vec![],
    }).unwrap();
    assert!(d.path().join("conflicts/TODO-1.json").is_file());
    let b = load(&p).unwrap();
    assert_eq!(b.conflicts["TODO:1"].fields[0].values[1], "wontfix");
    update(&p, "TODO:1", Changes { risk: Some(4), ..ch() }).unwrap();
    assert!(common::read(d.path(), "BOARD.md").contains("## Conflicts\n\n- [TODO:1](items/TODO-1.md): status"));
    set_kind(&p, "TODO:1", "BUG").unwrap();
    assert!(d.path().join("conflicts/BUG-1.json").is_file());
    assert_eq!(load(&p).unwrap().conflicts["BUG:1"].id, "BUG:1");
    delete(&p, "BUG:1").unwrap();
    assert!(!d.path().join("conflicts/BUG-1.json").exists());
}
```

- [ ] **Step 2:** run, expect failure.

- [ ] **Step 3: `src/conflicts.rs`**

```rust
//! Conflicts a sync could not decide: `conflicts/<STEM>.json`, one file per
//! item, tracked so every machine sees them and anyone can resolve them.
//! The item file holds the first value of each; the file holds both.
use crate::item_file::stem;
use crate::store::Project;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Side {
    pub author: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldValues {
    pub field: String,
    pub values: [String; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyValues {
    pub values: [String; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocConflict {
    /// The document as this side has it, in place.
    pub path: String,
    /// The other side's text, saved beside it.
    pub other: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub id: String,
    pub sides: [Side; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldValues>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyValues>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<DocConflict>,
}

impl Conflict {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.body.is_none() && self.documents.is_empty()
    }

    /// Adds `other`'s entries; one for a field or document already here
    /// replaces it.
    pub fn add(&mut self, other: Conflict) {
        self.sides = other.sides;
        for f in other.fields {
            self.fields.retain(|x| x.field != f.field);
            self.fields.push(f);
        }
        if other.body.is_some() {
            self.body = other.body;
        }
        for d in other.documents {
            self.documents.retain(|x| x.path != d.path);
            self.documents.push(d);
        }
    }
}

fn dir(p: &Project) -> PathBuf {
    p.root.join("conflicts")
}

fn path(p: &Project, id: &str) -> PathBuf {
    dir(p).join(format!("{}.json", stem(id)))
}

pub fn read_all(p: &Project) -> Result<BTreeMap<String, Conflict>> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(dir(p)) else {
        return Ok(out);
    };
    for e in entries.flatten() {
        let file = e.path();
        if file.extension().is_some_and(|x| x == "json") {
            let text = fs::read_to_string(&file)?;
            let c: Conflict = serde_json::from_str(&text)
                .with_context(|| format!("{} is not a conflict record", file.display()))?;
            out.insert(c.id.clone(), c);
        }
    }
    Ok(out)
}

pub fn write(p: &Project, c: &Conflict) -> Result<()> {
    let file = path(p, &c.id);
    if c.is_empty() {
        if file.exists() {
            fs::remove_file(file)?;
        }
        return Ok(());
    }
    fs::create_dir_all(dir(p))?;
    fs::write(file, serde_json::to_string_pretty(c)? + "\n")?;
    Ok(())
}

pub fn rename(p: &Project, old_id: &str, new_id: &str) -> Result<()> {
    let from = path(p, old_id);
    if let Ok(text) = fs::read_to_string(&from) {
        let mut c: Conflict = serde_json::from_str(&text)?;
        c.id = new_id.to_string();
        fs::remove_file(&from)?;
        write(p, &c)?;
    }
    Ok(())
}

pub fn remove(p: &Project, id: &str) -> Result<()> {
    let file = path(p, id);
    if file.exists() {
        fs::remove_file(file)?;
    }
    Ok(())
}
```

- [ ] **Step 4: store.** `Board` gains `pub conflicts: BTreeMap<String, crate::conflicts::Conflict>`, filled in `load` by `crate::conflicts::read_all(p)?`. `move_locked` calls `crate::conflicts::rename(p, id, &new_id)?` (the line marked in Task 2). `delete` calls `crate::conflicts::remove(p, id)?` after removing the item's files. `board_markdown` inserts, after the counts line and before `## Open items`, when `board.conflicts` is not empty:

```rust
    if !board.conflicts.is_empty() {
        out.push("## Conflicts\n".to_string());
        for (id, c) in &board.conflicts {
            let mut what: Vec<String> = c.fields.iter().map(|f| f.field.clone()).collect();
            if c.body.is_some() { what.push("body".into()); }
            what.extend(c.documents.iter().map(|d| d.path.clone()));
            out.push(format!("- [{id}](items/{}): {}", file_name(id), what.join(", ")));
        }
        out.push(String::new());
    }
```

**server.** `ItemView` gains `pub conflict: Option<crate::conflicts::Conflict>`, set from `board.conflicts.get(&it.id).cloned()`.

- [ ] **Step 5:** `cargo test`: all pass. `make fmt && make lint-check`.
- [ ] **Step 6:** commit `src/conflicts.rs src/lib.rs src/store.rs src/server.rs tests/store.rs`: "Conflicts on disk and on the board".

---

### Task 6: Remotes and clone (`history`, `cli`)

**Files:** Modify `src/history.rs`, `src/cli.rs`, `src/projects.rs`, `tests/history.rs`, `tests/cli.rs`, `tests/common/mod.rs`.

**Interfaces:**
- Produces: `history::Remote { url: String, branch: String, manual: bool }`; `History::remote(&self) -> Result<Option<Remote>>`; `History::set_remote(&self, url, branch: Option<&str>, manual: bool) -> Result<Remote>`; `History::remove_remote(&self) -> Result<()>`; `enum Fetched { Ok, Empty, Offline(String) }`, `History::fetch(&self, &Remote) -> Result<Fetched>`; `enum Pushed { Ok, Rejected, Offline(String) }`, `History::push(&self, &Remote) -> Result<Pushed>`; `history::clone(url, branch: Option<&str>, folder: &Path) -> Result<()>`; `History::run_timed(&self, args, secs) -> Result<Option<Output>>` (None on timeout); CLI `primum remote [set URL [--branch B] [--manual] | remove]`, `primum clone URL [FOLDER] [--branch B]`.

- [ ] **Step 1: Helpers** (`tests/common/mod.rs`):

```rust
/// An empty bare repository to use as a remote, as a file URL.
pub fn remote(dir: &Path) -> String {
    let r = dir.join("remote.git");
    let out = std::process::Command::new("git")
        .args(["init", "--quiet", "--bare", "-b", "main"])
        .arg(&r)
        .output()
        .unwrap();
    assert!(out.status.success());
    r.to_string_lossy().to_string()
}

/// A person: a project folder `name` with its history's identity set.
pub fn person(dir: &Path, name: &str) -> Project {
    let mut p = project(&dir.join(name));
    p.name = "ortak".into();
    let h = history(&p);
    h.git(&["config", "user.name", name]).unwrap();
    h.git(&["config", "user.email", &format!("{}@example.com", name.to_lowercase())]).unwrap();
    p
}
```

- [ ] **Step 2: Failing tests** (`tests/history.rs`):

```rust
#[test]
fn remote_settings_live_in_the_history_not_primum_toml() {
    let d = tempfile::tempdir().unwrap();
    let url = common::remote(d.path());
    let p = common::person(d.path(), "Özgür");
    let h = common::history(&p);
    assert!(h.remote().unwrap().is_none());
    let r = h.set_remote(&url, Some("videre"), false).unwrap();
    assert_eq!((r.branch.as_str(), r.manual), ("videre", false));
    assert_eq!(h.remote().unwrap().unwrap().url, url);
    assert_eq!(h.git(&["symbolic-ref", "--short", "HEAD"]).unwrap().trim(), "videre");
    h.set_remote(&url, None, true).unwrap();
    assert!(h.remote().unwrap().unwrap().manual);
    h.remove_remote().unwrap();
    assert!(h.remote().unwrap().is_none());
}

#[test]
fn push_fetch_and_clone() {
    use primum::history::{Fetched, Pushed, clone};
    let d = tempfile::tempdir().unwrap();
    let url = common::remote(d.path());
    let p = common::person(d.path(), "Özgür");
    std::fs::write(p.root.join("primum.toml"), "name = \"ortak\"\n").unwrap();
    primum::store::insert(&p, "TODO", "Gökçe'nin işi", Default::default()).unwrap();
    let h = common::history(&p);
    let r = h.set_remote(&url, None, false).unwrap();
    assert!(matches!(h.fetch(&r).unwrap(), Fetched::Empty));
    assert!(matches!(h.push(&r).unwrap(), Pushed::Ok));
    assert!(matches!(h.fetch(&r).unwrap(), Fetched::Ok));
    let other = d.path().join("ışıl");
    clone(&url, None, &other).unwrap();
    assert!(other.join("items/TODO-1.md").is_file());
    assert!(other.join("BOARD.md").is_file());
    let unreachable = h.set_remote("/yok/remote.git", None, false).unwrap();
    assert!(matches!(h.fetch(&unreachable).unwrap(), Fetched::Offline(_)));
}
```

(`tests/cli.rs`) `primum clone` lists the project and `primum remote` prints the URL:

```rust
#[test]
fn remote_and_clone_commands() {
    let t = T::new();
    t.out(&["add", "TODO", "Özgür"]);
    let url = t.dir.path().join("remote.git");
    assert!(Command::new("git").args(["init", "--quiet", "--bare", "-b", "main"]).arg(&url).status().unwrap().success());
    let url = url.to_string_lossy().to_string();
    t.out(&["remote", "set", &url, "--manual"]);
    assert!(t.out(&["remote"]).contains(&url));
    t.out(&["sync"]);
    // a second machine: its own config
    let cfg2 = t.dir.path().join("config2");
    let o = Command::new(env!("CARGO_BIN_EXE_primum"))
        .args(["clone", &url, "kopya"])
        .current_dir(t.dir.path())
        .env("XDG_CONFIG_HOME", &cfg2)
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    assert!(t.dir.path().join("kopya/items/TODO-1.md").is_file());
    assert!(std::fs::read_to_string(cfg2.join("primum/projects.toml")).unwrap().contains("kopya"));
}
```

(This test needs `sync` from Task 7; mark it `#[ignore = "needs sync, Task 7"]` here and remove the attribute in Task 7.)

- [ ] **Step 3: `history.rs`** additions:

```rust
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct Remote {
    pub url: String,
    pub branch: String,
    pub manual: bool,
}

pub enum Fetched {
    Ok,
    /// The remote has no such branch yet.
    Empty,
    Offline(String),
}

pub enum Pushed {
    Ok,
    /// The remote moved since the fetch.
    Rejected,
    Offline(String),
}

const NETWORK_SECS: u64 = 10;

fn network(c: &mut Command) {
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        c.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes -o ConnectTimeout=10");
    }
}

/// Runs `c`, killing it after `secs`. None on timeout.
fn timed(mut c: Command, secs: u64) -> Result<Option<Output>> {
    network(&mut c);
    let mut child = c
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run git")?;
    let start = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return Ok(Some(child.wait_with_output()?));
        }
        if start.elapsed() > Duration::from_secs(secs) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).trim().to_string()
}

impl History {
    pub fn run_timed(&self, args: &[&str], secs: u64) -> Result<Option<Output>> {
        let mut c = self.command();
        c.args(args);
        timed(c, secs)
    }

    fn config(&self, key: &str) -> Result<Option<String>> {
        let out = self.run(&["config", "--get", key])?;
        Ok(out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()))
    }

    pub fn remote(&self) -> Result<Option<Remote>> {
        let Some(url) = self.config("remote.origin.url")? else {
            return Ok(None);
        };
        Ok(Some(Remote {
            url,
            branch: self.config("primum.branch")?.unwrap_or_else(|| "main".into()),
            manual: self.config("primum.sync")?.as_deref() == Some("manual"),
        }))
    }

    pub fn set_remote(&self, url: &str, branch: Option<&str>, manual: bool) -> Result<Remote> {
        let branch = match branch {
            Some(b) => b.to_string(),
            None => self.config("primum.branch")?.unwrap_or_else(|| "main".into()),
        };
        if self.config("remote.origin.url")?.is_some() {
            self.git(&["remote", "set-url", "origin", url])?;
        } else {
            self.git(&["remote", "add", "origin", url])?;
        }
        let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
        self.git(&["config", "remote.origin.fetch", &refspec])?;
        self.git(&["config", "primum.branch", &branch])?;
        self.git(&["config", "primum.sync", if manual { "manual" } else { "auto" }])?;
        let current = self.git(&["symbolic-ref", "--short", "HEAD"])?.trim().to_string();
        if current != branch {
            self.git(&["branch", "-m", &current, &branch])?;
        }
        Ok(Remote { url: url.into(), branch, manual })
    }

    pub fn remove_remote(&self) -> Result<()> {
        if self.config("remote.origin.url")?.is_some() {
            self.git(&["remote", "remove", "origin"])?;
        }
        let _ = self.run(&["config", "--unset", "primum.sync"])?;
        Ok(())
    }

    pub fn fetch(&self, r: &Remote) -> Result<Fetched> {
        let refspec = format!("+refs/heads/{0}:refs/remotes/origin/{0}", r.branch);
        let Some(out) = self.run_timed(&["fetch", "--quiet", "origin", &refspec], NETWORK_SECS)? else {
            return Ok(Fetched::Offline("the remote did not answer".into()));
        };
        if out.status.success() {
            return Ok(Fetched::Ok);
        }
        let err = stderr(&out);
        if err.contains("couldn't find remote ref") {
            return Ok(Fetched::Empty);
        }
        Ok(Fetched::Offline(err))
    }

    pub fn push(&self, r: &Remote) -> Result<Pushed> {
        let refspec = format!("HEAD:refs/heads/{}", r.branch);
        let Some(out) = self.run_timed(&["push", "--quiet", "origin", &refspec], NETWORK_SECS)? else {
            return Ok(Pushed::Offline("the remote did not answer".into()));
        };
        if out.status.success() {
            return Ok(Pushed::Ok);
        }
        let err = stderr(&out);
        if err.contains("rejected") || err.contains("fetch first") || err.contains("non-fast-forward") {
            return Ok(Pushed::Rejected);
        }
        Ok(Pushed::Offline(err))
    }
}

/// Makes `folder` a project from a remote: a bare clone of its history and
/// the files written from it. The caller lists the project.
pub fn clone(url: &str, branch: Option<&str>, folder: &std::path::Path) -> Result<()> {
    if folder.join(".primum").exists() || folder.join("primum.toml").exists() {
        bail!("{} is already a primum project", folder.display());
    }
    std::fs::create_dir_all(folder.join(".primum"))?;
    let h = History { git_dir: folder.join(".primum/history.git"), work_tree: folder.to_path_buf() };
    let mut c = Command::new(git_binary());
    c.args(["clone", "--quiet", "--bare"]);
    if let Some(b) = branch {
        c.args(["--branch", b]);
    }
    c.arg(url).arg(&h.git_dir).env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::null());
    let out = timed(c, 60)?.ok_or_else(|| anyhow::anyhow!("the remote did not answer"))?;
    check(out, "clone")?;
    std::fs::write(folder.join(".primum/.gitignore"), "*\n")?;
    h.write_exclude()?;
    let branch = h.git(&["symbolic-ref", "--short", "HEAD"])?.trim().to_string();
    let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    h.git(&["config", "remote.origin.fetch", &refspec])?;
    h.git(&["config", "primum.branch", &branch])?;
    h.git(&["config", "primum.sync", "auto"])?;
    h.git(&["read-tree", "-u", "--reset", "HEAD"])?;
    if !folder.join("primum.toml").is_file() {
        bail!("{url} has no primum.toml on {branch}: not a primum project");
    }
    Ok(())
}
```

- [ ] **Step 4: CLI.**

```rust
    /// This machine's remote for the project, or set or remove it
    Remote {
        #[command(subcommand)]
        cmd: Option<RemoteCmd>,
    },
    /// Make a project from a remote and list it
    Clone {
        url: String,
        /// Default: the remote's name without .git
        folder: Option<PathBuf>,
        #[arg(long)]
        branch: Option<String>,
    },

#[derive(Subcommand)]
pub enum RemoteCmd {
    /// Sync with URL (the branch defaults to main)
    Set {
        url: String,
        #[arg(long)]
        branch: Option<String>,
        /// Sync only with `primum sync`
        #[arg(long)]
        manual: bool,
    },
    Remove,
}
```

`Remote { cmd: None }` prints `url  branch  auto|manual`, or `no remote` (exit 0). `Clone` computes the folder (`url` trimmed of `/` and `.git`, last path segment) relative to the working directory, calls `history::clone`, then `projects::add(&folder)` and prints `primum project <name> in <folder>`.

- [ ] **Step 5:** `cargo test --test history`: passes. `make fmt && make lint-check`.
- [ ] **Step 6:** commit "Remotes per machine, and primum clone".

---

### Task 7: Sync (`sync`)

**Files:** Create `src/sync.rs`, `tests/sync.rs`. Modify `src/lib.rs`, `src/history.rs` (merge helpers), `src/cli.rs` (`primum sync`), `tests/cli.rs` (remove the `#[ignore]`).

**Interfaces:**
- Consumes: `History::{fetch, push, remote, record_outside, commit, git, run}`, `merge::merge_item`, `conflicts::*`, `store::{lock, move_locked, settle_locked, load}`, `item_file::{parse, write, parse_item_name, parse_doc_name}`.
- Produces: `sync::Report { pulled: bool, pushed: bool, renumbered: Vec<(String, String)>, conflicts: Vec<String>, kept: Vec<String>, offline: Option<String>, no_remote: bool }` (Default, Debug); `sync::sync(&Project) -> Result<Report>`; `History::stage(&self, n: u8, path) -> Result<Option<String>>`; `History::merge_text(&self, base, ours, theirs) -> Result<Option<String>>`; `History::side(&self, rev, path) -> Result<conflicts::Side>`; CLI `primum sync`.

- [ ] **Step 1: Failing tests** `tests/sync.rs`. Each test makes a remote and two people, Özgür and Işıl: Özgür creates the project and pushes, Işıl clones.

```rust
mod common;
use primum::history::clone;
use primum::store::*;
use primum::sync::sync;
use std::path::Path;

fn ch() -> Changes { Changes::default() }

/// Özgür's project on a fresh remote with BUG:1 and TODO:1, and Işıl's
/// clone of it.
fn two(dir: &Path) -> (Project, Project) {
    let url = common::remote(dir);
    let oz = common::person(dir, "Özgür");
    std::fs::write(oz.root.join("primum.toml"), "name = \"ortak\"\n").unwrap();
    insert(&oz, "BUG", "Gökçe'nin hatası", ch()).unwrap();
    insert(&oz, "TODO", "Özgür'ün işi", Changes { body: Some("bir\niki\nüç\n".into()), ..ch() }).unwrap();
    common::history(&oz).set_remote(&url, None, false).unwrap();
    assert!(sync(&oz).unwrap().pushed);
    clone(&url, None, &dir.join("Işıl")).unwrap();
    let is = Project { root: dir.join("Işıl"), name: "ortak".into() };
    let h = common::history(&is);
    h.git(&["config", "user.name", "Işıl"]).unwrap();
    h.git(&["config", "user.email", "isil@example.com"]).unwrap();
    (oz, is)
}

#[test]
fn different_items_and_adjacent_fields_merge_without_asking() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    update(&oz, "TODO:1", Changes { status: Some("doing".into()), ..ch() }).unwrap();
    update(&oz, "BUG:1", Changes { effort: Some(2), ..ch() }).unwrap();
    sync(&oz).unwrap();
    update(&is, "TODO:1", Changes { impact: Some(9), ..ch() }).unwrap();
    let r = sync(&is).unwrap();
    assert!(r.pulled && r.pushed && r.conflicts.is_empty(), "{r:?}");
    let b = load(&is).unwrap();
    let t = b.get("TODO:1").unwrap();
    assert_eq!((t.status.as_str(), t.impact), ("doing", 9));
    assert_eq!(b.get("BUG:1").unwrap().effort, 2);
    sync(&oz).unwrap();
    assert_eq!(load(&oz).unwrap().get("TODO:1").unwrap().impact, 9);
    assert!(common::subjects(&is).iter().any(|s| s.starts_with("sync with main")));
}

#[test]
fn bodies_merge_by_line() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    update(&oz, "TODO:1", Changes { body: Some("BİR\niki\nüç\n".into()), ..ch() }).unwrap();
    sync(&oz).unwrap();
    update(&is, "TODO:1", Changes { body: Some("bir\niki\nÜÇ\n".into()), ..ch() }).unwrap();
    assert!(sync(&is).unwrap().conflicts.is_empty());
    assert_eq!(load(&is).unwrap().get("TODO:1").unwrap().body, "BİR\niki\nÜÇ\n");
}

#[test]
fn the_same_new_id_on_both_sides_renumbers_the_local_one() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    insert(&oz, "TODO", "Özgür'ün yenisi", ch()).unwrap();
    sync(&oz).unwrap();
    insert(&is, "TODO", "Işıl'ın yenisi", ch()).unwrap();
    insert(&is, "BUG", "bekler", Changes { needs: Some(vec!["TODO:2".into()]), ..ch() }).unwrap();
    set_doc(&is, "TODO:2", "spec", "# Işıl\n").unwrap();
    let r = sync(&is).unwrap();
    assert_eq!(r.renumbered, [("TODO:2".to_string(), "TODO:3".to_string())]);
    let b = load(&is).unwrap();
    assert_eq!(b.get("TODO:2").unwrap().title, "Özgür'ün yenisi");
    assert_eq!(b.get("TODO:3").unwrap().title, "Işıl'ın yenisi");
    assert_eq!(b.get("BUG:2").unwrap().needs, vec!["TODO:3".to_string()]);
    assert!(is.root.join("items/TODO-3.spec.md").is_file());
}

#[test]
fn a_delete_against_an_edit_keeps_the_item() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    delete(&oz, "BUG:1").unwrap();
    sync(&oz).unwrap();
    update(&is, "BUG:1", Changes { risk: Some(9), ..ch() }).unwrap();
    let r = sync(&is).unwrap();
    assert_eq!(r.kept.len(), 1, "{r:?}");
    assert_eq!(load(&is).unwrap().get("BUG:1").unwrap().risk, 9);
}

#[test]
fn a_true_conflict_is_recorded_everywhere_and_resolved_once() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    update(&oz, "TODO:1", Changes { status: Some("done".into()), ..ch() }).unwrap();
    sync(&oz).unwrap();
    update(&is, "TODO:1", Changes { status: Some("wontfix".into()), ..ch() }).unwrap();
    let r = sync(&is).unwrap();
    assert_eq!(r.conflicts, ["TODO:1"]);
    let b = load(&is).unwrap();
    let c = &b.conflicts["TODO:1"];
    assert_eq!(c.fields[0].values, ["wontfix".to_string(), "done".to_string()]);
    assert_eq!((c.sides[0].author.as_str(), c.sides[1].author.as_str()), ("Işıl", "Özgür"));
    assert_eq!(b.get("TODO:1").unwrap().status, "wontfix");
    sync(&oz).unwrap();
    assert!(load(&oz).unwrap().conflicts.contains_key("TODO:1"), "the conflict reaches Özgür");
}

#[test]
fn offline_keeps_the_change_and_a_race_is_retried() {
    let d = tempfile::tempdir().unwrap();
    let (oz, _is) = two(d.path());
    let h = common::history(&oz);
    let real = h.remote().unwrap().unwrap().url;
    h.set_remote("/yok/remote.git", None, false).unwrap();
    update(&oz, "BUG:1", Changes { urgency: Some(8), ..ch() }).unwrap();
    let r = sync(&oz).unwrap();
    assert!(r.offline.is_some() && !r.pushed);
    assert_eq!(common::subjects(&oz)[0], "BUG:1: urgency");
    h.set_remote(&real, None, false).unwrap();
    assert!(sync(&oz).unwrap().pushed);
}

#[test]
fn an_unrelated_history_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let (_oz, _is) = two(d.path());
    let url = d.path().join("remote.git").to_string_lossy().to_string();
    let gokce = common::person(d.path(), "Gökçe");
    insert(&gokce, "TODO", "ayrı başladı", ch()).unwrap();
    common::history(&gokce).set_remote(&url, None, false).unwrap();
    let e = sync(&gokce).unwrap_err().to_string();
    assert!(e.contains("primum clone"), "{e}");
}
```

- [ ] **Step 2:** `cargo test --test sync`: compile failure.

- [ ] **Step 3: `history.rs`** merge helpers:

```rust
impl History {
    /// A path's text at merge stage `n` (1 base, 2 ours, 3 theirs), None
    /// when that side has no such file.
    pub fn stage(&self, n: u8, path: &str) -> Result<Option<String>> {
        let out = self.run(&["show", &format!(":{n}:{path}")])?;
        Ok(out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string()))
    }

    /// git merge-file on three texts: the merge, or None on conflict.
    pub fn merge_text(&self, base: &str, ours: &str, theirs: &str) -> Result<Option<String>> {
        let dir = self.git_dir.join("primum-merge");
        std::fs::create_dir_all(&dir)?;
        let (o, b, t) = (dir.join("ours"), dir.join("base"), dir.join("theirs"));
        std::fs::write(&o, ours)?;
        std::fs::write(&b, base)?;
        std::fs::write(&t, theirs)?;
        let out = Command::new(git_binary())
            .args(["merge-file", "-p", "--quiet"])
            .args([&o, &b, &t])
            .stdin(Stdio::null())
            .output()?;
        let _ = std::fs::remove_dir_all(&dir);
        match out.status.code() {
            Some(0) => Ok(Some(String::from_utf8_lossy(&out.stdout).to_string())),
            Some(n) if n > 0 => Ok(None),
            _ => bail!("git merge-file failed: {}", stderr(&out)),
        }
    }

    /// Who last changed `path` on `rev`, and when.
    pub fn side(&self, rev: &str, path: &str) -> Result<crate::conflicts::Side> {
        let out = self.git(&["log", "-1", "--format=%an%x1f%aI", rev, "--", path])?;
        let (author, date) = out.trim().split_once('\x1f').unwrap_or(("", ""));
        Ok(crate::conflicts::Side { author: author.into(), date: date.into() })
    }
}
```

- [ ] **Step 4: `src/sync.rs`**

```rust
//! Sync with the project's remote: commit outside edits, fetch, renumber
//! new items whose ids were taken on the remote, merge (primum decides
//! items field by field; git only carries), settle, push. Only a true
//! conflict is left for a person, in conflicts/.
use crate::conflicts::{self, BodyValues, Conflict, DocConflict, FieldValues};
use crate::history::{Fetched, History, Pushed, Remote};
use crate::item_file::{self, parse_doc_name, parse_item_name};
use crate::merge::merge_item;
use crate::model::id_of;
use crate::store::{self, Project};
use anyhow::{Result, bail};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct Report {
    pub pulled: bool,
    pub pushed: bool,
    /// (old id, new id) of local items moved to a free number.
    pub renumbered: Vec<(String, String)>,
    /// Items left with a conflict.
    pub conflicts: Vec<String>,
    /// What was kept by rule, said once.
    pub kept: Vec<String>,
    pub offline: Option<String>,
    pub no_remote: bool,
}

pub fn sync(p: &Project) -> Result<Report> {
    let _l = store::lock(p)?;
    let Some(h) = History::open(p)? else {
        bail!("history is off: git not found");
    };
    let mut report = Report::default();
    let Some(remote) = h.remote()? else {
        report.no_remote = true;
        return Ok(report);
    };
    h.record_outside()?;
    let theirs = format!("refs/remotes/origin/{}", remote.branch);
    for _ in 0..3 {
        match h.fetch(&remote)? {
            Fetched::Offline(why) => {
                report.offline = Some(why);
                return Ok(report);
            }
            Fetched::Empty => {}
            Fetched::Ok => pull(p, &h, &theirs, &remote, &mut report)?,
        }
        let ahead = match h.run(&["rev-parse", "--verify", "--quiet", &theirs])?.status.success() {
            true => h.git(&["rev-list", "--count", &format!("{theirs}..HEAD")])?.trim() != "0",
            false => true,
        };
        if !ahead {
            return Ok(report);
        }
        match h.push(&remote)? {
            Pushed::Ok => {
                report.pushed = true;
                return Ok(report);
            }
            Pushed::Rejected => continue,
            Pushed::Offline(why) => {
                report.offline = Some(why);
                return Ok(report);
            }
        }
    }
    bail!("the remote kept changing during the sync; run primum sync again")
}

fn pull(p: &Project, h: &History, theirs: &str, remote: &Remote, report: &mut Report) -> Result<()> {
    if h.run(&["merge-base", "--is-ancestor", theirs, "HEAD"])?.status.success() {
        return Ok(()); // nothing new on the remote
    }
    let base = h.run(&["merge-base", "HEAD", theirs])?;
    if !base.status.success() {
        bail!(
            "this project's history and the remote's ({} {}) are unrelated; \
             make the project with primum clone instead",
            remote.url, remote.branch
        );
    }
    let base = String::from_utf8_lossy(&base.stdout).trim().to_string();
    if h.run(&["merge-base", "--is-ancestor", "HEAD", theirs])?.status.success() {
        h.git(&["merge", "--quiet", "--ff-only", theirs])?;
        report.pulled = true;
        return Ok(());
    }
    renumber(p, h, &base, theirs, report)?;
    merge(p, h, theirs, remote, report)?;
    report.pulled = true;
    Ok(())
}

fn added_items(h: &History, base: &str, rev: &str) -> Result<BTreeSet<String>> {
    Ok(h.git(&["diff", "--name-only", "--diff-filter=A", base, rev, "--", "items/"])?
        .lines()
        .filter_map(|l| l.strip_prefix("items/"))
        .filter(|n| parse_item_name(n).is_some())
        .map(str::to_string)
        .collect())
}

/// A new local item whose id was also taken on the remote moves to the
/// next number free on both sides. The remote's numbers never change.
fn renumber(p: &Project, h: &History, base: &str, theirs: &str, report: &mut Report) -> Result<()> {
    let ours = added_items(h, base, "HEAD")?;
    let remote_new = added_items(h, base, theirs)?;
    let remote_names: Vec<String> = h
        .git(&["ls-tree", "--name-only", theirs, "items/"])?
        .lines()
        .filter_map(|l| l.strip_prefix("items/").map(str::to_string))
        .collect();
    for name in ours.intersection(&remote_new) {
        let (kind, num) = parse_item_name(name).expect("filtered above");
        let old = id_of(&kind, num);
        let board = store::load(p)?;
        let taken = |n: u32| {
            board.get(&id_of(&kind, n)).is_some()
                || remote_names.iter().any(|r| parse_item_name(r) == Some((kind.clone(), n)))
        };
        let mut free = num + 1;
        while taken(free) {
            free += 1;
        }
        let new = store::move_locked(p, &old, &kind, free)?;
        h.commit(&format!("{old} -> {new}: number taken on the remote"))?;
        report.renumbered.push((old, new));
    }
    Ok(())
}

fn merge(p: &Project, h: &History, theirs: &str, remote: &Remote, report: &mut Report) -> Result<()> {
    let out = h.run(&["merge", "--no-ff", "--no-commit", "--quiet", theirs])?;
    let unmerged: Vec<String> = h
        .git(&["diff", "--name-only", "--diff-filter=U"])?
        .lines()
        .map(str::to_string)
        .collect();
    if !out.status.success() && unmerged.is_empty() {
        let _ = h.run(&["merge", "--abort"]);
        bail!("git merge failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    if let Err(e) = resolve_all(p, h, theirs, &unmerged, report) {
        let _ = h.run(&["merge", "--abort"]);
        return Err(e);
    }
    store::settle_locked(p)?;
    h.git(&["add", "--all", "--", "."])?;
    h.git(&["commit", "--quiet", "--no-verify", "-m", &format!("sync with {} {}", remote.branch, remote.url)])?;
    Ok(())
}

fn resolve_all(p: &Project, h: &History, theirs: &str, paths: &[String], report: &mut Report) -> Result<()> {
    let text = |b: &str, o: &str, t: &str| h.merge_text(b, o, t).ok().flatten();
    for path in paths {
        let (base, ours, other) = (h.stage(1, path)?, h.stage(2, path)?, h.stage(3, path)?);
        let file = p.root.join(path);
        let name = path.rsplit('/').next().unwrap_or(path);
        if path == "BOARD.md" {
            std::fs::write(&file, ours.or(other).unwrap_or_default())?; // rebuilt by settle
        } else if path.starts_with("items/") && parse_item_name(name).is_some() {
            match (ours, other) {
                (Some(o), Some(t)) => {
                    let parse = |s: &str| item_file::parse(name, s);
                    let base = base.as_deref().map(parse).transpose()?;
                    let (o, t) = (parse(&o)?, parse(&t)?);
                    let m = merge_item(base.as_ref(), &o, &t, &text);
                    std::fs::write(&file, item_file::write(&m.item))?;
                    if !m.fields.is_empty() || m.body.is_some() {
                        let c = Conflict {
                            id: m.item.id.clone(),
                            sides: [h.side("HEAD", path)?, h.side(theirs, path)?],
                            fields: m.fields.into_iter().map(|f| FieldValues { field: f.field, values: f.values }).collect(),
                            body: m.body.map(|values| BodyValues { values }),
                            documents: vec![],
                        };
                        record(p, c)?;
                        report.conflicts.push(m.item.id.clone());
                    }
                }
                (Some(kept), None) | (None, Some(kept)) => {
                    std::fs::write(&file, &kept)?;
                    let id = parse_item_name(name).map(|(k, n)| id_of(&k, n)).unwrap_or_default();
                    report.kept.push(format!("{id} was deleted on one side and changed on the other; kept"));
                }
                (None, None) => {}
            }
        } else if path.starts_with("items/") && parse_doc_name(name).is_some() {
            match (ours, other) {
                (Some(o), Some(t)) => match text(base.as_deref().unwrap_or(""), &o, &t) {
                    Some(merged) => std::fs::write(&file, merged)?,
                    None => {
                        std::fs::write(&file, &o)?;
                        let short = h.git(&["rev-parse", "--short", theirs])?.trim().to_string();
                        let other_name = name.replacen(".md", &format!(".{short}.md"), 1);
                        std::fs::write(p.items_dir().join(&other_name), &t)?;
                        let (kind, num, _) = parse_doc_name(name).expect("checked");
                        let id = id_of(&kind, num);
                        record(p, Conflict {
                            id: id.clone(),
                            sides: [h.side("HEAD", path)?, h.side(theirs, path)?],
                            fields: vec![],
                            body: None,
                            documents: vec![DocConflict { path: path.clone(), other: format!("items/{other_name}") }],
                        })?;
                        report.conflicts.push(id);
                    }
                },
                (Some(kept), None) | (None, Some(kept)) => std::fs::write(&file, kept)?,
                (None, None) => {}
            }
        } else if path.starts_with("conflicts/") {
            let parse = |s: &Option<String>| s.as_deref().and_then(|t| serde_json::from_str::<Conflict>(t).ok());
            let mut c = parse(&ours).or_else(|| parse(&other)).expect("one side has it");
            if let Some(t) = parse(&other) {
                c.add(t);
            }
            std::fs::write(&file, serde_json::to_string_pretty(&c)? + "\n")?;
        } else {
            std::fs::write(&file, ours.or(other).unwrap_or_default())?;
            report.kept.push(format!("{path} changed on both sides; kept this side's"));
        }
        h.git(&["add", "--", path])?;
    }
    report.conflicts.sort();
    report.conflicts.dedup();
    Ok(())
}

/// Adds `c` to the item's conflict file.
fn record(p: &Project, c: Conflict) -> Result<()> {
    let mut all = conflicts::read_all(p)?;
    let entry = all.remove(&c.id);
    let merged = match entry {
        Some(mut existing) => {
            existing.add(c);
            existing
        }
        None => c,
    };
    conflicts::write(p, &merged)
}
```

- [ ] **Step 5: CLI `primum sync`**, printing the report:

```rust
Cmd::Sync => print_sync(&crate::sync::sync(&project()?)?, true),

/// Says what a sync did. `verbose` for `primum sync`; automatic syncs say
/// only what needs attention.
fn print_sync(r: &crate::sync::Report, verbose: bool) {
    if r.no_remote {
        if verbose { println!("no remote: set one with primum remote set URL"); }
        return;
    }
    if let Some(why) = &r.offline {
        eprintln!("primum: offline, changes stay local until the next sync ({why})");
    }
    for (old, new) in &r.renumbered {
        eprintln!("primum: {old} is now {new} (the number was taken on the remote)");
    }
    for k in &r.kept {
        eprintln!("primum: {k}");
    }
    for id in &r.conflicts {
        eprintln!("primum: {id} has a conflict: primum resolve {id}");
    }
    if verbose && r.offline.is_none() {
        println!("{}", match (r.pulled, r.pushed) {
            (true, true) => "pulled and pushed",
            (true, false) => "pulled",
            (false, true) => "pushed",
            (false, false) => "up to date",
        });
    }
}
```

Remove the `#[ignore]` from `remote_and_clone_commands`.

- [ ] **Step 6:** `cargo test`: all pass. `make fmt && make lint-check`.
- [ ] **Step 7:** commit `src/sync.rs src/lib.rs src/history.rs src/cli.rs tests/sync.rs tests/cli.rs`: "Sync: primum merges, git carries".

---

### Task 8: Automatic sync

**Files:** Modify `src/cli.rs`, `src/server.rs`, `static/board.js`, `static/item.js`, `tests/cli.rs`.

**Interfaces:**
- Consumes: `sync::sync`, `History::remote`, `print_sync`.
- Produces: `cli::auto_sync(&Project)`; `server` syncs every 60 seconds and after each change.

- [ ] **Step 1: Failing test** (`tests/cli.rs`):

```rust
#[test]
fn changes_sync_by_themselves() {
    let t = T::new();
    let url = t.dir.path().join("remote.git");
    assert!(Command::new("git").args(["init", "--quiet", "--bare", "-b", "main"]).arg(&url).status().unwrap().success());
    t.out(&["remote", "set", &url.to_string_lossy()]);
    t.out(&["add", "TODO", "Gökçe"]);
    let log = Command::new("git").arg("--git-dir").arg(&url).args(["log", "--format=%s", "main"]).output().unwrap();
    assert!(String::from_utf8_lossy(&log.stdout).contains("TODO:1: add \"Gökçe\""));
}
```

- [ ] **Step 2:** run, expect failure.

- [ ] **Step 3: CLI.** After a successful change command (the same set as `history_notice`, plus `Resolve` from Task 9):

```rust
/// Syncs after a change when the project has a remote and sync is not
/// manual. Never fails the command: problems are said on stderr.
fn auto_sync(p: &Project) {
    let remote = crate::history::History::open(p).ok().flatten().and_then(|h| h.remote().ok().flatten());
    if remote.is_some_and(|r| !r.manual) {
        match crate::sync::sync(p) {
            Ok(r) => print_sync(&r, false),
            Err(e) => eprintln!("primum: sync: {e:#}"),
        }
    }
}
```

`dispatch` keeps the resolved project of a change command (`let mut changed: Option<Project> = None;` set by those arms) and calls `auto_sync` on it after the `match`.

- [ ] **Step 4: server.** In `serve`, before `axum::serve`:

```rust
let projects: Vec<Project> = entries.iter().filter_map(|e| e.project.clone().ok()).collect();
tokio::spawn(async move {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        tick.tick().await;
        for p in projects.clone() {
            let _ = tokio::task::spawn_blocking(move || background_sync(&p)).await;
        }
    }
});
```

```rust
/// A sync the page did not ask for: only when there is a remote and sync
/// is automatic; problems go to the server's stderr.
fn background_sync(p: &Project) {
    let auto = crate::history::History::open(p)
        .ok()
        .flatten()
        .and_then(|h| h.remote().ok().flatten())
        .is_some_and(|r| !r.manual);
    if auto {
        match crate::sync::sync(p) {
            Ok(r) => {
                if let Some(why) = r.offline { eprintln!("primum: {}: offline ({why})", p.name); }
                for id in r.conflicts { eprintln!("primum: {}: {id} has a conflict", p.name); }
            }
            Err(e) => eprintln!("primum: {}: sync: {e:#}", p.name),
        }
    }
}
```

Each POST handler (`api_update`, `api_kind`, `api_new`, `api_delete`, and `api_resolve` in Task 9) ends with `let q = p.clone(); tokio::task::spawn_blocking(move || background_sync(&q));` (not awaited: the answer does not wait for the network).

- [ ] **Step 5: the page polls.** `board.js`: `setInterval(()=>{ if(document.visibilityState==='visible' && !document.activeElement.closest('input,select,textarea')) load(); }, 60000);`. `item.js`: the same, skipped while editing (`!$('#editor').classList.contains('hide')`).
- [ ] **Step 6:** `cargo test`: all pass. `make fmt && make lint-check`.
- [ ] **Step 7:** commit "Sync by itself: after changes, and every minute while serving".

---

### Task 9: Resolving conflicts

**Files:** Modify `src/store.rs` (`resolve`), `src/cli.rs` (`resolve`, list marker), `src/server.rs` (`/api/resolve`), `static/item.js`, `static/board.js`, `static/primum.css`, `tests/store.rs`, `tests/server.rs`, `e2e/support/primum.ts`, create `e2e/tests/history.spec.ts`.

**Interfaces:**
- Produces: `store::Keep { First, Second, Both }`; `store::resolve(p, id, what: &str, keep: Keep) -> Result<()>` where `what` is a field name, `body`, or a document path; route `POST /p/{project}/api/resolve {id, field, keep: 1|2|"both"}`; CLI `primum resolve ID [--field F --keep 1|2|both]`.

- [ ] **Step 1: Failing test** (`tests/store.rs`):

```rust
#[test]
fn resolving_writes_the_choice_and_clears_the_record() {
    use primum::conflicts::*;
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "x", Changes { body: Some("bizim\n".into()), ..ch() }).unwrap();
    set_doc(&p, "TODO:1", "spec", "# bizim\n").unwrap();
    std::fs::write(d.path().join("items/TODO-1.spec.abc1234.md"), "# onların\n").unwrap();
    let side = |a: &str| Side { author: a.into(), date: String::new() };
    write(&p, &Conflict {
        id: "TODO:1".into(),
        sides: [side("Özgür"), side("Işıl")],
        fields: vec![FieldValues { field: "impact".into(), values: ["5".into(), "8".into()] }],
        body: Some(BodyValues { values: ["bizim\n".into(), "onların\n".into()] }),
        documents: vec![DocConflict { path: "items/TODO-1.spec.md".into(), other: "items/TODO-1.spec.abc1234.md".into() }],
    }).unwrap();
    resolve(&p, "TODO:1", "impact", Keep::Second).unwrap();
    assert_eq!(load(&p).unwrap().get("TODO:1").unwrap().impact, 8);
    resolve(&p, "TODO:1", "body", Keep::First).unwrap();
    resolve(&p, "TODO:1", "items/TODO-1.spec.md", Keep::Second).unwrap();
    assert_eq!(common::read(d.path(), "items/TODO-1.spec.md"), "# onların\n");
    assert!(!d.path().join("items/TODO-1.spec.abc1234.md").exists());
    assert!(!d.path().join("conflicts/TODO-1.json").exists(), "empty record removed");
    assert_eq!(common::subjects(&p)[0], "TODO:1: resolve items/TODO-1.spec.md");
    assert!(resolve(&p, "TODO:1", "impact", Keep::First).is_err(), "nothing left to resolve");
}
```

- [ ] **Step 2:** run, expect failure.

- [ ] **Step 3: `store::resolve`**

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Keep {
    First,
    Second,
    Both,
}

/// Settles one conflicting field, the body, or a document of `id` with the
/// chosen side, and removes it from the item's conflict record.
pub fn resolve(p: &Project, id: &str, what: &str, keep: Keep) -> Result<()> {
    change(p, |board| {
        let mut c = board
            .conflicts
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("{id} has no conflict"))?;
        let pick = |values: &[String; 2]| -> Result<String> {
            match keep {
                Keep::First => Ok(values[0].clone()),
                Keep::Second => Ok(values[1].clone()),
                Keep::Both => bail!("keep 1 or 2 for {what}"),
            }
        };
        if what == "body" {
            let b = c.body.take().ok_or_else(|| anyhow!("{id} has no body conflict"))?;
            let body = pick(&b.values)?;
            update_locked(p, board, id, Changes { body: Some(body), ..Default::default() })?;
        } else if let Some(i) = c.documents.iter().position(|d| d.path == what) {
            let d = c.documents.remove(i);
            match keep {
                Keep::First => fs::remove_file(p.root.join(&d.other))?,
                Keep::Second => fs::rename(p.root.join(&d.other), p.root.join(&d.path))?,
                Keep::Both => {}
            }
        } else {
            let i = c
                .fields
                .iter()
                .position(|f| f.field == what)
                .ok_or_else(|| anyhow!("{id} has no conflict on {what}"))?;
            let f = c.fields.remove(i);
            let v = pick(&f.values)?;
            let num = || v.parse::<i64>().map_err(|_| anyhow!("{v} is not a score"));
            let mut ch = Changes::default();
            match what {
                "title" => ch.title = Some(v.clone()),
                "status" => ch.status = Some(v.clone()),
                "impact" => ch.impact = Some(num()?),
                "urgency" => ch.urgency = Some(num()?),
                "risk" => ch.risk = Some(num()?),
                "effort" => ch.effort = Some(num()?),
                "source" => {
                    let it = board.get_mut(id)?;
                    it.source = v.clone();
                    let it = it.clone();
                    save(p, &it)?;
                }
                other => bail!("cannot resolve {other}"),
            }
            if what != "source" {
                update_locked(p, board, id, ch)?;
            }
        }
        crate::conflicts::write(p, &c)?;
        settle_locked(p)?;
        Ok(((), format!("{id}: resolve {what}")))
    })
}
```

- [ ] **Step 4: CLI.** `Resolve { id: String, #[arg(long)] field: Option<String>, #[arg(long)] keep: Option<String> }`. With `--field` and `--keep` (`1`, `2`, `both`): one call to `store::resolve`, printing `ID: resolved FIELD`. Without them on a terminal: for each field, the body and each document, print

```
TODO:1 status
  1) done     Özgür, 2026-10-14 09:12
  2) wontfix  Işıl, 2026-10-14 09:15
keep [1/2]:
```

and resolve with the answer (documents also offer `b` for both). Without them and not on a terminal: print the conflict as above and exit 1 with `primum: choose with --field F --keep 1|2`. `primum list` appends `!` to the status of an item with a conflict (`open!`), and its JSON carries `conflict`.

- [ ] **Step 5: API.**

```rust
.route("/p/{project}/api/resolve", post(api_resolve))

async fn api_resolve(
    State(s): State<Shared>,
    Path(project): Path<String>,
    axum::Json(v): axum::Json<Value>,
) -> Result<axum::Json<Value>, ApiError> {
    let p = s.project(&project)?;
    let id = id_field(&v)?;
    let what = text_field(&v, "field")?.ok_or_else(|| bad("field required"))?;
    let keep = match v.get("keep") {
        Some(Value::Number(n)) if n.as_u64() == Some(1) => store::Keep::First,
        Some(Value::Number(n)) if n.as_u64() == Some(2) => store::Keep::Second,
        Some(Value::String(s)) if s == "both" => store::Keep::Both,
        _ => return Err(bad("keep must be 1, 2 or \"both\"")),
    };
    store::resolve(p, &id, &what, keep)?;
    let q = p.clone();
    tokio::task::spawn_blocking(move || background_sync(&q));
    Ok(axum::Json(json!({ "ok": true })))
}
```

Server test (`tests/server.rs`): write a conflict file for `BUG:1` with an `effort` conflict `["5","2"]`, `GET /api/items` shows `conflict.fields[0].field == "effort"`, `POST /api/resolve {"id":"BUG:1","field":"effort","keep":2}` answers 200 and the item's effort is 2, `keep: 3` answers 400.

- [ ] **Step 6: the page.** `item.html` gains `<div class="conflicts hide" id="conflicts"></div>` above `#controls`. `item.js` `draw()` renders `IT.conflict` there:

```js
function drawConflicts(){
  const c=IT.conflict, box=$('#conflicts');
  box.classList.toggle('hide',!c);
  if(!c)return;
  const who=i=>`${esc(c.sides[i].author)}, ${esc((c.sides[i].date||'').slice(0,16).replace('T',' '))}`;
  const row=(what,label,a,b,both)=>`<div class=conflict data-what="${esc(what)}"><b>${esc(label)}</b>
    <button type=button data-keep=1>keep: ${a} <small>${who(0)}</small></button>
    <button type=button data-keep=2>keep: ${b} <small>${who(1)}</small></button>
    ${both?'<button type=button data-keep=both>keep both</button>':''}</div>`;
  box.innerHTML='<h3>Changed on two sides</h3>'
    +c.fields.map(f=>row(f.field,f.field,esc(f.values[0]),esc(f.values[1]),false)).join('')
    +(c.body?`<div class=conflict data-what=body><b>text</b><div class=bodies><div class=md>${md(c.body.values[0])}</div><div class=md>${md(c.body.values[1])}</div></div>
       <button type=button data-keep=1>keep the first <small>${who(0)}</small></button><button type=button data-keep=2>keep the second <small>${who(1)}</small></button></div>`:'')
    +c.documents.map(d=>row(d.path,d.path,'this',esc(d.other.split('/').pop()),true)).join('');
  box.querySelectorAll('button[data-keep]').forEach(b=>b.onclick=async()=>{
    const what=b.closest('.conflict').dataset.what, k=b.dataset.keep;
    try{await api('/resolve',{id:IT.id,field:what,keep:k==='both'?'both':+k});}catch(e){alert(e.message);return;}
    await load();
  });
}
```

CSS:

```css
.conflicts{border:1px solid var(--warn);border-radius:10px;padding:10px 14px;margin:0 0 14px;background:var(--surface)}
.conflicts h3{font-size:14px;margin:0 0 8px;color:var(--warn)}
.conflict{display:flex;gap:8px;flex-wrap:wrap;align-items:center;margin:6px 0}
.conflict small{color:var(--mut)}
.conflict .bodies{display:grid;grid-template-columns:1fr 1fr;gap:12px;width:100%}
.conflict .bodies .md{border:1px solid var(--line);border-radius:6px;padding:8px;font-size:13px}
.conflict-mark{color:var(--warn);font-weight:700;margin-left:4px}
```

`board.js`: the title cell appends `<span class=conflict-mark title="changed on two sides">!</span>` when `i.conflict`.

- [ ] **Step 7: Playwright.** `e2e/support/primum.ts` gains, in `makeBoard`, a second machine making a conflict: a bare remote in the temp dir; `primum remote set <remote> --manual` in `işler`; `primum sync`; `primum clone <remote> kopya` with its own `XDG_CONFIG_HOME` (`config2`); in the clone `primum set BUG:1 --status wontfix`, `primum sync`; in `işler` `primum set BUG:1 --status done`, `primum sync` (this records the conflict). `e2e/tests/history.spec.ts`:

```ts
import { expect, test } from "../support/primum";

test("the History tab lists an item's changes with their diffs", async ({ page, board }) => {
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/BUG-1#history`);
  await expect(page.locator("#doctabs button.on")).toHaveText(/history/);
  const first = page.locator("ol.history details").first();
  await expect(first.locator("summary")).toContainText("BUG:1");
  await first.locator("summary").click();
  await expect(first.locator("pre.diff")).toContainText("status");
});

test("a conflict is resolved from the item page", async ({ page, board }) => {
  await page.goto(board.baseURL);
  await expect(page.locator("tr.item[data-id='BUG:1'] .conflict-mark")).toBeVisible();
  await page.goto(`${board.baseURL}/p/i%C5%9Fler/item/BUG-1`);
  const status = page.locator(".conflict[data-what=status]");
  await expect(status).toContainText("done");
  await expect(status).toContainText("wontfix");
  await status.locator("button[data-keep='2']").click();
  await expect(page.locator("#conflicts")).toBeHidden();
  await expect(page.locator("#controls [data-f=status]")).toHaveValue("wontfix");
  await expect.poll(() => board.item("BUG:1")).toContain("status: wontfix");
});
```

The existing `board.spec.ts` tests still pass with the remote in place (the board's counts and order do not change).

- [ ] **Step 8:** `cargo test`, `make e2e`: all pass. `make fmt && make lint-check`.
- [ ] **Step 9:** commit in two parts: Rust and static ("Resolve conflicts from the terminal and the item page"), then e2e ("Playwright: history and conflicts").

---

### Task 10: Documentation

**Files:** Modify `README.md`, `CLAUDE.md`.

- [ ] **Step 1: README** gains a section **History and sync** after "Many projects": every change is a commit in `.primum/history.git` (no git repository needed); `primum log ID`; `primum remote set URL [--branch B]` and `primum clone URL`; sync is automatic, merges are primum's, and a conflict shows on the board and is settled with `primum resolve ID` or the item page. Note that git 2.28 or later is used when installed, and that everything works without it, minus history.
- [ ] **Step 2: CLAUDE.md** layout gains `src/history.rs`, `src/merge.rs`, `src/conflicts.rs`, `src/sync.rs` with one line each, and a working rule: "Every `git` call goes through `history`."
- [ ] **Step 3:** commit "Document history and sync".

---

## Self-review notes

- Spec decision 1 (bare repo, allowlist, `.primum/.gitignore`): Task 1. Decision 2 (shell out, flags, identity, `PRIMUM_GIT`, history off): Tasks 1-2. Decision 3 (one commit per change, messages, outside edits, start history, init, import): Task 2. Decision 4 (log, API, History tab): Task 3. Decision 5 (remote per machine, branches, clone): Task 6. Decision 6 (automatic sync, offline, retries): Tasks 7-8. Decision 7 (renumber, field merge, delete against edit, documents, BOARD.md): Tasks 4 and 7. Decision 8 (conflict records, board and item page, CLI, BOARD.md list, rename and delete): Tasks 5 and 9. Testing section: Tasks 1-9.
- Names used across tasks: `History::{open, open_with, git, run, run_timed, commit, record_outside, head, log, diff, remote, set_remote, remove_remote, fetch, push, stage, merge_text, side}`, `history::{clone, Entry, Remote, Fetched, Pushed}`, `store::{change, lock, settle_locked, move_locked, resolve, Keep}`, `Changes::fields`, `merge::{merge_item, Merged, FieldConflict}`, `conflicts::{Conflict, Side, FieldValues, BodyValues, DocConflict, read_all, write, rename, remove}`, `sync::{sync, Report}`, `cli::{auto_sync, print_sync}`, `server::background_sync`.
