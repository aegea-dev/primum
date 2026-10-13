//! A project's items on disk. Every change takes the project lock, reads the
//! files, writes only what changed (temporary file, then rename), removes
//! closed ids from every `needs`, and rewrites BOARD.md.
use crate::item_file::{self, file_name, parse_doc_name, parse_item_name, stem};
use crate::model::{Item, KINDS, STATUSES, clamp_score, id_of, priority_cmp};
use anyhow::{Context, Result, anyhow, bail};
use fs2::FileExt;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
}

impl Project {
    pub fn items_dir(&self) -> PathBuf {
        self.root.join("items")
    }
}

/// A document an item shows: `kind` is its type (`spec`, `plan`, ...), the
/// tab label; `path` is relative to the project root.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Doc {
    pub kind: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug)]
pub struct Board {
    /// In priority order.
    pub items: Vec<Item>,
    /// Every item's documents, its own first (spec, plan, then by name),
    /// then the ones its `docs:` lists.
    pub docs: BTreeMap<String, Vec<Doc>>,
}

impl Board {
    pub fn get(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut Item> {
        self.items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow!("no item {id}"))
    }

    fn next_num(&self, kind: &str) -> u32 {
        self.items
            .iter()
            .filter(|i| i.kind == kind)
            .map(|i| i.num)
            .max()
            .unwrap_or(0)
            + 1
    }

