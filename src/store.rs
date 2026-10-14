//! A project's items on disk. Every change takes the project lock, reads the
//! files, writes only what changed (temporary file, then rename), removes
//! closed ids from every `needs`, and rewrites BOARD.md.
use crate::history::History;
use crate::item_file::{self, file_name, parse_doc_name, parse_item_name, stem};
use crate::lifecycle::Rules;
use crate::model::{Item, KINDS, clamp_score, id_of, label, priority_cmp};
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
    /// Items a sync left with a conflict, by id.
    pub conflicts: BTreeMap<String, crate::conflicts::Conflict>,
    /// The highest number any document file carries, item or not: such a
    /// number is taken, so a new item never inherits stray documents.
    pub doc_max: u32,
    /// The rules of the built-in `manual` lifecycle, which tickets without
    /// a lifecycle follow.
    pub manual: Rules,
    /// The rules of every other lifecycle a ticket follows, by name.
    pub rules: BTreeMap<String, Rules>,
}

impl Board {
    pub fn get(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    /// The rules of the lifecycle an item follows.
    pub fn rules_of(&self, it: &Item) -> &Rules {
        it.lifecycle
            .as_deref()
            .map(|pin| crate::lifecycle::split_pin(pin).0)
            .and_then(|name| self.rules.get(name))
            .unwrap_or(&self.manual)
    }

    /// Whether an item is closed in its lifecycle.
    pub fn is_closed(&self, it: &Item) -> bool {
        it.is_closed_in(&self.rules_of(it).closed)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut Item> {
        self.items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow!("no item {id}"))
    }

    /// The next number: one above every item's, and every number in
    /// `deleted`, so a number is never reused.
    pub fn next_num(&self, deleted: &[u32]) -> u32 {
        self.items
            .iter()
            .map(|i| i.num)
            .chain(deleted.iter().copied())
            .chain([self.doc_max])
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
    pub kind: Option<String>,
    pub status: Option<String>,
    pub impact: Option<i64>,
    pub urgency: Option<i64>,
    pub risk: Option<i64>,
    pub effort: Option<i64>,
    pub needs: Option<Vec<String>>,
    pub body: Option<String>,
}

impl Changes {
    /// The fields this change sets, in a fixed order.
    pub fn fields(&self) -> Vec<&'static str> {
        [
            ("title", self.title.is_some()),
            ("kind", self.kind.is_some()),
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
        let id = label(id);
        match (&self.status, &self.kind, self.fields().as_slice()) {
            (Some(s), _, ["status"]) => format!("{id}: status {s}"),
            (_, Some(k), ["kind"]) => format!("{id}: kind {k}"),
            (_, _, fields) => format!("{id}: {}", fields.join(", ")),
        }
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub(crate) fn lock(p: &Project) -> Result<File> {
    let f = File::create(p.root.join(".primum.lock")).context("cannot create the project lock")?;
    f.lock_exclusive().context("cannot take the project lock")?;
    Ok(f)
}

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

/// Changes a project's settings files under the lock, as one commit named
/// `message`, after any edits made outside primum.
pub(crate) fn change_settings(
    p: &Project,
    message: &str,
    f: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let _l = lock(p)?;
    let history = History::open(p)?;
    if let Some(h) = &history {
        h.record_outside()?;
    }
    f()?;
    if let Some(h) = &history {
        h.commit(message)?;
    }
    Ok(())
}

pub(crate) fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, text).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
}

/// Numbers of items deleted in the project's history; none without git.
fn deleted(p: &Project) -> Result<Vec<u32>> {
    Ok(match History::open(p)? {
        Some(h) => h.deleted_numbers()?,
        None => vec![],
    })
}

pub fn load(p: &Project) -> Result<Board> {
    let mut items = vec![];
    let mut own: BTreeMap<String, Vec<Doc>> = BTreeMap::new();
    let mut names: Vec<String> = fs::read_dir(p.items_dir())
        .with_context(|| format!("{} has no items/ folder", p.root.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    if names.iter().any(|n| item_file::old_style_name(n)) {
        bail!("this board uses ids from an earlier version: run primum migrate");
    }
    names.sort();
    for name in names {
        if parse_item_name(&name).is_some() {
            let text = fs::read_to_string(p.items_dir().join(&name))
                .with_context(|| format!("cannot read items/{name}"))?;
            items.push(item_file::parse(&name, &text)?);
        } else if let Some((num, doc_type)) = parse_doc_name(&name) {
            own.entry(id_of(num)).or_default().push(Doc {
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
                .map(|x| x.1)
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
    let conflicts = crate::conflicts::read_all(p)?;
    let doc_max = own
        .keys()
        .filter_map(|id| crate::model::parse_id(id))
        .max()
        .unwrap_or(0);
    let manual = Rules::manual();
    // each lifecycle a ticket follows: its file's rules, or the defaults
    // when the file is gone (the engine keeps pinned versions)
    let mut rules: BTreeMap<String, Rules> = BTreeMap::new();
    for pin in items.iter().filter_map(|i| i.lifecycle.as_deref()) {
        let name = crate::lifecycle::split_pin(pin).0;
        if !rules.contains_key(name) {
            let r = match crate::lifecycle::resolve(p, name) {
                Some(Ok(l)) => Rules::of(&l.file),
                Some(Err(e)) => return Err(anyhow!(e)),
                None => Rules {
                    states: crate::lifecycle::FALLBACK_STATES
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    closed: vec!["done".into(), "wontfix".into()],
                },
            };
            rules.insert(name.to_string(), r);
        }
    }
    for it in &items {
        let r = it
            .lifecycle
            .as_deref()
            .and_then(|pin| rules.get(crate::lifecycle::split_pin(pin).0))
            .unwrap_or(&manual);
        check_status(&it.id, &it.status, r).map_err(|e| {
            anyhow!(
                "items/{}: {e:#}; boards made before lifecycles: run primum migrate",
                file_name(&it.id)
            )
        })?;
    }
    Ok(Board {
        items,
        docs,
        conflicts,
        doc_max,
        manual,
        rules,
    })
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
        .filter(|i| board.is_closed(i))
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
    settle_locked(p)
}

/// `settle_all` without taking the lock, for callers that hold it.
pub(crate) fn settle_locked(p: &Project) -> Result<()> {
    let mut board = load(p)?;
    settle(p, &mut board)
}

/// Checks that `status` is one of the rules' states.
fn check_status(id: &str, status: &str, rules: &Rules) -> Result<()> {
    if !rules.states.iter().any(|s| s == status) {
        bail!(
            "{}: status \"{status}\" is not one of {}",
            label(id),
            rules.states.join(", ")
        );
    }
    Ok(())
}

fn apply(it: &mut Item, c: Changes, rules: &Rules) -> Result<()> {
    if let Some(s) = c.status {
        check_status(&it.id, &s, rules)?;
        it.finished = rules.closed.contains(&s).then(today);
        it.status = s;
    }
    if let Some(t) = c.title {
        it.title = t;
    }
    if let Some(k) = c.kind {
        if !KINDS.contains(&k.as_str()) {
            bail!("unknown kind {k}");
        }
        it.kind = k;
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

/// The text of the body's first markdown heading (any level), without its
/// `#` marks.
fn first_heading(body: &str) -> Option<String> {
    body.lines().find_map(|l| {
        let l = l.trim_start();
        let hashes = l.len() - l.trim_start_matches('#').len();
        let text = &l[hashes..];
        let text = text.strip_prefix([' ', '\t'])?.trim();
        (1..=6)
            .contains(&hashes)
            .then(|| text.to_string())
            .filter(|t| !t.is_empty())
    })
}

/// Adds an item. A blank title is taken from the body's first heading; with
/// neither, nothing is added.
pub fn insert(p: &Project, kind: &str, title: &str, c: Changes) -> Result<String> {
    if !KINDS.contains(&kind) {
        bail!("unknown kind {kind}");
    }
    let title = match title.trim() {
        "" => c
            .body
            .as_deref()
            .and_then(first_heading)
            .ok_or_else(|| anyhow!("give a title, or start the text with a heading"))?,
        _ => title.to_string(),
    };
    let deleted = deleted(p)?;
    change(p, |board| {
        let mut it = Item::new(kind, board.next_num(&deleted), &title);
        it.source = "primum".into();
        apply(&mut it, c, &board.manual.clone())?;
        save(p, &it)?;
        let id = it.id.clone();
        let message = format!("{}: add {}", label(&id), serde_json::to_string(&it.title)?);
        board.items.push(it);
        settle(p, board)?;
        Ok((id, message))
    })
}

fn update_locked(p: &Project, board: &mut Board, id: &str, c: Changes) -> Result<()> {
    let rules = board
        .rules_of(board.get(id).ok_or_else(|| anyhow!("no item {id}"))?)
        .clone();
    let it = board.get_mut(id)?;
    apply(it, c, &rules)?;
    let it = it.clone();
    save(p, &it)?;
    settle(p, board)
}

pub fn update(p: &Project, id: &str, c: Changes) -> Result<()> {
    let message = c.message(id);
    change(p, |board| Ok((update_locked(p, board, id, c)?, message)))
}

/// Appends `text` under a dated heading, after a rule when the body already
/// has text.
pub fn append_note(p: &Project, id: &str, text: &str) -> Result<()> {
    change(p, |board| {
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
        update_locked(p, board, id, c)?;
        Ok(((), format!("{}: note", label(id))))
    })
}

/// Gives an item another number without the lock: its file and own
/// documents renamed, its conflict record, and every reference to it in
/// other items (`needs`, `docs`, `#n` in text) rewritten. Sync's
/// renumbering of a number taken on the remote. Returns the new id.
pub(crate) fn renumber_locked(p: &Project, id: &str, num: u32) -> Result<String> {
    let board = load(p)?;
    let old = board
        .get(id)
        .ok_or_else(|| anyhow!("no item {id}"))?
        .clone();
    let new_id = id_of(num);
    let map: BTreeMap<String, String> = [(id.to_string(), new_id.clone())].into();
    let mut moved = old.clone();
    moved.id = new_id.clone();
    moved.num = num;
    moved.body = crate::migrate::rewrite_refs(&old.body, &map);
    save(p, &moved)?;
    fs::remove_file(p.items_dir().join(file_name(id)))?;
    let old_prefix = format!("{}.", stem(id));
    for d in board.own_docs(id) {
        let to = format!("{}.{}", stem(&new_id), &d.name[old_prefix.len()..]);
        fs::rename(p.items_dir().join(&d.name), p.items_dir().join(to))?;
    }
    crate::conflicts::rename(p, id, &new_id)?;
    for it in board.items.iter().filter(|i| i.id != id) {
        let mut changed = it.clone();
        changed.needs = it
            .needs
            .iter()
            .map(|n| if n == id { new_id.clone() } else { n.clone() })
            .collect();
        changed.docs = it
            .docs
            .iter()
            .map(|d| crate::migrate::rewrite_refs(d, &map))
            .collect();
        changed.body = crate::migrate::rewrite_refs(&it.body, &map);
        if changed != *it {
            save(p, &changed)?;
        }
    }
    settle_locked(p)?;
    Ok(new_id)
}

/// Moves an item to another lane: one field, nothing renamed.
pub fn set_kind(p: &Project, id: &str, kind: &str) -> Result<()> {
    update(
        p,
        id,
        Changes {
            kind: Some(kind.to_string()),
            ..Default::default()
        },
    )
}

/// Context variables derived from the board: items, open, closed, and a
/// count per status (`status.idle`, ...).
pub fn computed_context(board: &Board) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    let closed = board.items.iter().filter(|i| board.is_closed(i)).count();
    m.insert("items".into(), board.items.len().into());
    m.insert("open".into(), (board.items.len() - closed).into());
    m.insert("closed".into(), closed.into());
    let mut by: BTreeMap<&str, usize> = BTreeMap::new();
    for it in &board.items {
        *by.entry(it.status.as_str()).or_default() += 1;
    }
    for (s, n) in by {
        m.insert(format!("status.{s}"), n.into());
    }
    m
}

/// Attaches an item to a lifecycle at its current version, with the
/// arguments its actions read: its status is the lifecycle's start.
/// Returns the version.
pub fn attach(
    p: &Project,
    id: &str,
    name: &str,
    args: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<String> {
    let l = crate::lifecycle::resolve(p, name)
        .ok_or_else(|| anyhow!("no lifecycle {name} in lifecycles/"))??;
    change(p, |board| {
        let it = board.get_mut(id)?;
        it.lifecycle = Some(format!("{name}@{}", l.version));
        it.args = args;
        it.status = crate::lifecycle::first_state(&l.file);
        it.finished = None;
        let it = it.clone();
        save(p, &it)?;
        settle(p, board)?;
        Ok((l.version.clone(), format!("{}: attach {name}", label(id))))
    })
}

/// Detaches an item from its lifecycle: it follows `manual` again, and a
/// status `manual` does not have becomes `idle`.
pub fn detach(p: &Project, id: &str) -> Result<()> {
    change(p, |board| {
        let manual = board.manual.clone();
        let it = board.get_mut(id)?;
        it.lifecycle = None;
        it.args = None;
        if !manual.states.contains(&it.status) {
            it.status = manual.states[0].clone();
        }
        it.finished = manual.closed.contains(&it.status).then(today);
        let it = it.clone();
        save(p, &it)?;
        settle(p, board)?;
        Ok(((), format!("{}: detach", label(id))))
    })
}

/// Deletes an item and its own documents, and removes its id and documents
/// from every other item. Returns the files removed.
pub fn delete(p: &Project, id: &str) -> Result<Vec<String>> {
    change(p, |board| {
        Ok((
            delete_locked(p, board, id)?,
            format!("{}: delete", label(id)),
        ))
    })
}

fn delete_locked(p: &Project, board: &mut Board, id: &str) -> Result<Vec<String>> {
    board.get(id).ok_or_else(|| anyhow!("no item {id}"))?;
    let mut gone = vec![format!("items/{}", file_name(id))];
    fs::remove_file(p.items_dir().join(file_name(id)))?;
    for d in board.own_docs(id) {
        fs::remove_file(p.root.join(&d.path))?;
        gone.push(d.path);
    }
    crate::conflicts::remove(p, id)?;
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
    settle(p, board)?;
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
    change(p, |board| {
        board.get(id).ok_or_else(|| anyhow!("no item {id}"))?;
        let name = format!("{}.{doc_type}.md", stem(id));
        write_atomic(&p.items_dir().join(&name), text)?;
        Ok((
            format!("items/{name}"),
            format!("{}: {doc_type} document", label(id)),
        ))
    })
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
    let open: Vec<&Item> = board.items.iter().filter(|i| !board.is_closed(i)).collect();
    let mut closed: Vec<&Item> = board.items.iter().filter(|i| board.is_closed(i)).collect();
    let mut out = vec![
        format!("# {name}\n"),
        "Generated by primum on every change. **Do not edit this file**: change items with `primum` or in `items/`.\n".to_string(),
        format!(
            "{} open, {} closed, {} total.\n",
            open.len(),
            closed.len(),
            board.items.len()
        ),
    ];
    if !board.conflicts.is_empty() {
        out.push("## Conflicts\n".to_string());
        for (id, c) in &board.conflicts {
            out.push(format!(
                "- [{}](items/{}): {}",
                label(id),
                file_name(id),
                c.what().join(", ")
            ));
        }
        out.push(String::new());
    }
    out.push("## Open items\n".to_string());
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
                label(&i.id),
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
        closed.sort_by_key(|i| i.num);
        out.push("## Closed\n".into());
        out.push("| id | status | finished | title |".into());
        out.push("|---|---|---|---|".into());
        for i in closed {
            out.push(format!(
                "| {} | {} | {} | {} |",
                label(&i.id),
                i.status,
                i.finished.as_deref().unwrap_or(""),
                i.title
            ));
        }
        out.push(String::new());
    }
    out.join("\n").trim_end().to_string() + "\n"
}

/// Which side of a conflict to keep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Keep {
    First,
    Second,
    /// Documents only: keep both copies.
    Both,
}

/// Settles one conflicting field, the body (`body`), or a document (its
/// path) of `id` with the chosen side, and removes it from the item's
/// conflict record.
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
            let b = c
                .body
                .clone()
                .ok_or_else(|| anyhow!("{id} has no conflict on its text"))?;
            let body = pick(&b.values)?;
            c.body = None;
            let ch = Changes {
                body: Some(body),
                ..Default::default()
            };
            update_locked(p, board, id, ch)?;
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
            let v = pick(&c.fields[i].values)?;
            c.fields.remove(i);
            let num = || v.parse::<i64>().map_err(|_| anyhow!("{v} is not a score"));
            let mut ch = Changes::default();
            match what {
                "title" => ch.title = Some(v.clone()),
                "status" => ch.status = Some(v.clone()),
                "impact" => ch.impact = Some(num()?),
                "urgency" => ch.urgency = Some(num()?),
                "risk" => ch.risk = Some(num()?),
                "effort" => ch.effort = Some(num()?),
                "kind" => ch.kind = Some(v.clone()),
                "source" => {
                    let it = board.get_mut(id)?;
                    it.source = v.clone();
                    let it = it.clone();
                    save(p, &it)?;
                }
                // "<name>@<version>", "<name>@<version> <args JSON>", or "manual"
                "lifecycle" => {
                    let (pin, args) = match v.as_str() {
                        "manual" => (None, None),
                        v => match v.split_once(' ') {
                            Some((pin, args)) => {
                                (Some(pin.to_string()), Some(serde_json::from_str(args)?))
                            }
                            None => (Some(v.to_string()), None),
                        },
                    };
                    // the status follows the side kept: one its lifecycle has
                    let rules = match &pin {
                        Some(pin) => {
                            match crate::lifecycle::resolve(p, crate::lifecycle::split_pin(pin).0) {
                                Some(Ok(l)) => Rules::of(&l.file),
                                _ => board.manual.clone(),
                            }
                        }
                        None => board.manual.clone(),
                    };
                    let it = board.get_mut(id)?;
                    (it.lifecycle, it.args) = (pin, args);
                    if !rules.states.contains(&it.status) {
                        it.status = rules.states[0].clone();
                    }
                    let it = it.clone();
                    save(p, &it)?;
                }
                other => bail!("cannot resolve {other}"),
            }
            if what != "source" && what != "lifecycle" {
                update_locked(p, board, id, ch)?;
            }
        }
        crate::conflicts::write(p, &c)?;
        settle_locked(p)?;
        Ok(((), format!("{}: resolve {what}", label(id))))
    })
}
