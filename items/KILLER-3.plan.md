# GitHub Issues sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A project connected to a GitHub repository keeps its items and that repository's issues in step in both directions, automatically, with GitHub's closing keywords (and primum ids in them) closing items.

**Architecture:** `github` holds the issue types, an `Api` trait with a `gh`-backed implementation and an in-memory one for tests, and the pure mapping between an item and an issue. `github_sync` runs one GitHub sync of a project against any `Api`, using KILLER:2's `merge_item` and conflict records, with the agreed version of each linked item kept in `github/<STEM>.json`. KILLER:2's `sync` calls it between pulling and pushing the history.

**Tech Stack:** Rust 1.96.0, existing crates, the `gh` CLI (`gh api`), Playwright.

**Spec:** `items/KILLER-3.spec.md`. **Requires:** KILLER:2 implemented (`history`, `merge`, `conflicts`, `sync`, `store::{change, move_locked, settle_locked, resolve}`).

## Global Constraints

- Optional: nothing in this plan runs unless `primum.toml` has `[github] repo = "owner/repo"`.
- All GitHub access through `gh api` (binary from `PRIMUM_GH`, default `gh`); primum never reads or stores a token. Without `gh` or a login: `primum: GitHub sync is off: <reason>` once, and everything else works.
- Labels exactly `kind:urgent`, `kind:bug`, `kind:debt`, `kind:killer`, `kind:todo` (colors `B63F3F`, `9A6A16`, `4F6F99`, `3F7E69`, `75689A`) and `doing` (`2A6DB5`).
- primum's block is exactly the spec's form: `<!-- primum` line, `id:`, `impact:`, `urgency:`, `risk:`, `effort:`, `needs: [..]`, `-->`, at the end of the body.
- Statuses: `done` = closed + `state_reason: completed`; `wontfix` = closed + `not_planned`.
- Bases tracked at `github/<STEM>.json`; the history allowlist gains `!/github/`.
- primum's rules: fixtures in Turkish, no em dash in anything public, stage files by name, `make fmt-check` and `make lint-check` clean, no AI attribution in commits.

## File Structure

```
src/github.rs        Issue, IssueWrite, Api, Gh, Memory; issue_write, item_from_issue, block parse/write
src/github_sync.rs   run(project, api) -> Report; bases; keywords; deletes; duplicates
src/item_file.rs     + github field
src/projects.rs      + [github] table; github_repo(&Project)
src/history.rs       + !/github/ in EXCLUDE
src/sync.rs          calls github_sync between pull and push
src/cli.rs           github connect/disconnect/status; issue shown in show
src/server.rs        ItemView.github; repo in page globals
static/item.js, static/board.js, static/md.js   issue links, #45 links
tests/github.rs      tests/github_sync.rs      tests/fixtures/fake-gh (python3)
```

---

### Task 1: The link and the project setting

**Files:** Modify `src/model.rs`, `src/item_file.rs`, `src/projects.rs`, `src/history.rs`, `tests/item_file.rs`, `tests/projects.rs`.

**Interfaces:**
- Produces: `Item.github: Option<u64>`; `item_file` reads and writes `github: N` (after `docs`, before `finished`, only when set); `projects::github_repo(&Project) -> Result<Option<String>>`; `projects::set_github(&Project, Option<&str>) -> Result<()>` (rewrites `primum.toml` keeping `name`).

- [ ] **Step 1: Failing tests.** `tests/item_file.rs`:

```rust
#[test]
fn the_github_link() {
    let mut it = Item::new("TODO", 3, "Gökçe");
    it.github = Some(123);
    let text = write(&it);
    assert!(text.contains("needs: []\ngithub: 123\n---\n"));
    assert_eq!(parse("TODO-3.md", &text).unwrap(), it);
    let e = parse("TODO-3.md", "---\ntitle: x\ngithub: #12\n---\n").unwrap_err();
    assert_eq!(e.to_string(), "TODO-3.md:3: github must be an issue number, not `#12`");
}
```

`tests/projects.rs`:

```rust
#[test]
fn the_github_table() {
    with_config(|d| {
        let dir = d.join("işler");
        std::fs::create_dir_all(&dir).unwrap();
        let p = projects::init(&dir, None).unwrap();
        assert_eq!(projects::github_repo(&p).unwrap(), None);
        projects::set_github(&p, Some("ozgur/isler")).unwrap();
        assert_eq!(projects::github_repo(&p).unwrap().as_deref(), Some("ozgur/isler"));
        assert_eq!(
            std::fs::read_to_string(dir.join("primum.toml")).unwrap(),
            "name = \"işler\"\n\n[github]\nrepo = \"ozgur/isler\"\n"
        );
        assert!(projects::set_github(&p, Some("isler")).is_err(), "owner/repo");
        projects::set_github(&p, None).unwrap();
        assert_eq!(projects::github_repo(&p).unwrap(), None);
    });
}
```

- [ ] **Step 2:** run, expect failures.

- [ ] **Step 3: implement.** `model::Item` gains `pub github: Option<u64>` (`None` in `Item::new`). `item_file::write` adds, after `docs`:

```rust
    if let Some(n) = item.github {
        out += &format!("github: {n}\n");
    }
```

and `set_field` gains:

```rust
        "github" => {
            item.github = Some(raw.parse().map_err(|_| format!("github must be an issue number, not `{raw}`"))?)
        }
```

`projects.rs`: `ProjectFile` becomes

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    github: Option<GithubTable>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GithubTable {
    repo: String,
}

fn read_project_file(p: &Project) -> Result<ProjectFile> {
    let file = p.root.join("primum.toml");
    let text = std::fs::read_to_string(&file)?;
    toml::from_str(&text).with_context(|| format!("{} is not valid", file.display()))
}

pub fn github_repo(p: &Project) -> Result<Option<String>> {
    Ok(read_project_file(p)?.github.map(|g| g.repo))
}

pub fn set_github(p: &Project, repo: Option<&str>) -> Result<()> {
    if let Some(r) = repo {
        let ok = r.split_once('/').is_some_and(|(o, n)| !o.is_empty() && !n.is_empty() && !n.contains('/'));
        if !ok {
            bail!("name the repository as owner/repo, not {r}");
        }
    }
    let mut pf = read_project_file(p)?;
    pf.github = repo.map(|r| GithubTable { repo: r.to_string() });
    std::fs::write(p.root.join("primum.toml"), toml::to_string(&pf)?)?;
    Ok(())
}
```

(`init` keeps writing `ProjectFile { name, github: None }`.) `history::EXCLUDE` gains `!/github/\n`.

- [ ] **Step 4:** `cargo test`: all pass. `make fmt && make lint-check`.
- [ ] **Step 5:** commit "The GitHub link and setting".

---

### Task 2: Issues, the API, and the mapping (`github`)

**Files:** Create `src/github.rs`, `tests/github.rs`. Modify `src/lib.rs`.

