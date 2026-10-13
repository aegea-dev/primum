//! GitHub issues: the types, the API (through `gh api`, or in memory for
//! tests), and how an item and an issue map onto each other.
use crate::model::{Item, clamp_score, parse_id};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// (kind, label, color) of each lane's label.
pub const KIND_LABELS: [(&str, &str, &str); 5] = [
    ("URGENT", "kind:urgent", "B63F3F"),
    ("BUG", "kind:bug", "9A6A16"),
    ("DEBT", "kind:debt", "4F6F99"),
    ("KILLER", "kind:killer", "3F7E69"),
    ("TODO", "kind:todo", "75689A"),
];
/// The label of an item in progress, and its color.
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
    /// A pull request that was merged.
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
    /// Closes an issue as `completed` or `not_planned`, nothing else changed.
    fn close(&self, number: u64, reason: &str) -> Result<()>;
    /// Creates primum's labels that the repository lacks.
    fn ensure_labels(&self) -> Result<()>;
    /// Every label name in the repository.
    fn labels(&self) -> Result<Vec<String>>;
    fn create_label(&self, name: &str, color: &str) -> Result<()>;
    fn delete_label(&self, name: &str) -> Result<()>;
}

/// How an item's primum id shows on its issue (`[github] ids` in
/// primum.toml). Reading always strips a title prefix, so switching is
/// safe. Ids are never labels: one label per item would bury a
/// repository's label list in hundreds of them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum IdStyle {
    /// Only in primum's hidden block.
    #[default]
    None,
    /// `[TODO:7] Title`.
    Title,
}

impl IdStyle {
    pub fn parse(s: &str) -> Option<IdStyle> {
        match s {
            "none" => Some(IdStyle::None),
            "title" => Some(IdStyle::Title),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            IdStyle::None => "none",
            IdStyle::Title => "title",
        }
    }

    pub fn title(self) -> bool {
        self == IdStyle::Title
    }
}

/// The lane color of a kind, for its labels.
pub fn kind_color(kind: &str) -> &'static str {
    KIND_LABELS
        .iter()
        .find(|k| k.0 == kind)
        .map(|k| k.2)
        .unwrap_or("75689A")
}

/// A title without a leading `[ID] ` that primum wrote.
fn strip_id_prefix(title: &str) -> &str {
    if let Some(rest) = title.strip_prefix('[')
        && let Some((id, after)) = rest.split_once("] ")
        && parse_id(id).is_some()
    {
        return after;
    }
    title
}

// ------------------------------------------------------------ the block

/// What primum keeps in its block at the end of an issue's body.
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

fn normal(text: &str) -> String {
    let t = text.trim_end();
    if t.is_empty() {
        String::new()
    } else {
        format!("{t}\n")
    }
}

/// The text without the visible `Needs:` line primum writes above its block.
fn strip_needs_line(text: &str) -> String {
    let t = text.trim_end();
    match t.rsplit_once('\n') {
        Some((before, last)) if last.starts_with("Needs: ") => before.to_string(),
        None if t.starts_with("Needs: ") => String::new(),
        _ => t.to_string(),
    }
}

/// An issue body's text, and primum's block when there is a valid one.
pub fn split_block(body: &str) -> (String, Option<Meta>) {
    let body = body.replace("\r\n", "\n");
    let Some(at) = body.rfind(OPEN) else {
        return (normal(&body), None);
    };
    let (text, block) = body.split_at(at);
    let text = strip_needs_line(text);
    let inner = block[OPEN.len()..].split("-->").next().unwrap_or("");
    let mut meta = Meta {
        id: String::new(),
        impact: 5,
        urgency: 3,
        risk: 3,
        effort: 5,
        needs: vec![],
    };
    for line in inner.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        let score = |d: u8| v.parse::<i64>().map(clamp_score).unwrap_or(d);
        match k.trim() {
            "id" => meta.id = v.to_string(),
            "impact" => meta.impact = score(5),
            "urgency" => meta.urgency = score(3),
            "risk" => meta.risk = score(3),
            "effort" => meta.effort = score(5),
            "needs" => {
                meta.needs = v
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(str::trim)
                    .filter(|s| parse_id(s).is_some())
                    .map(str::to_string)
                    .collect()
            }
            _ => {}
        }
    }
    let meta = parse_id(&meta.id).is_some().then_some(meta);
    (normal(&text), meta)
}