    /// The item's own documents: the files named for it.
    fn own_docs(&self, id: &str) -> Vec<Doc> {
        let prefix = format!("items/{}.", stem(id));
        self.docs
            .get(id)
            .map(|v| {
                v.iter()
                    .filter(|d| d.path.starts_with(&prefix))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A change to an item; `None` leaves a field as it is. Scores are clamped
/// to 1-10.
#[derive(Debug, Default, Clone)]
pub struct Changes {
    pub title: Option<String>,
    pub status: Option<String>,
    pub impact: Option<i64>,
    pub urgency: Option<i64>,
    pub risk: Option<i64>,
    pub effort: Option<i64>,
    pub needs: Option<Vec<String>>,
    pub body: Option<String>,
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn lock(p: &Project) -> Result<File> {
    let f = File::create(p.root.join(".primum.lock")).context("cannot create the project lock")?;
    f.lock_exclusive().context("cannot take the project lock")?;
    Ok(f)
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, text).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
}

pub fn load(p: &Project) -> Result<Board> {
    let mut items = vec![];
    let mut own: BTreeMap<String, Vec<Doc>> = BTreeMap::new();
    let mut names: Vec<String> = fs::read_dir(p.items_dir())
        .with_context(|| format!("{} has no items/ folder", p.root.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    for name in names {
        if parse_item_name(&name).is_some() {
            let text = fs::read_to_string(p.items_dir().join(&name))
                .with_context(|| format!("cannot read items/{name}"))?;
            items.push(item_file::parse(&name, &text)?);
        } else if let Some((kind, num, doc_type)) = parse_doc_name(&name) {
            own.entry(id_of(&kind, num)).or_default().push(Doc {
                kind: doc_type,
                path: format!("items/{name}"),
                name,
            });
        }
    }
    let mut docs = BTreeMap::new();
    for it in &items {
        let mut list = own.get(&it.id).cloned().unwrap_or_default();
        list.sort_by_key(|d| {
            let rank = match d.kind.as_str() {
                "spec" => 0,
                "plan" => 1,
                _ => 2,
            };
            (rank, d.name.clone())
        });
        for path in &it.docs {
            let name = path.rsplit('/').next().unwrap_or(path).to_string();
            let kind = parse_doc_name(&name)
                .map(|x| x.2)
                .unwrap_or_else(|| "doc".into());
            list.push(Doc {
                kind,
                name,
                path: path.clone(),
            });
        }
        docs.insert(it.id.clone(), list);
    }
    items.sort_by(priority_cmp);
    Ok(Board { items, docs })
}

fn save(p: &Project, it: &Item) -> Result<()> {
    write_atomic(
        &p.items_dir().join(file_name(&it.id)),
        &item_file::write(it),
    )
}

/// Removes closed ids from every `needs` and writes the items that changed,
/// then BOARD.md. Runs after every change, as board.py's export did. An id
/// naming no item is kept: it may be written ahead of the item.
fn settle(p: &Project, board: &mut Board) -> Result<()> {
    let closed: Vec<String> = board
        .items
        .iter()
        .filter(|i| i.is_closed())
        .map(|i| i.id.clone())
        .collect();
    for it in board.items.iter_mut() {
        let kept: Vec<String> = it
            .needs
            .iter()
            .filter(|n| !closed.contains(n))
            .cloned()
            .collect();
        if kept != it.needs {
            it.needs = kept;
            save(p, it)?;
        }
    }
    write_atomic(&p.root.join("BOARD.md"), &board_markdown(board, &p.name))
}

/// Settles the whole project and rewrites BOARD.md, for when items were
/// written directly (an import).
pub fn settle_all(p: &Project) -> Result<()> {
    let _l = lock(p)?;
    let mut board = load(p)?;
    settle(p, &mut board)
}

fn apply(it: &mut Item, c: Changes) -> Result<()> {
    if let Some(s) = c.status {
        if !STATUSES.contains(&s.as_str()) {
            bail!("unknown status {s}");
        }
        it.finished = (s == "done" || s == "wontfix").then(today);
        it.status = s;
    }
    if let Some(t) = c.title {
        it.title = t;
    }
    for (field, v) in [
        (&mut it.impact, c.impact),
        (&mut it.urgency, c.urgency),
        (&mut it.risk, c.risk),
        (&mut it.effort, c.effort),
    ] {
        if let Some(v) = v {
            *field = clamp_score(v);
        }
    }
    if let Some(n) = c.needs {
        it.needs = n;
    }
    if let Some(b) = c.body {
        it.body = b;
    }
    Ok(())
}

pub fn insert(p: &Project, kind: &str, title: &str, c: Changes) -> Result<String> {
    if !KINDS.contains(&kind) {
        bail!("unknown kind {kind}");
    }
    let _l = lock(p)?;
    let mut board = load(p)?;
    let title = if title.trim().is_empty() {
        "untitled"
    } else {
        title
    };
    let mut it = Item::new(kind, board.next_num(kind), title);
    it.source = "primum".into();
    apply(&mut it, c)?;
    save(p, &it)?;
    let id = it.id.clone();
    board.items.push(it);
    settle(p, &mut board)?;
    Ok(id)
}

fn update_locked(p: &Project, board: &mut Board, id: &str, c: Changes) -> Result<()> {
    let it = board.get_mut(id)?;
    apply(it, c)?;
    let it = it.clone();
    save(p, &it)?;
    settle(p, board)
}

pub fn update(p: &Project, id: &str, c: Changes) -> Result<()> {
    let _l = lock(p)?;
    let mut board = load(p)?;
    update_locked(p, &mut board, id, c)
}

/// Appends `text` under a dated heading, after a rule when the body already
/// has text.
pub fn append_note(p: &Project, id: &str, text: &str) -> Result<()> {
    let _l = lock(p)?;
    let mut board = load(p)?;
    let body = board.get_mut(id)?.body.trim_end().to_string();
    let note = format!("## {}\n\n{}\n", today(), text.trim());
    let body = if body.is_empty() {
        note
    } else {
        format!("{body}\n\n---\n\n{note}")
    };
    let c = Changes {
        body: Some(body),
        ..Default::default()
    };
    update_locked(p, &mut board, id, c)
}

/// `## OLD rest` or `## <num>. rest` on the body's first line becomes
/// `## NEW rest`: board.py's rewrite_body_heading.
fn rewrite_heading(body: &str, old: &str, new: &str) -> String {
    let (first, rest) = match body.split_once('\n') {
        Some((a, b)) => (a, Some(b)),
        None => (body, None),
    };
    let hashes = first.len() - first.trim_start_matches('#').len();
    let after = &first[hashes..];
    if hashes == 0 || !after.starts_with(char::is_whitespace) {
        return body.to_string();
    }
    let text = after.trim_start();
    let prefix = &first[..first.len() - text.len()];
    let old_num = old.split_once(':').map(|x| x.1).unwrap_or("");
    let tail = if let Some(t) = text.strip_prefix(old) {
        if t.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            return body.to_string();
        }
        t
    } else if let Some(t) = text
        .strip_prefix(old_num)
        .and_then(|t| t.strip_prefix('.'))
        .filter(|t| t.starts_with(char::is_whitespace))
    {
        t
    } else {
        return body.to_string();
    };
    let line = format!("{prefix}{new} {}", tail.trim_start())
        .trim_end()
        .to_string();
    match rest {
        Some(r) => format!("{line}\n{r}"),
        None => line,
    }
}

/// Moves an open item to another lane: the next number there, its files and
/// documents renamed, its heading, and every `needs` and `docs` naming it
/// rewritten. Returns the new id.
pub fn set_kind(p: &Project, id: &str, kind: &str) -> Result<String> {
    if !KINDS.contains(&kind) {
        bail!("unknown kind {kind}");
    }
    let _l = lock(p)?;
    let board = load(p)?;
    let old = board
        .get(id)
        .ok_or_else(|| anyhow!("no item {id}"))?
        .clone();
    if old.is_closed() {
        bail!("{id} is closed; reopen it before changing kind");
    }
    if old.kind == kind {
        return Ok(id.to_string());
    }
    let new_id = id_of(kind, board.next_num(kind));
    let (old_stem, new_stem) = (stem(id), stem(&new_id));
    let mut moved = old.clone();
    moved.id = new_id.clone();
    moved.kind = kind.to_string();
    moved.num = board.next_num(kind);
    moved.body = rewrite_heading(&old.body, id, &new_id);
    save(p, &moved)?;
    fs::remove_file(p.items_dir().join(file_name(id)))?;
    for d in board.own_docs(id) {
        let to = d.name.replacen(&old_stem, &new_stem, 1);
        fs::rename(p.items_dir().join(&d.name), p.items_dir().join(to))?;
    }
    let (old_prefix, new_prefix) = (format!("items/{old_stem}."), format!("items/{new_stem}."));
    for it in board.items.iter().filter(|i| i.id != id) {
        let needs: Vec<String> = it
            .needs
            .iter()
            .map(|n| if n == id { new_id.clone() } else { n.clone() })
            .collect();
        let docs: Vec<String> = it
            .docs
            .iter()
            .map(|d| match d.strip_prefix(&old_prefix) {
                Some(rest) => format!("{new_prefix}{rest}"),
                None => d.clone(),
            })
            .collect();
        if needs != it.needs || docs != it.docs {
            let mut it = it.clone();
            it.needs = needs;
            it.docs = docs;
            save(p, &it)?;
        }
    }
    let mut board = load(p)?;
    settle(p, &mut board)?;
    Ok(new_id)
}

/// Deletes an item and its own documents, and removes its id and documents
/// from every other item. Returns the files removed.
pub fn delete(p: &Project, id: &str) -> Result<Vec<String>> {
    let _l = lock(p)?;
    let mut board = load(p)?;
    board.get(id).ok_or_else(|| anyhow!("no item {id}"))?;
    let mut gone = vec![format!("items/{}", file_name(id))];
    fs::remove_file(p.items_dir().join(file_name(id)))?;
    for d in board.own_docs(id) {
        fs::remove_file(p.root.join(&d.path))?;
        gone.push(d.path);
    }
    board.items.retain(|i| i.id != id);
    for it in board.items.iter_mut() {
        let needs: Vec<String> = it.needs.iter().filter(|n| *n != id).cloned().collect();
        let docs: Vec<String> = it
            .docs
            .iter()
            .filter(|d| !gone.contains(d))
            .cloned()
            .collect();
        if needs != it.needs || docs != it.docs {
            it.needs = needs;
            it.docs = docs;
            save(p, it)?;
        }
    }
    settle(p, &mut board)?;
    Ok(gone)
}

/// Adds or replaces an item's document `items/<ID>.<type>.md`. Returns its
/// path.
pub fn set_doc(p: &Project, id: &str, doc_type: &str, text: &str) -> Result<String> {
    if doc_type.is_empty()
        || !doc_type
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        bail!("a document type is lowercase letters, digits and -, like spec or plan");
    }
    let _l = lock(p)?;
    let board = load(p)?;
    board.get(id).ok_or_else(|| anyhow!("no item {id}"))?;
    let name = format!("{}.{doc_type}.md", stem(id));
    write_atomic(&p.items_dir().join(&name), text)?;
    Ok(format!("items/{name}"))
}

/// A document's text, or None for anything that is not a document file
/// directly in items/.
pub fn read_doc(p: &Project, rel: &str) -> Option<String> {
    let name = rel.strip_prefix("items/")?;
    if name.contains('/') || name.contains('\\') || parse_doc_name(name).is_none() {
        return None;
    }
    fs::read_to_string(p.items_dir().join(name)).ok()
}

const KIND_TITLES: [(&str, &str); 5] = [
    ("URGENT", "Urgent"),
    ("BUG", "Bugs"),
    ("DEBT", "Tech debt"),
    ("KILLER", "Killer features"),
    ("TODO", "TODOs"),
];

/// board.py's export_markdown, its links pointing at the item files. The
/// bodies are not repeated: each is one click away.
pub fn board_markdown(board: &Board, name: &str) -> String {
    let open: Vec<&Item> = board.items.iter().filter(|i| !i.is_closed()).collect();
    let mut closed: Vec<&Item> = board.items.iter().filter(|i| i.is_closed()).collect();
    let mut out = vec![
        format!("# {name}\n"),
        "Generated by primum on every change. **Do not edit this file**: change items with `primum` or in `items/`.\n".to_string(),
        format!(
            "{} open, {} closed, {} total.\n",
            open.len(),
            closed.len(),
            board.items.len()
        ),
        "## Open items\n".to_string(),
    ];
    for (kind, title) in KIND_TITLES {
        let group: Vec<&&Item> = open.iter().filter(|i| i.kind == kind).collect();
        if group.is_empty() {
            continue;
        }
        out.push(format!("### {title} ({})\n", group.len()));
        out.push("| id | impact | urgency | risk | effort | score | status | title |".into());
        out.push("|---|---|---|---|---|---|---|---|".into());
        for i in group {
            out.push(format!(
                "| [{}](items/{}) | {} | {} | {} | {} | {:.1} | {} | {} |",
                i.id,
                file_name(&i.id),
                i.impact,
                i.urgency,
                i.risk,
                i.effort,
                i.score(),
                i.status,
                i.title
            ));
        }
        out.push(String::new());
    }
    if !closed.is_empty() {
        closed.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.num.cmp(&b.num)));
        out.push("## Closed\n".into());
        out.push("| id | status | finished | title |".into());
        out.push("|---|---|---|---|".into());
        for i in closed {
            out.push(format!(
                "| {} | {} | {} | {} |",
                i.id,
                i.status,
                i.finished.as_deref().unwrap_or(""),
                i.title
            ));
        }
        out.push(String::new());
    }
    out.join("\n").trim_end().to_string() + "\n"
}