**Interfaces:**
- Produces:
  - `Issue { number: u64, title: String, body: String, open: bool, reason: Option<String>, labels: Vec<String>, updated: String, closed: Option<String>, author: String, pr_merged: bool }` (Clone, Debug, PartialEq). `pr_merged` is true only for a merged pull request; plain issues and open PRs are told apart by `is_pr: bool` (add the field).
  - `IssueWrite { title: String, body: String, open: bool, reason: Option<String>, labels: Vec<String> }`.
  - `trait Api { fn issues_since(&self, since: Option<&str>) -> Result<Vec<Issue>>; fn create(&self, w: &IssueWrite) -> Result<Issue>; fn update(&self, number: u64, w: &IssueWrite) -> Result<Issue>; fn comment(&self, number: u64, body: &str) -> Result<()>; fn close(&self, number: u64, reason: &str) -> Result<()>; fn ensure_labels(&self) -> Result<()>; fn whoami(&self) -> Result<String>; }`
  - `Gh { repo: String }` implementing `Api` through `gh api`; `Gh::available() -> Result<(), String>`.
  - `Memory` implementing `Api` in memory: `Memory::new()`, `Memory::issue(&self, n) -> Issue`, `Memory::edit(&self, n, f: impl FnOnce(&mut Issue))` (bumps `updated`), `Memory::open_pr(&self, title, body, merged: bool) -> u64`, `Memory::remove(&self, n)`, `Memory::all(&self) -> Vec<Issue>`. `Memory::update` of a missing issue fails with `HTTP 404`, as `gh` does.
  - `Meta { id: String, impact: u8, urgency: u8, risk: u8, effort: u8, needs: Vec<String> }`; `split_block(body: &str) -> (String, Option<Meta>)`; `issue_write(it: &Item, issue_of: &dyn Fn(&str) -> Option<u64>) -> IssueWrite`; `item_from_issue(issue: &Issue, fallback: &Item) -> Item` (the issue's view of the item: fields GitHub carries from the issue, the rest from `fallback`).
  - `KIND_LABELS: [(&str, &str, &str); 5]` (kind, label, color) and `DOING: (&str, &str)`.

- [ ] **Step 1: Failing tests** `tests/github.rs`:

```rust
use primum::github::*;
use primum::model::Item;

fn item() -> Item {
    let mut it = Item::new("BUG", 4, "Gökçe'nin galerisi boş");
    (it.impact, it.urgency, it.risk, it.effort) = (8, 7, 3, 2);
    it.status = "doing".into();
    it.needs = vec!["TODO:1".into(), "DEBT:2".into()];
    it.body = "Işıl gördü.\n".into();
    it
}

#[test]
fn an_item_as_an_issue() {
    let w = issue_write(&item(), &|id| (id == "TODO:1").then_some(45));
    assert_eq!(w.title, "Gökçe'nin galerisi boş");
    assert!(w.open && w.reason.is_none());
    assert_eq!(w.labels, ["kind:bug", "doing"]);
    assert_eq!(
        w.body,
        "Işıl gördü.\n\nNeeds: #45, DEBT:2\n\n<!-- primum\nid: BUG:4\nimpact: 8\nurgency: 7\nrisk: 3\neffort: 2\nneeds: [TODO:1, DEBT:2]\n-->\n"
    );
    let mut done = item();
    done.status = "wontfix".into();
    let w = issue_write(&done, &|_| None);
    assert_eq!((w.open, w.reason.as_deref()), (false, Some("not_planned")));
    assert_eq!(w.labels, ["kind:bug"]);
}

#[test]
fn an_issue_as_an_item() {
    let w = issue_write(&item(), &|_| None);
    let issue = Issue {
        number: 9, title: w.title.clone(), body: w.body.clone(), open: true, reason: None,
        labels: w.labels.clone(), updated: "2026-10-14T09:00:00Z".into(), closed: None,
        author: "isil".into(), is_pr: false, pr_merged: false,
    };
    let back = item_from_issue(&issue, &Item::new("TODO", 1, ""));
    let mut want = item();
    want.github = Some(9);
    assert_eq!(back, want);
    let mut closed = issue.clone();
    closed.open = false;
    closed.reason = Some("completed".into());
    closed.closed = Some("2026-10-15T10:00:00Z".into());
    closed.labels = vec!["kind:killer".into()];
    let b = item_from_issue(&closed, &Item::new("TODO", 1, ""));
    assert_eq!((b.status.as_str(), b.finished.as_deref(), b.kind.as_str()), ("done", Some("2026-10-15"), "KILLER"));
}

#[test]
fn a_plain_issue_and_a_broken_block() {
    let issue = Issue {
        number: 3, title: "Özgür'ün isteği".into(), body: "Sadece metin.".into(), open: true,
        reason: None, labels: vec![], updated: String::new(), closed: None, author: "ozgur".into(),
        is_pr: false, pr_merged: false,
    };
    let it = item_from_issue(&issue, &Item::new("TODO", 7, ""));
    assert_eq!((it.kind.as_str(), it.impact, it.body.as_str()), ("TODO", 5, "Sadece metin.\n"));
    let (text, meta) = split_block("metin\n\n<!-- primum\nid: TODO:x\n-->\n");
    assert_eq!((text.as_str(), meta), ("metin\n", None));
}

#[test]
fn memory_is_a_small_github() {
    let m = Memory::new();
    let i = m.create(&issue_write(&item(), &|_| None)).unwrap();
    assert_eq!(i.number, 1);
    m.edit(1, |i| i.title = "yeni".into());
    let pr = m.open_pr("Galeri düzeltmesi", "Fixes BUG:4", true);
    let all = m.issues_since(None).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|x| x.number == pr && x.is_pr && x.pr_merged));
    let later = m.issues_since(Some(&i.updated)).unwrap();
    assert!(later.iter().all(|x| x.updated > i.updated));
}
```

- [ ] **Step 2:** run, expect failure.

- [ ] **Step 3: `src/github.rs`**

```rust
//! GitHub issues: the types, the API (through `gh api`, or in memory for
//! tests), and how an item and an issue map onto each other.
use crate::model::{Item, KINDS, clamp_score, parse_id};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use std::sync::Mutex;

pub const KIND_LABELS: [(&str, &str, &str); 5] = [
    ("URGENT", "kind:urgent", "B63F3F"),
    ("BUG", "kind:bug", "9A6A16"),
    ("DEBT", "kind:debt", "4F6F99"),
    ("KILLER", "kind:killer", "3F7E69"),
    ("TODO", "kind:todo", "75689A"),
];
pub const DOING: (&str, &str) = ("doing", "2A6DB5");

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub open: bool,
    /// `completed` or `not_planned` when closed.
    pub reason: Option<String>,
    pub labels: Vec<String>,
    pub updated: String,
    pub closed: Option<String>,
    pub author: String,
    pub is_pr: bool,
    pub pr_merged: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IssueWrite {
    pub title: String,
    pub body: String,
    pub open: bool,
    pub reason: Option<String>,
    pub labels: Vec<String>,
}

pub trait Api {
    /// Issues and pull requests updated after `since` (all when None).
    fn issues_since(&self, since: Option<&str>) -> Result<Vec<Issue>>;
    fn create(&self, w: &IssueWrite) -> Result<Issue>;
    fn update(&self, number: u64, w: &IssueWrite) -> Result<Issue>;
    fn comment(&self, number: u64, body: &str) -> Result<()>;
    /// Closes an issue with `completed` or `not_planned`, nothing else changed.
    fn close(&self, number: u64, reason: &str) -> Result<()>;
    /// Creates primum's labels that the repository lacks.
    fn ensure_labels(&self) -> Result<()>;
    fn whoami(&self) -> Result<String>;
}

// ------------------------------------------------------------ the block

#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub id: String,
    pub impact: u8,
    pub urgency: u8,
    pub risk: u8,
    pub effort: u8,
    pub needs: Vec<String>,
}

const OPEN: &str = "<!-- primum\n";

/// An issue body's text, and primum's block when there is a valid one.
pub fn split_block(body: &str) -> (String, Option<Meta>) {
    let body = body.replace("\r\n", "\n");
    let Some(at) = body.rfind(OPEN) else {
        return (normal(&body), None);
    };
    let (text, block) = body.split_at(at);
    let text = strip_needs_line(text);
    let inner = block[OPEN.len()..].split("-->").next().unwrap_or("");
    let mut meta = Meta { id: String::new(), impact: 5, urgency: 3, risk: 3, effort: 5, needs: vec![] };
    for line in inner.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim();
        let score = |v: &str| v.parse::<i64>().ok().map(clamp_score);
        match k.trim() {
            "id" => meta.id = v.to_string(),
            "impact" => meta.impact = score(v).unwrap_or(5),
            "urgency" => meta.urgency = score(v).unwrap_or(3),
            "risk" => meta.risk = score(v).unwrap_or(3),
            "effort" => meta.effort = score(v).unwrap_or(5),
            "needs" => {
                meta.needs = v.trim_start_matches('[').trim_end_matches(']').split(',')
                    .map(str::trim).filter(|s| parse_id(s).is_some()).map(str::to_string).collect()
            }
            _ => {}
        }
    }
    let meta = parse_id(&meta.id).is_some().then_some(meta);
    (normal(&text), meta)
}

fn strip_needs_line(text: &str) -> String {
    let t = text.trim_end();
    match t.rsplit_once('\n') {
        Some((before, last)) if last.starts_with("Needs: ") => before.to_string(),
        None if t.starts_with("Needs: ") => String::new(),
        _ => t.to_string(),
    }
}

fn normal(text: &str) -> String {
    let t = text.trim_end();
    if t.is_empty() { String::new() } else { format!("{t}\n") }
}

pub fn issue_write(it: &Item, issue_of: &dyn Fn(&str) -> Option<u64>) -> IssueWrite {
    let mut body = it.body.trim_end().to_string();
    if !it.needs.is_empty() {
        let refs: Vec<String> = it.needs.iter()
            .map(|n| issue_of(n).map(|i| format!("#{i}")).unwrap_or_else(|| n.clone()))
            .collect();
        if !body.is_empty() { body += "\n\n"; }
        body += &format!("Needs: {}", refs.join(", "));
    }
    if !body.is_empty() { body += "\n\n"; }
    body += &format!(
        "{OPEN}id: {}\nimpact: {}\nurgency: {}\nrisk: {}\neffort: {}\nneeds: [{}]\n-->\n",
        it.id, it.impact, it.urgency, it.risk, it.effort, it.needs.join(", ")
    );
    let mut labels = vec![KIND_LABELS.iter().find(|k| k.0 == it.kind).map(|k| k.1).unwrap_or("kind:todo").to_string()];
    if it.status == "doing" {
        labels.push(DOING.0.into());
    }
    let (open, reason) = match it.status.as_str() {
        "done" => (false, Some("completed".to_string())),
        "wontfix" => (false, Some("not_planned".to_string())),
        _ => (true, None),
    };
    IssueWrite { title: it.title.clone(), body, open, reason, labels }
}

/// The item as the issue has it. Kind and number come from the block's id,
/// else the kind label and `fallback`; fields GitHub does not carry come
/// from `fallback`.
pub fn item_from_issue(issue: &Issue, fallback: &Item) -> Item {
    let (text, meta) = split_block(&issue.body);
    let mut it = fallback.clone();
    let label_kind = KIND_LABELS.iter().find(|k| issue.labels.iter().any(|l| l == k.1)).map(|k| k.0);
    if let Some(m) = &meta {
        let (kind, num) = parse_id(&m.id).expect("checked in split_block");
        (it.kind, it.num, it.id) = (kind, num, m.id.clone());
        (it.impact, it.urgency, it.risk, it.effort) = (m.impact, m.urgency, m.risk, m.effort);
        it.needs = m.needs.clone();
    }
    if let Some(k) = label_kind {
        it.kind = k.to_string(); // the label wins: it is what people change on GitHub
    }
    it.title = issue.title.clone();
    it.body = text;
    it.status = match (issue.open, issue.reason.as_deref()) {
        (true, _) if issue.labels.iter().any(|l| l == DOING.0) => "doing",
        (true, _) => "open",
        (false, Some("not_planned")) => "wontfix",
        (false, _) => "done",
    }
    .to_string();
    it.finished = if issue.open { None } else { issue.closed.as_deref().map(|c| c.chars().take(10).collect()) };
    it.github = Some(issue.number);
    debug_assert!(KINDS.contains(&it.kind.as_str()));
    it
}

// ------------------------------------------------------------ gh

pub fn gh_binary() -> String {
    std::env::var("PRIMUM_GH").unwrap_or_else(|_| "gh".into())
}

pub struct Gh {
    pub repo: String,
}

fn issue_from_json(v: &Value) -> Issue {
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    Issue {
        number: v["number"].as_u64().unwrap_or(0),
        title: s("title"),
        body: s("body"),
        open: v["state"] == "open",
        reason: v["state_reason"].as_str().map(str::to_string),
        labels: v["labels"].as_array().map(|a| a.iter().filter_map(|l| l["name"].as_str().map(str::to_string)).collect()).unwrap_or_default(),
        updated: s("updated_at"),
        closed: v["closed_at"].as_str().map(str::to_string),
        author: v["user"]["login"].as_str().unwrap_or_default().to_string(),
        is_pr: v.get("pull_request").is_some_and(|p| !p.is_null()),
        pr_merged: v["pull_request"]["merged_at"].is_string(),
    }
}

impl Gh {
    /// Ok when `gh` runs and is logged in, else why not.
    pub fn available() -> std::result::Result<(), String> {
        let out = Command::new(gh_binary()).args(["auth", "status"]).stdin(Stdio::null()).output()
            .map_err(|_| "gh not found".to_string())?;
        if out.status.success() { Ok(()) } else { Err("gh is not logged in (gh auth login)".into()) }
    }

    fn api(&self, method: &str, path: &str, body: Option<&Value>, paginate: bool) -> Result<Value> {
        let mut c = Command::new(gh_binary());
        c.args(["api", "-X", method, "-H", "Accept: application/vnd.github+json"]);
        if paginate {
            c.args(["--paginate", "--slurp"]);
        }
        c.arg(path).env("GH_PROMPT_DISABLED", "1").stdout(Stdio::piped()).stderr(Stdio::piped());
        if body.is_some() {
            c.args(["--input", "-"]).stdin(Stdio::piped());
        } else {
            c.stdin(Stdio::null());
        }
        let mut child = c.spawn().context("cannot run gh")?;
        if let Some(b) = body {
            use std::io::Write;
            child.stdin.take().expect("piped").write_all(b.to_string().as_bytes())?;
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!("gh api {method} {path}: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        let text = String::from_utf8_lossy(&out.stdout);
        if text.trim().is_empty() { return Ok(Value::Null); }
        Ok(serde_json::from_str(&text)?)
    }

    fn write_json(w: &IssueWrite) -> Value {
        let mut v = json!({ "title": w.title, "body": w.body, "labels": w.labels, "state": if w.open { "open" } else { "closed" } });
        if let Some(r) = &w.reason {
            v["state_reason"] = json!(r);
        }
        v
    }
}

impl Api for Gh {
    fn issues_since(&self, since: Option<&str>) -> Result<Vec<Issue>> {
        let mut path = format!("repos/{}/issues?state=all&per_page=100&sort=updated&direction=asc", self.repo);
        if let Some(s) = since {
            path += &format!("&since={s}");
        }
        let pages = self.api("GET", &path, None, true)?;
        let mut out = vec![];
        for page in pages.as_array().into_iter().flatten() {
            for v in page.as_array().into_iter().flatten() {
                out.push(issue_from_json(v));
            }
        }
        // a PR's merge state is not in the issues list: ask for merged PRs only
        for i in out.iter_mut().filter(|i| i.is_pr && !i.open) {
            let pr = self.api("GET", &format!("repos/{}/pulls/{}", self.repo, i.number), None, false)?;
            i.pr_merged = pr["merged"].as_bool().unwrap_or(false);
        }
        Ok(out)
    }

    fn create(&self, w: &IssueWrite) -> Result<Issue> {
        let mut body = Gh::write_json(w);
        body.as_object_mut().expect("object").remove("state");
        body.as_object_mut().expect("object").remove("state_reason");
        let created = issue_from_json(&self.api("POST", &format!("repos/{}/issues", self.repo), Some(&body), false)?);
        if !w.open { self.update(created.number, w) } else { Ok(created) }
    }

    fn update(&self, number: u64, w: &IssueWrite) -> Result<Issue> {
        let v = self.api("PATCH", &format!("repos/{}/issues/{number}", self.repo), Some(&Gh::write_json(w)), false)?;
        Ok(issue_from_json(&v))
    }

    fn comment(&self, number: u64, body: &str) -> Result<()> {
        self.api("POST", &format!("repos/{}/issues/{number}/comments", self.repo), Some(&json!({ "body": body })), false)?;
        Ok(())
    }

    fn close(&self, number: u64, reason: &str) -> Result<()> {
        let body = json!({ "state": "closed", "state_reason": reason });
        self.api("PATCH", &format!("repos/{}/issues/{number}", self.repo), Some(&body), false)?;
        Ok(())
    }

    fn ensure_labels(&self) -> Result<()> {
        let have = self.api("GET", &format!("repos/{}/labels?per_page=100", self.repo), None, true)?;
        let names: Vec<String> = have.as_array().into_iter().flatten()
            .flat_map(|p| p.as_array().cloned().unwrap_or_default())
            .filter_map(|l| l["name"].as_str().map(str::to_lowercase)).collect();
        let wanted = KIND_LABELS.iter().map(|k| (k.1, k.2)).chain([DOING]);
        for (name, color) in wanted {
            if !names.contains(&name.to_string()) {
                self.api("POST", &format!("repos/{}/labels", self.repo), Some(&json!({ "name": name, "color": color })), false)?;
            }
        }
        Ok(())
    }

    fn whoami(&self) -> Result<String> {
        Ok(self.api("GET", "user", None, false)?["login"].as_str().ok_or_else(|| anyhow!("no login"))?.to_string())
    }
}

// ------------------------------------------------------------ in memory

/// A small GitHub for tests: issues in memory, `updated` a counter.
pub struct Memory {
    state: Mutex<(Vec<Issue>, u64)>,
}

impl Default for Memory {
    fn default() -> Self { Memory::new() }
}

impl Memory {
    pub fn new() -> Memory {
        Memory { state: Mutex::new((vec![], 0)) }
    }

    fn tick(clock: &mut u64) -> String {
        *clock += 1;
        format!("2026-10-14T00:00:{:02}Z", *clock)
    }

    pub fn issue(&self, n: u64) -> Issue {
        self.state.lock().expect("lock").0.iter().find(|i| i.number == n).cloned().expect("no such issue")
    }

    pub fn all(&self) -> Vec<Issue> {
        self.state.lock().expect("lock").0.clone()
    }

    /// An edit made on GitHub.
    pub fn edit(&self, n: u64, f: impl FnOnce(&mut Issue)) {
        let mut s = self.state.lock().expect("lock");
        let (issues, clock) = &mut *s;
        let i = issues.iter_mut().find(|i| i.number == n).expect("no such issue");
        f(i);
        i.updated = Memory::tick(clock);
        if !i.open && i.closed.is_none() {
            i.closed = Some(i.updated.clone());
        }
        if i.open {
            i.closed = None;
        }
    }

    /// An issue deleted on GitHub (by an admin, or a transferred repository).
    pub fn remove(&self, n: u64) {
        self.state.lock().expect("lock").0.retain(|i| i.number != n);
    }

    pub fn open_pr(&self, title: &str, body: &str, merged: bool) -> u64 {
        let mut s = self.state.lock().expect("lock");
        let (issues, clock) = &mut *s;
        let n = issues.len() as u64 + 1;
        let t = Memory::tick(clock);
        issues.push(Issue {
            number: n, title: title.into(), body: body.into(), open: !merged, reason: None, labels: vec![],
            updated: t.clone(), closed: merged.then_some(t), author: "gokce".into(), is_pr: true, pr_merged: merged,
        });
        n
    }

    fn put(&self, number: Option<u64>, w: &IssueWrite) -> Issue {
        let mut s = self.state.lock().expect("lock");
        let (issues, clock) = &mut *s;
        let updated = Memory::tick(clock);
        let n = number.unwrap_or(issues.len() as u64 + 1);
        let closed = (!w.open).then(|| updated.clone());
        let issue = Issue {
            number: n, title: w.title.clone(), body: w.body.clone(), open: w.open, reason: w.reason.clone(),
            labels: w.labels.clone(), updated, closed, author: "primum".into(), is_pr: false, pr_merged: false,
        };
        issues.retain(|i| i.number != n);
        issues.push(issue.clone());
        issues.sort_by_key(|i| i.number);
        issue
    }
}

impl Api for Memory {
    fn issues_since(&self, since: Option<&str>) -> Result<Vec<Issue>> {
        Ok(self.all().into_iter().filter(|i| since.is_none_or(|s| i.updated.as_str() > s)).collect())
    }
    fn create(&self, w: &IssueWrite) -> Result<Issue> { Ok(self.put(None, w)) }
    fn update(&self, number: u64, w: &IssueWrite) -> Result<Issue> {
        if !self.all().iter().any(|i| i.number == number) {
            bail!("gh api PATCH issues/{number}: HTTP 404: Not Found");
        }
        Ok(self.put(Some(number), w))
    }
    fn comment(&self, _: u64, _: &str) -> Result<()> { Ok(()) }
    fn close(&self, number: u64, reason: &str) -> Result<()> {
        self.edit(number, |i| { i.open = false; i.reason = Some(reason.into()); });
        Ok(())
    }
    fn ensure_labels(&self) -> Result<()> { Ok(()) }
    fn whoami(&self) -> Result<String> { Ok("primum".into()) }
}
```

- [ ] **Step 4:** `cargo test --test github`: 4 passed. `make fmt && make lint-check`.
- [ ] **Step 5:** commit "GitHub issues: types, gh, an in-memory GitHub, and the mapping".

---

### Task 3: One GitHub sync (`github_sync`)

**Files:** Create `src/github_sync.rs`, `tests/github_sync.rs`. Modify `src/lib.rs`, `src/store.rs` (a locked insert for an incoming issue; delete reports the removed item's link).

**Interfaces:**
- Consumes: `github::{Api, Issue, issue_write, item_from_issue}`, `merge::merge_item`, `conflicts::{Conflict, FieldValues, BodyValues, Side, read_all, write}`, `store::{load, settle_locked, move_locked}`, `item_file::{write, stem}`.
- Produces: `github_sync::Report { created: Vec<String>, added: Vec<String>, updated_items: Vec<String>, updated_issues: Vec<u64>, closed_by: Vec<(String, u64)>, conflicts: Vec<String> }` (Default, Debug); `github_sync::run(p: &Project, api: &dyn Api, since: Option<&str>) -> Result<(Report, Option<String>)>` (the new `since`: the latest `updated` seen); `github_sync::base_path(p, id) -> PathBuf` (`github/<STEM>.json`); `store::insert_locked(p, it: Item) -> Result<String>` (keeps the given kind, takes the next number, `pub(crate)`). Deleting a linked item records `github/deleted.json` (a list of issue numbers to close), consumed by `run`.

`run` is called with the project lock held (from `sync::sync`). It does not commit; `sync` commits once afterwards with `GitHub sync: …` (Task 5).

- [ ] **Step 1: Failing tests** `tests/github_sync.rs`:

```rust
mod common;
use primum::github::*;
use primum::github_sync::run;
use primum::store::*;

fn ch() -> Changes { Changes::default() }

fn synced() -> (tempfile::TempDir, Project, Memory) {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "BUG", "Gökçe'nin hatası", Changes { body: Some("bir\n".into()), ..ch() }).unwrap();
    insert(&p, "TODO", "Özgür'ün işi", ch()).unwrap();
    let m = Memory::new();
    run(&p, &m, None).unwrap();
    (d, p, m)
}

#[test]
fn items_become_issues_and_keep_their_link() {
    let (_d, p, m) = synced();
    assert_eq!(m.all().len(), 2);
    let b = load(&p).unwrap();
    let n = b.get("BUG:1").unwrap().github.unwrap();
    assert_eq!(m.issue(n).labels, ["kind:bug"]);
    assert!(m.issue(n).body.contains("id: BUG:1"));
    let (r, _) = run(&p, &m, None).unwrap();
    assert!(r.created.is_empty() && r.updated_issues.is_empty(), "nothing changed: {r:?}");
}

#[test]
fn edits_flow_both_ways_and_different_fields_merge() {
    let (_d, p, m) = synced();
    let n = load(&p).unwrap().get("BUG:1").unwrap().github.unwrap();
    update(&p, "BUG:1", Changes { effort: Some(2), ..ch() }).unwrap();
    m.edit(n, |i| i.title = "Işıl'ın başlığı".into());
    run(&p, &m, None).unwrap();
    let it = load(&p).unwrap().get("BUG:1").unwrap().clone();
    assert_eq!((it.title.as_str(), it.effort), ("Işıl'ın başlığı", 2));
    assert!(m.issue(n).body.contains("effort: 2"));
}

#[test]
fn closing_and_reopening_on_github() {
    let (_d, p, m) = synced();
    let n = load(&p).unwrap().get("TODO:1").unwrap().github.unwrap();
    m.edit(n, |i| { i.open = false; i.reason = Some("not_planned".into()); });
    run(&p, &m, None).unwrap();
    assert_eq!(load(&p).unwrap().get("TODO:1").unwrap().status, "wontfix");
    m.edit(n, |i| { i.open = true; i.reason = None; });
    run(&p, &m, None).unwrap();
    let it = load(&p).unwrap().get("TODO:1").unwrap().clone();
    assert_eq!((it.status.as_str(), it.finished), ("open", None));
}

#[test]
fn a_new_issue_becomes_an_item_and_a_kind_label_moves_it() {
    let (_d, p, m) = synced();
    let n = m.create(&IssueWrite { title: "Gökçe'nin isteği".into(), body: "Lütfen.".into(), open: true, reason: None, labels: vec![] }).unwrap().number;
    run(&p, &m, None).unwrap();
    let b = load(&p).unwrap();
    let it = b.items.iter().find(|i| i.github == Some(n)).unwrap();
    assert_eq!((it.id.as_str(), it.body.as_str()), ("TODO:2", "Lütfen.\n"));
    assert!(m.issue(n).body.contains("id: TODO:2"), "the issue gets primum's block");
    m.edit(n, |i| i.labels = vec!["kind:bug".into()]);
    run(&p, &m, None).unwrap();
    assert_eq!(load(&p).unwrap().items.iter().find(|i| i.github == Some(n)).unwrap().id, "BUG:2");
}

#[test]
fn the_same_field_on_both_sides_is_a_conflict_not_pushed() {
    let (_d, p, m) = synced();
    let n = load(&p).unwrap().get("BUG:1").unwrap().github.unwrap();
    update(&p, "BUG:1", Changes { title: Some("burada".into()), ..ch() }).unwrap();
    m.edit(n, |i| i.title = "orada".into());
    let (r, _) = run(&p, &m, None).unwrap();
    assert_eq!(r.conflicts, ["BUG:1"]);
    let c = &load(&p).unwrap().conflicts["BUG:1"];
    assert_eq!(c.fields[0].values, ["burada".to_string(), "orada".to_string()]);
    assert!(c.sides[1].author.starts_with("GitHub"));
    assert_eq!(m.issue(n).title, "orada", "the conflicting field stays as GitHub has it");
}

#[test]
fn a_merged_pull_request_with_a_primum_id_closes_the_item() {
    let (_d, p, m) = synced();
    let pr = m.open_pr("Galeri", "Bu değişiklik Fixes BUG:1 ve fazlası", true);
    let (r, _) = run(&p, &m, None).unwrap();
    assert_eq!(r.closed_by, [("BUG:1".to_string(), pr)]);
    let it = load(&p).unwrap().get("BUG:1").unwrap().clone();
    assert_eq!(it.status, "done");
    assert!(it.body.contains(&format!("closed by #{pr}")));
    assert!(!m.issue(it.github.unwrap()).open);
    m.open_pr("Açık", "Fixes TODO:1", false);
    run(&p, &m, None).unwrap();
    assert_eq!(load(&p).unwrap().get("TODO:1").unwrap().status, "open", "an unmerged PR closes nothing");
}

#[test]
fn deleting_closes_the_issue_and_duplicates_are_linked_not_created() {
    let (_d, p, m) = synced();
    let n = load(&p).unwrap().get("TODO:1").unwrap().github.unwrap();
    delete(&p, "TODO:1").unwrap();
    run(&p, &m, None).unwrap();
    assert_eq!((m.issue(n).open, m.issue(n).reason.as_deref()), (false, Some("not_planned")));
    // another machine already filed TODO:2's issue
    insert(&p, "TODO", "ikinci", ch()).unwrap();
    let twin = issue_write(load(&p).unwrap().get("TODO:2").unwrap(), &|_| None);
    let existing = m.create(&twin).unwrap().number;
    run(&p, &m, None).unwrap();
    assert_eq!(load(&p).unwrap().get("TODO:2").unwrap().github, Some(existing));
    assert_eq!(m.all().iter().filter(|i| i.body.contains("id: TODO:2")).count(), 1);
}

#[test]
fn an_issue_that_disappears_leaves_its_item_unlinked() {
    let (_d, p, m) = synced();
    let n = load(&p).unwrap().get("BUG:1").unwrap().github.unwrap();
    m.remove(n);
    update(&p, "BUG:1", Changes { risk: Some(9), ..ch() }).unwrap();
    run(&p, &m, None).unwrap();
    let it = load(&p).unwrap().get("BUG:1").unwrap().clone();
    assert_eq!((it.github, it.risk), (None, 9));
}
```

- [ ] **Step 2:** run, expect failure.

- [ ] **Step 3: `src/github_sync.rs`**

```rust
//! One GitHub sync of one project, against any `Api`. Called by `sync`
//! with the project lock held; `sync` commits afterwards.
//!
//! For each linked item, `github/<STEM>.json` is the version both sides
//! last agreed on: the base of a three-way merge between the item and its
//! issue (KILLER:2's merge_item). A field both changed is a conflict, and
//! is left as GitHub has it until resolved.
use crate::conflicts::{self, BodyValues, Conflict, FieldValues, Side};
use crate::github::{Api, Issue, issue_write, item_from_issue};
use crate::item_file::{self, stem};
use crate::merge::merge_item;
use crate::model::Item;
use crate::store::{self, Project};
use anyhow::Result;
use regex::Regex;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::LazyLock;

#[derive(Debug, Default)]
pub struct Report {
    /// Items that became issues.
    pub created: Vec<String>,
    /// Issues that became items.
    pub added: Vec<String>,
    pub updated_items: Vec<String>,
    pub updated_issues: Vec<u64>,
    /// (item, pull request) closed by a primum id in a merged PR.
    pub closed_by: Vec<(String, u64)>,
    pub conflicts: Vec<String>,
}

static CLOSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s*:?\s+((?:URGENT|BUG|DEBT|KILLER|TODO):\d+)\b").expect("regex")
});

pub fn base_path(p: &Project, id: &str) -> PathBuf {
    p.root.join("github").join(format!("{}.json", stem(id)))
}

fn deleted_path(p: &Project) -> PathBuf {
    p.root.join("github").join("deleted.json")
}

/// The agreed version, stored as the item's own file text so it parses
/// with the same rules.
fn read_base(p: &Project, id: &str) -> Option<Item> {
    let text = std::fs::read_to_string(base_path(p, id)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    item_file::parse(&item_file::file_name(id), v["item"].as_str()?).ok()
}

fn write_base(p: &Project, it: &Item) -> Result<()> {
    std::fs::create_dir_all(p.root.join("github"))?;
    let v = serde_json::json!({ "item": item_file::write(it) });
    std::fs::write(base_path(p, &it.id), serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

/// Only the fields GitHub carries take part in the comparison.
fn carried(it: &Item) -> (String, String, String, String, [u8; 4], Vec<String>) {
    (it.title.clone(), it.body.clone(), it.status.clone(), it.kind.clone(), [it.impact, it.urgency, it.risk, it.effort], it.needs.clone())
}

pub fn run(p: &Project, api: &dyn Api, since: Option<&str>) -> Result<(Report, Option<String>)> {
    let mut report = Report::default();
    let incoming = api.issues_since(since)?;
    let newest = incoming.iter().map(|i| i.updated.clone()).max().or(since.map(str::to_string));

    // 1. issues closed for deleted items
    if let Ok(text) = std::fs::read_to_string(deleted_path(p)) {
        let numbers: Vec<u64> = serde_json::from_str(&text).unwrap_or_default();
        for n in numbers {
            api.comment(n, "Deleted in primum.")?;
            api.close(n, "not_planned")?;
        }
        std::fs::remove_file(deleted_path(p))?;
    }

    // 2. merged pull requests naming primum ids
    for pr in incoming.iter().filter(|i| i.is_pr && i.pr_merged) {
        for cap in CLOSING.captures_iter(&format!("{}\n{}", pr.title, pr.body)) {
            let id = cap[1].to_string();
            let board = store::load(p)?;
            if board.get(&id).is_some_and(|i| !i.is_closed()) {
                let note = format!("## {}\n\nclosed by #{}\n", chrono::Local::now().format("%Y-%m-%d"), pr.number);
                let it = board.get(&id).expect("checked").clone();
                let body = if it.body.trim().is_empty() { note } else { format!("{}\n\n---\n\n{note}", it.body.trim_end()) };
                store::update_locked_by_id(p, &id, store::Changes { status: Some("done".into()), body: Some(body), ..Default::default() })?;
                report.closed_by.push((id, pr.number));
            }
        }
    }

    // 3. issues into items
    let mut board = store::load(p)?;
    let by_number: BTreeMap<u64, String> = board.items.iter().filter_map(|i| i.github.map(|n| (n, i.id.clone()))).collect();
    for issue in incoming.iter().filter(|i| !i.is_pr) {
        match by_number.get(&issue.number) {
            Some(id) => merge_issue(p, api, id, issue, &mut report)?,
            None => {
                let (_, meta) = crate::github::split_block(&issue.body);
                let unlinked = meta.as_ref().and_then(|m| board.get(&m.id)).filter(|i| i.github.is_none()).map(|i| i.id.clone());
                if let Some(id) = unlinked {
                    // another machine filed it: link it instead of filing a second one
                    link(p, &id, issue)?;
                } else if meta.as_ref().is_none_or(|m| board.get(&m.id).is_none()) {
                    let fallback = Item::new("TODO", 0, "");
                    let mut it = item_from_issue(issue, &fallback);
                    it.source = format!("GitHub #{}", issue.number);
                    let id = store::insert_locked(p, it)?;
                    let it = store::load(p)?.get(&id).expect("inserted").clone();
                    let w = issue_write(&it, &|_| None);
                    api.update(issue.number, &w)?;
                    write_base(p, &it)?;
                    report.added.push(id);
                }
            }
        }
        board = store::load(p)?;
    }

    // 4. items into issues
    let issue_of = |b: &store::Board, id: &str| b.get(id).and_then(|i| i.github);
    for it in board.items.clone() {
        if it.github.is_none() {
            if it.is_closed() { continue; }
            let w = issue_write(&it, &|id| issue_of(&board, id));
            let created = api.create(&w)?;
            link(p, &it.id, &created)?;
            report.created.push(it.id.clone());
        } else if read_base(p, &it.id).is_some_and(|b| carried(&b) != carried(&it))
            && !board.conflicts.contains_key(&it.id)
        {
            let n = it.github.expect("linked");
            if push(p, api, &it, n, &issue_write(&it, &|id| issue_of(&board, id)))? {
                report.updated_issues.push(n);
            }
        }
    }
    store::settle_locked(p)?;
    Ok((report, newest))
}

/// Writes the link into the item and records the agreed version.
fn link(p: &Project, id: &str, issue: &Issue) -> Result<()> {
    let board = store::load(p)?;
    let mut it = board.get(id).expect("linked item exists").clone();
    it.github = Some(issue.number);
    std::fs::write(p.items_dir().join(item_file::file_name(id)), item_file::write(&it))?;
    write_base(p, &it)
}

fn merge_issue(p: &Project, api: &dyn Api, id: &str, issue: &Issue, report: &mut Report) -> Result<()> {
    let board = store::load(p)?;
    let ours = board.get(id).expect("linked").clone();
    let theirs = item_from_issue(issue, &ours);
    let base = read_base(p, id);
    if carried(&ours) == carried(&theirs) {
        return write_base(p, &ours);
    }
    let text = |b: &str, o: &str, t: &str| -> Option<String> { (o == b).then(|| t.to_string()).or_else(|| (t == b).then(|| o.to_string())) };
    let m = merge_item(base.as_ref(), &ours, &theirs, &text);
    let mut item = m.item.clone();
    // kind: changed on GitHub only moves the item
    let base_kind = base.as_ref().map(|b| b.kind.clone()).unwrap_or_else(|| ours.kind.clone());
    let mut id = id.to_string();
    if theirs.kind != ours.kind && ours.kind == base_kind {
        let num = store::load(p)?.items.iter().filter(|i| i.kind == theirs.kind).map(|i| i.num).max().unwrap_or(0) + 1;
        id = store::move_locked(p, &id, &theirs.kind, num)?;
        item = store::load(p)?.get(&id).expect("moved").clone().with_fields_of(&item);
    }
    std::fs::write(p.items_dir().join(item_file::file_name(&id)), item_file::write(&item))?;
    if !m.fields.is_empty() || m.body.is_some() {
        conflicts::write(p, &Conflict {
            id: id.clone(),
            sides: [Side { author: "this board".into(), date: String::new() }, Side { author: format!("GitHub ({})", issue.author), date: issue.updated.clone() }],
            fields: m.fields.into_iter().map(|f| FieldValues { field: f.field, values: f.values }).collect(),
            body: m.body.map(|values| BodyValues { values }),
            documents: vec![],
        })?;
        report.conflicts.push(id.clone());
    } else {
        let board = store::load(p)?;
        if push(p, api, &item, issue.number, &issue_write(&item, &|n| board.get(n).and_then(|i| i.github)))? {
            report.updated_issues.push(issue.number);
        }
    }
    write_base(p, &item)?;
    report.updated_items.push(id);
    Ok(())
}

/// Writes an item's issue and its agreed version. An issue that no longer
/// exists (deleted on GitHub) leaves the item without its link; returns
/// false then.
fn push(p: &Project, api: &dyn Api, it: &Item, n: u64, w: &crate::github::IssueWrite) -> Result<bool> {
    match api.update(n, w) {
        Ok(_) => {
            write_base(p, it)?;
            Ok(true)
        }
        Err(e) if e.to_string().contains("404") => {
            let mut unlinked = it.clone();
            unlinked.github = None;
            std::fs::write(p.items_dir().join(item_file::file_name(&it.id)), item_file::write(&unlinked))?;
            let _ = std::fs::remove_file(base_path(p, &it.id));
            Ok(false)
        }
        Err(e) => Err(e),
    }
}
```

Supporting changes in `store.rs`:

```rust
/// Inserts an item that came from elsewhere (a GitHub issue), keeping its
/// kind and fields and giving it the next number there. No lock: the caller
/// holds it.
pub(crate) fn insert_locked(p: &Project, mut it: Item) -> Result<String> {
    let board = load(p)?;
    it.num = board.next_num(&it.kind);
    it.id = id_of(&it.kind, it.num);
    save(p, &it)?;
    settle_locked(p)?;
    Ok(it.id)
}

/// `update` without the lock or a commit, for a caller holding the lock.
pub(crate) fn update_locked_by_id(p: &Project, id: &str, c: Changes) -> Result<()> {
    let mut board = load(p)?;
    update_locked(p, &mut board, id, c)
}
```

`model::Item` gains a helper used after a move, copying every field but id, kind and num:

```rust
impl Item {
    /// This item's id, kind and number with `other`'s fields.
    pub fn with_fields_of(&self, other: &Item) -> Item {
        Item { id: self.id.clone(), kind: self.kind.clone(), num: self.num, ..other.clone() }
    }
}
```

`store::delete`, when the item had `github: N`, appends `N` to `github/deleted.json` (a JSON array) and removes `github/<STEM>.json`; `move_locked` renames `github/<STEM>.json` like the conflict file.

- [ ] **Step 4:** `cargo test --test github_sync`: 8 passed. `make fmt && make lint-check`.
- [ ] **Step 5:** commit "One GitHub sync: issues and items in step".

---

### Task 4: Wiring: the commands, sync, and `gh`

**Files:** Modify `src/cli.rs`, `src/sync.rs`, `src/history.rs` (`github-since` config), create `tests/fixtures/fake-gh`, modify `tests/cli.rs`.

**Interfaces:**
- Consumes: `github::{Gh, Api}`, `github_sync::run`, `projects::{github_repo, set_github}`.
- Produces: CLI `primum github connect OWNER/REPO [--all]`, `primum github disconnect`, `primum github status`; `sync::Report.github: Option<github_sync::Report>` and `sync::Report.github_off: Option<String>`.

- [ ] **Step 1: `sync.rs`.** In `sync`, after a successful fetch and pull and before the push loop's push (and also when there is no remote: GitHub sync does not need one):

```rust
    if let Some(repo) = crate::projects::github_repo(p)? {
        match crate::github::Gh::available() {
            Err(why) => report.github_off = Some(why),
            Ok(()) => {
                let api = crate::github::Gh { repo };
                let since = h.run(&["config", "--get", "primum.github-since"]).ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
                let (gr, newest) = crate::github_sync::run(p, &api, since.as_deref())?;
                if let Some(n) = newest {
                    h.git(&["config", "primum.github-since", &n])?;
                }
                let what = gr.created.len() + gr.added.len() + gr.updated_items.len() + gr.updated_issues.len() + gr.closed_by.len();
                if what > 0 {
                    h.commit(&format!("GitHub sync: {} new issues, {} new items, {} updated", gr.created.len(), gr.added.len(), gr.updated_items.len() + gr.updated_issues.len()))?;
                }
                report.github = Some(gr);
            }
        }
    }
```

(Restructure `sync` so that without a remote it still runs this block and returns, and `auto_sync`/`background_sync` call `sync` when the project has a remote **or** a `[github]` table.) `print_sync` reports `github_off` once (`primum: GitHub sync is off: …`), each created issue in verbose mode (`TODO:3 -> #12`), each `closed_by` (`primum: BUG:1 closed by #14`), and conflicts as before.

- [ ] **Step 2: CLI.**

```rust
    /// Keep this project's items and a GitHub repository's issues in step
    Github {
        #[command(subcommand)]
        cmd: GithubCmd,
    },

#[derive(Subcommand)]
pub enum GithubCmd {
    /// Connect to OWNER/REPO: open items become issues, issues become items
    Connect {
        repo: String,
        /// Also file closed items as (closed) issues
        #[arg(long)]
        all: bool,
    },
    /// Stop syncing; links in items stay
    Disconnect,
    /// The repository, the last sync, and whether gh is ready
    Status,
}
```

`connect`: `projects::set_github`, `Gh::available()?` (error with the reason), `api.ensure_labels()`, then `sync::sync` and print its report (`--all` sets git config `primum.github-all=true`, read by `github_sync::run` step 4 to also create issues for closed items). `disconnect`: `set_github(None)` and unset `primum.github-since`. `status`: `owner/repo`, `gh ready` or the reason, the `github-since` time, the number of linked items. `show` prints `issue #123  https://github.com/owner/repo/issues/123` when linked.

- [ ] **Step 3: fake `gh`** `tests/fixtures/fake-gh` (python3, executable), state in the JSON file named by `FAKE_GH_STATE`:

```python
#!/usr/bin/env python3
# A fake gh for primum's tests: `gh auth status` and the few `gh api` calls
# primum makes, against a JSON file of issues.
import json, os, sys
state_file = os.environ["FAKE_GH_STATE"]
state = json.load(open(state_file)) if os.path.exists(state_file) else {"issues": [], "labels": [], "clock": 0}
args = sys.argv[1:]
if args[:2] == ["auth", "status"]:
    sys.exit(0)
assert args[0] == "api", args
method = args[args.index("-X") + 1] if "-X" in args else "GET"
path = [a for a in args[1:] if a.startswith("repos/") or a == "user"][0]
body = json.load(sys.stdin) if "--input" in args else None
def tick():
    state["clock"] += 1
    return "2026-10-14T00:00:%02dZ" % state["clock"]
def save(out):
    json.dump(state, open(state_file, "w"))
    print(json.dumps(out))
    sys.exit(0)
parts = path.split("?")[0].split("/")
if path == "user":
    save({"login": "ozgur"})
if parts[3] == "labels":
    if method == "POST":
        state["labels"].append(body); save(body)
    save([state["labels"]])
if parts[3] == "issues" and len(parts) == 4 and method == "GET":
    since = dict(q.split("=") for q in path.split("?")[1].split("&")).get("since")
    save([[i for i in state["issues"] if since is None or i["updated_at"] > since]])
if parts[3] == "issues" and len(parts) == 4 and method == "POST":
    n = len(state["issues"]) + 1
    issue = {"number": n, "title": body["title"], "body": body["body"], "state": "open", "state_reason": None,
             "labels": [{"name": l} for l in body.get("labels", [])], "updated_at": tick(), "closed_at": None,
             "user": {"login": "ozgur"}}
    state["issues"].append(issue); save(issue)
if parts[3] == "issues" and len(parts) == 5 and method == "PATCH":
    issue = [i for i in state["issues"] if i["number"] == int(parts[4])][0]
    for k in ("title", "body", "state", "state_reason"):
        if k in body: issue[k] = body[k]
    if "labels" in body: issue["labels"] = [{"name": l} for l in body["labels"]]
    issue["updated_at"] = tick()
    issue["closed_at"] = issue["updated_at"] if issue["state"] == "closed" else None
    save(issue)
if parts[3] == "issues" and len(parts) == 6 and parts[5] == "comments":
    save({"body": body["body"]})
sys.exit("fake gh: unsupported " + " ".join(args))
```

- [ ] **Step 4: Failing test, then make it pass** (`tests/cli.rs`):

```rust
#[test]
fn github_connect_files_issues_through_gh() {
    let t = T::new();
    t.out(&["add", "BUG", "Gökçe'nin hatası"]);
    let fake = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-gh");
    let state = t.dir.path().join("gh.json");
    let o = Command::new(env!("CARGO_BIN_EXE_primum"))
        .args(["github", "connect", "ozgur/isler"])
        .current_dir(t.dir.path().join("işler"))
        .env("HOME", t.dir.path())
        .env("XDG_CONFIG_HOME", t.dir.path().join("config"))
        .env("PRIMUM_GH", &fake)
        .env("FAKE_GH_STATE", &state)
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(s["issues"][0]["title"], "Gökçe'nin hatası");
    assert!(t.item("BUG:1").contains("github: 1"));
    assert!(std::fs::read_to_string(t.dir.path().join("işler/primum.toml")).unwrap().contains("repo = \"ozgur/isler\""));
}
```

- [ ] **Step 5:** `cargo test`: all pass. `make fmt && make lint-check`.
- [ ] **Step 6:** commit `src/cli.rs src/sync.rs src/history.rs tests/fixtures/fake-gh tests/cli.rs`: "primum github connect, and GitHub in every sync".

---

### Task 5: Issue links on the board, and documentation

**Files:** Modify `src/server.rs`, `static/item.js`, `static/board.js`, `static/md.js`, `static/primum.css`, `README.md`, `CLAUDE.md`, `e2e/tests/board.spec.ts`.

- [ ] **Step 1: server.** `ItemView` gains `github: Option<u64>`; `PageCommon.globals` gains `GITHUB` (`"owner/repo"` or `null`) from `projects::github_repo`.
- [ ] **Step 2: pages.** `item.js` shows, in `#item-meta`, `<a class=issue href="https://github.com/${GITHUB}/issues/${n}" target=_blank rel=noopener>#${n}</a>` and a copy button that copies `Fixes #${n}`. `board.js` adds the same `#n` link after the title when linked. `md.js` `mdInline` turns `#45` (after a space or line start) and `owner/repo#45` into links when `window.GITHUB` is set:

```js
  if(window.GITHUB)s=s.replace(/(^|[\s(])(?:([\w.-]+\/[\w.-]+))?#(\d+)\b/g,(m,pre,repo,n)=>
    `${pre}<a href="https://github.com/${repo||GITHUB}/issues/${n}" target=_blank rel=noopener>${repo?repo:''}#${n}</a>`);
```

(Placed after the code-span extraction, so `#45` inside backticks stays text.) CSS: `.issue{font:12px ui-monospace,Menlo,monospace;margin-left:6px}`.
- [ ] **Step 3: Playwright** (`board.spec.ts`): a project whose `primum.toml` has `[github] repo = "ozgur/isler"` and an item with `github: 7` (written by the support script directly into the item file) shows `#7` linking to `https://github.com/ozgur/isler/issues/7` on the item page.
- [ ] **Step 4: README** section **GitHub Issues** (optional): `primum github connect owner/repo` (needs `gh auth login`); what maps to what (the spec's table, short); closing keywords and `Fixes TODO:87`; scores live in a hidden block at the end of the issue body; `primum github disconnect`. **CLAUDE.md**: `src/github.rs`, `src/github_sync.rs`; "Every GitHub call goes through `github::Api`; tests use `github::Memory`."
- [ ] **Step 5:** `cargo test`, `make e2e`, `make fmt-check`, `make lint-check`: clean.
- [ ] **Step 6:** commit "Issue links on the board; document GitHub sync".

---

## Self-review notes

- Spec decisions: 1 (optional, `[github]`) Task 1 and 4; 2 (`gh`, off without it) Tasks 2 and 4; 3 (mapping, block, labels, needs line) Task 2; 4 (what syncs, when, `since` per machine, `--all`) Tasks 3 and 4; 5 (merge with bases, conflicts not pushed, kind) Task 3; 6 (keywords, `Fixes TODO:87`, `#123` shown and linked) Tasks 3 and 5; 7 (delete closes; a vanished issue unlinks its item on the next push) Task 3; 8 (no duplicates) Task 3; 9 (renumber keeps link) Task 3 (`move_locked` renames the base; the block follows at the next sync because `carried` differs).
- Names across tasks: `Item.github`, `Item::with_fields_of`, `projects::{github_repo, set_github}`, `github::{Issue, IssueWrite, Api, Gh, Memory, Meta, split_block, issue_write, item_from_issue, KIND_LABELS, DOING, gh_binary}`, `github_sync::{run, Report, base_path}`, `store::{insert_locked, update_locked_by_id}`, `sync::Report.{github, github_off}`.