/// The issue an item is: its text, a visible `Needs:` line, primum's block,
/// its kind and `doing` as labels, its status as open or closed.
pub fn issue_write(it: &Item, issue_of: &dyn Fn(&str) -> Option<u64>) -> IssueWrite {
    issue_write_as(it, issue_of, IdStyle::None)
}

/// `issue_write` with the item's id shown as `style` says.
pub fn issue_write_as(
    it: &Item,
    issue_of: &dyn Fn(&str) -> Option<u64>,
    style: IdStyle,
) -> IssueWrite {
    let mut body = it.body.trim_end().to_string();
    if !it.needs.is_empty() {
        let refs: Vec<String> = it
            .needs
            .iter()
            .map(|n| {
                issue_of(n)
                    .map(|i| format!("#{i}"))
                    .unwrap_or_else(|| n.clone())
            })
            .collect();
        if !body.is_empty() {
            body += "\n\n";
        }
        body += &format!("Needs: {}", refs.join(", "));
    }
    if !body.is_empty() {
        body += "\n\n";
    }
    body += &format!(
        "{OPEN}id: {}\nimpact: {}\nurgency: {}\nrisk: {}\neffort: {}\nneeds: [{}]\n-->\n",
        it.id,
        it.impact,
        it.urgency,
        it.risk,
        it.effort,
        it.needs.join(", ")
    );
    let kind_label = KIND_LABELS
        .iter()
        .find(|k| k.0 == it.kind)
        .map(|k| k.1)
        .unwrap_or("kind:todo");
    let mut labels = vec![kind_label.to_string()];
    if it.status == "doing" {
        labels.push(DOING.0.into());
    }
    let (open, reason) = match it.status.as_str() {
        "done" => (false, Some("completed".to_string())),
        "wontfix" => (false, Some("not_planned".to_string())),
        _ => (true, None),
    };
    let title = if style.title() {
        format!("[{}] {}", it.id, it.title)
    } else {
        it.title.clone()
    };
    IssueWrite {
        title,
        body,
        open,
        reason,
        labels,
    }
}

/// The item as the issue has it. Kind and number come from the block's
/// id, then the kind label (what people change on GitHub) sets the kind;
/// fields GitHub does not carry come from `fallback`.
pub fn item_from_issue(issue: &Issue, fallback: &Item) -> Item {
    let (text, meta) = split_block(&issue.body);
    let mut it = fallback.clone();
    if let Some(m) = &meta {
        let (kind, num) = parse_id(&m.id).expect("checked in split_block");
        (it.kind, it.num, it.id) = (kind, num, m.id.clone());
        (it.impact, it.urgency, it.risk, it.effort) = (m.impact, m.urgency, m.risk, m.effort);
        it.needs = m.needs.clone();
    }
    if let Some(k) = KIND_LABELS
        .iter()
        .find(|k| issue.labels.iter().any(|l| l == k.1))
    {
        it.kind = k.0.to_string();
    }
    it.title = strip_id_prefix(&issue.title).to_string();
    it.body = text;
    let doing = issue.labels.iter().any(|l| l == DOING.0);
    it.status = match (issue.open, issue.reason.as_deref()) {
        (true, _) if doing => "doing",
        (true, _) => "open",
        (false, Some("not_planned")) => "wontfix",
        (false, _) => "done",
    }
    .to_string();
    it.finished = if issue.open {
        None
    } else {
        issue
            .closed
            .as_deref()
            .map(|c| c.chars().take(10).collect())
    };
    it.github = Some(issue.number);
    it
}

// ------------------------------------------------------------ gh

pub fn gh_binary() -> String {
    std::env::var("PRIMUM_GH").unwrap_or_else(|_| "gh".into())
}

/// GitHub through the `gh` CLI: its login, its hosts, its pagination.
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
        labels: v["labels"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|l| l["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
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
        let out = Command::new(gh_binary())
            .args(["auth", "status"])
            .stdin(Stdio::null())
            .output()
            .map_err(|_| "gh not found".to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err("gh is not logged in (gh auth login)".into())
        }
    }

    fn api(&self, method: &str, path: &str, body: Option<&Value>, paginate: bool) -> Result<Value> {
        let mut c = Command::new(gh_binary());
        c.args([
            "api",
            "-X",
            method,
            "-H",
            "Accept: application/vnd.github+json",
        ]);
        if paginate {
            c.args(["--paginate", "--slurp"]);
        }
        c.arg(path)
            .env("GH_PROMPT_DISABLED", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if body.is_some() {
            c.args(["--input", "-"]).stdin(Stdio::piped());
        } else {
            c.stdin(Stdio::null());
        }
        let mut child = c.spawn().context("cannot run gh")?;
        if let Some(b) = body {
            use std::io::Write;
            child
                .stdin
                .take()
                .expect("piped")
                .write_all(b.to_string().as_bytes())?;
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!(
                "gh api {method} {path}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let text = String::from_utf8_lossy(&out.stdout);
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }

    fn write_json(w: &IssueWrite) -> Value {
        let state = if w.open { "open" } else { "closed" };
        let mut v = json!({ "title": w.title, "body": w.body, "labels": w.labels, "state": state });
        if let Some(r) = &w.reason {
            v["state_reason"] = json!(r);
        }
        v
    }
}

/// The items of every page of a `--paginate --slurp` answer.
fn pages(v: Value) -> Vec<Value> {
    match v {
        Value::Array(pages) => pages
            .into_iter()
            .flat_map(|p| match p {
                Value::Array(items) => items,
                other => vec![other],
            })
            .collect(),
        _ => vec![],
    }
}

impl Api for Gh {
    fn issues_since(&self, since: Option<&str>) -> Result<Vec<Issue>> {
        let mut path = format!(
            "repos/{}/issues?state=all&per_page=100&sort=updated&direction=asc",
            self.repo
        );
        if let Some(s) = since {
            path += &format!("&since={s}");
        }
        let mut out: Vec<Issue> = pages(self.api("GET", &path, None, true)?)
            .iter()
            .map(issue_from_json)
            .collect();
        // a closed PR's merge state is not in the issues list
        for i in out.iter_mut().filter(|i| i.is_pr && !i.open) {
            let pr = self.api(
                "GET",
                &format!("repos/{}/pulls/{}", self.repo, i.number),
                None,
                false,
            )?;
            i.pr_merged = pr["merged"].as_bool().unwrap_or(false);
        }
        Ok(out)
    }

    fn create(&self, w: &IssueWrite) -> Result<Issue> {
        let body = json!({ "title": w.title, "body": w.body, "labels": w.labels });
        let path = format!("repos/{}/issues", self.repo);
        let created = issue_from_json(&self.api("POST", &path, Some(&body), false)?);
        if w.open {
            Ok(created)
        } else {
            self.update(created.number, w)
        }
    }

    fn update(&self, number: u64, w: &IssueWrite) -> Result<Issue> {
        let path = format!("repos/{}/issues/{number}", self.repo);
        let v = self.api("PATCH", &path, Some(&Gh::write_json(w)), false)?;
        Ok(issue_from_json(&v))
    }

    fn comment(&self, number: u64, body: &str) -> Result<()> {
        let path = format!("repos/{}/issues/{number}/comments", self.repo);
        self.api("POST", &path, Some(&json!({ "body": body })), false)?;
        Ok(())
    }

    fn close(&self, number: u64, reason: &str) -> Result<()> {
        let body = json!({ "state": "closed", "state_reason": reason });
        let path = format!("repos/{}/issues/{number}", self.repo);
        self.api("PATCH", &path, Some(&body), false)?;
        Ok(())
    }

    fn ensure_labels(&self) -> Result<()> {
        let names: Vec<String> = self.labels()?.iter().map(|n| n.to_lowercase()).collect();
        let wanted = KIND_LABELS.iter().map(|k| (k.1, k.2)).chain([DOING]);
        for (name, color) in wanted {
            if !names.iter().any(|n| n == name) {
                self.create_label(name, color)?;
            }
        }
        Ok(())
    }

    fn labels(&self) -> Result<Vec<String>> {
        let path = format!("repos/{}/labels?per_page=100", self.repo);
        Ok(pages(self.api("GET", &path, None, true)?)
            .iter()
            .filter_map(|l| l["name"].as_str().map(str::to_string))
            .collect())
    }

    fn create_label(&self, name: &str, color: &str) -> Result<()> {
        let path = format!("repos/{}/labels", self.repo);
        let body = json!({ "name": name, "color": color });
        self.api("POST", &path, Some(&body), false)?;
        Ok(())
    }

    fn delete_label(&self, name: &str) -> Result<()> {
        let name = name.replace(':', "%3A");
        self.api(
            "DELETE",
            &format!("repos/{}/labels/{name}", self.repo),
            None,
            false,
        )?;
        Ok(())
    }
}

// ------------------------------------------------------------ in memory

/// A small GitHub for tests: issues in memory, `updated` from a counter.
pub struct Memory {
    state: Mutex<(Vec<Issue>, u64)>,
    label_names: Mutex<Vec<String>>,
}

impl Default for Memory {
    fn default() -> Self {
        Memory::new()
    }
}

impl Memory {
    pub fn new() -> Memory {
        Memory {
            state: Mutex::new((vec![], 0)),
            label_names: Mutex::new(vec![]),
        }
    }

    fn tick(clock: &mut u64) -> String {
        *clock += 1;
        format!("2026-10-14T{:02}:{:02}:00Z", *clock / 60, *clock % 60)
    }

    pub fn issue(&self, n: u64) -> Issue {
        self.all()
            .into_iter()
            .find(|i| i.number == n)
            .expect("no such issue")
    }

    pub fn all(&self) -> Vec<Issue> {
        self.state.lock().expect("lock").0.clone()
    }

    /// An edit made on GitHub.
    pub fn edit(&self, n: u64, f: impl FnOnce(&mut Issue)) {
        let mut s = self.state.lock().expect("lock");
        let (issues, clock) = &mut *s;
        let i = issues
            .iter_mut()
            .find(|i| i.number == n)
            .expect("no such issue");
        f(i);
        i.updated = Memory::tick(clock);
        if i.open {
            i.closed = None;
        } else if i.closed.is_none() {
            i.closed = Some(i.updated.clone());
        }
    }

    /// An issue deleted on GitHub (by an admin, or a transferred repository).
    pub fn remove(&self, n: u64) {
        self.state.lock().expect("lock").0.retain(|i| i.number != n);
    }

    pub fn open_pr(&self, title: &str, body: &str, merged: bool) -> u64 {
        let mut s = self.state.lock().expect("lock");
        let (issues, clock) = &mut *s;
        let n = issues.iter().map(|i| i.number).max().unwrap_or(0) + 1;
        let t = Memory::tick(clock);
        issues.push(Issue {
            number: n,
            title: title.into(),
            body: body.into(),
            open: !merged,
            reason: None,
            labels: vec![],
            updated: t.clone(),
            closed: merged.then_some(t),
            author: "gokce".into(),
            is_pr: true,
            pr_merged: merged,
        });
        n
    }

    fn put(&self, number: Option<u64>, w: &IssueWrite) -> Issue {
        let mut s = self.state.lock().expect("lock");
        let (issues, clock) = &mut *s;
        let updated = Memory::tick(clock);
        let n = number.unwrap_or(issues.iter().map(|i| i.number).max().unwrap_or(0) + 1);
        let issue = Issue {
            number: n,
            title: w.title.clone(),
            body: w.body.clone(),
            open: w.open,
            reason: w.reason.clone(),
            labels: w.labels.clone(),
            closed: (!w.open).then(|| updated.clone()),
            updated,
            author: "primum".into(),
            is_pr: false,
            pr_merged: false,
        };
        issues.retain(|i| i.number != n);
        issues.push(issue.clone());
        issues.sort_by_key(|i| i.number);
        issue
    }
}

impl Api for Memory {
    fn issues_since(&self, since: Option<&str>) -> Result<Vec<Issue>> {
        Ok(self
            .all()
            .into_iter()
            .filter(|i| since.is_none_or(|s| i.updated.as_str() > s))
            .collect())
    }

    fn create(&self, w: &IssueWrite) -> Result<Issue> {
        Ok(self.put(None, w))
    }

    fn update(&self, number: u64, w: &IssueWrite) -> Result<Issue> {
        if !self.all().iter().any(|i| i.number == number) {
            bail!("gh api PATCH issues/{number}: HTTP 404: Not Found");
        }
        Ok(self.put(Some(number), w))
    }

    fn comment(&self, number: u64, _: &str) -> Result<()> {
        self.all()
            .iter()
            .any(|i| i.number == number)
            .then_some(())
            .ok_or_else(|| anyhow!("HTTP 404: no issue #{number}"))
    }

    fn close(&self, number: u64, reason: &str) -> Result<()> {
        self.edit(number, |i| {
            i.open = false;
            i.reason = Some(reason.into());
        });
        Ok(())
    }

    fn ensure_labels(&self) -> Result<()> {
        Ok(())
    }

    fn labels(&self) -> Result<Vec<String>> {
        Ok(self.label_names.lock().expect("lock").clone())
    }

    fn create_label(&self, name: &str, _: &str) -> Result<()> {
        self.label_names
            .lock()
            .expect("lock")
            .push(name.to_string());
        Ok(())
    }

    fn delete_label(&self, name: &str) -> Result<()> {
        self.label_names.lock().expect("lock").retain(|n| n != name);
        Ok(())
    }
}
