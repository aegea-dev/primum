//! A one-time import of a videre-board board (`board.py`'s `data/items.json`
//! and its specs and plans) into an empty project.
//!
//! This is the only place `board.py`'s rules for linking documents to items
//! survive (`ticket_ids`, `_topic`, `item_docs`): inside primum a document
//! belongs to an item by its file name.
use crate::item_file::{self, file_name, parse_item_name, stem};
use crate::migrate::rewrite_refs;
use crate::model::{Item, KINDS, STATUSES, clamp_score, id_of};
use crate::store::{self, Project};
use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

/// What an import wrote and what it left behind.
#[derive(Debug, Default)]
pub struct Report {
    pub items: usize,
    /// (source, copy) of every document copied.
    pub docs: Vec<(String, String)>,
    /// (item, copy) of every document an item lists in its `docs:`.
    pub shared: Vec<(String, String)>,
    /// Source documents linked to no item, not copied.
    pub left: Vec<String>,
}

const DOC_DIRS: [(&str, &str); 2] = [("specs", "spec"), ("plans", "plan")];

static ITEM_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:URGENT|BUG|DEBT|KILLER|TODO):\d+\b").expect("regex"));
static TICKET_LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\btickets?(?:\*\*)?\s*:(?:\*\*)?\s*").expect("regex"));
static SENTENCE_END: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.(?:\s|$)|\*\*").expect("regex"));
static PARENS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\([^)]*\)?").expect("regex"));
static TOPIC_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(-design|-plan)?\.md$").expect("regex"));
static DATE_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4}-\d{2}-\d{2})-").expect("regex"));

/// The items a spec or plan says it is for: the ids on its first `Ticket:`
/// line, up to the end of that sentence, parentheses dropped (they name
/// related work, not the ticket). board.py's `ticket_ids`.
pub fn ticket_ids(text: &str) -> Vec<String> {
    let Some(m) = TICKET_LABEL.find(text) else {
        return vec![];
    };
    let rest = text[m.end()..].split('\n').next().unwrap_or("");
    let rest = SENTENCE_END.splitn(rest, 2).next().unwrap_or("");
    let rest = PARENS.replace_all(rest, "");
    ITEM_ID
        .find_iter(&rest)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// `2026-10-01-galeri-design.md` and `2026-10-01-galeri-plan.md` are both
/// topic `galeri`. board.py's `_topic`.
pub fn topic(name: &str) -> String {
    let stem = TOPIC_SUFFIX.replace(name, "");
    DATE_PREFIX.replace(&stem, "").to_string()
}

struct SourceDoc {
    doc_type: &'static str,
    name: String,
    rel: String,
    text: String,
    ids: Vec<String>,
}

/// board.py's `coerce_priority`: an integer, else the default, clamped to 1-10.
fn coerce(v: Option<&Value>, default: i64) -> i64 {
    let n = match v {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    };
    n.unwrap_or(default).clamp(1, 10)
}

fn text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(lines)) => lines
            .iter()
            .map(|l| l.as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn item_from_row(row: &Value) -> Result<Item> {
    let id = row["id"].as_str().context("a row has no id")?;
    let (kind, num) = id
        .split_once(':')
        .and_then(|(k, n)| Some((k, n.parse::<u32>().ok()?)))
        .with_context(|| format!("bad id {id}"))?;
    if !KINDS.contains(&kind) {
        bail!("{id}: unknown kind");
    }
    // the board.py id until `run` numbers the items
    let mut it = Item::new(kind, num, &text(row.get("title")));
    it.id = id.to_string();
    let status = row["status"].as_str().unwrap_or("open");
    if !STATUSES.contains(&status) {
        bail!("{id}: unknown status {status}");
    }
    it.status = status.to_string();
    let crit = coerce(row.get("crit"), 5);
    let cplx = coerce(row.get("cplx"), 5);
    it.impact = clamp_score(coerce(row.get("impact"), crit));
    it.urgency = clamp_score(coerce(row.get("urgency"), 3));
    it.risk = clamp_score(coerce(row.get("risk"), 3));
    it.effort = clamp_score(coerce(row.get("effort"), cplx));
    it.needs = ITEM_ID
        .find_iter(&text(row.get("needs")))
        .map(|m| m.as_str().to_string())
        .collect();
    it.finished = row
        .get("finished")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    it.source = text(row.get("source"));
    let body = text(row.get("body"));
    it.body = if body.trim().is_empty() {
        String::new()
    } else {
        format!("{}\n", body.trim_end())
    };
    Ok(it)
}

fn source_docs(board_dir: &Path) -> Result<Vec<SourceDoc>> {
    let mut out = vec![];
    for (dir, doc_type) in DOC_DIRS {
        let d = board_dir.join("superpowers").join(dir);
        if !d.is_dir() {
            continue;
        }
        let mut names: Vec<String> = fs::read_dir(&d)?
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.ends_with(".md"))
            .collect();
        names.sort();
        for name in names {
            let bytes = fs::read(d.join(&name))?;
            let text = String::from_utf8_lossy(&bytes).to_string();
            let head: String = text.chars().take(1500).collect();
            out.push(SourceDoc {
                doc_type,
                rel: format!("superpowers/{dir}/{name}"),
                ids: ticket_ids(&head),
                name,
                text,
            });
        }
    }
    Ok(out)
}

pub fn run(board_dir: &Path, project: &Project) -> Result<Report> {
    let items_dir = project.items_dir();
    let has_items = fs::read_dir(&items_dir)
        .with_context(|| format!("{} has no items/ folder", project.root.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .any(|n| parse_item_name(&n).is_some());
    if has_items {
        bail!(
            "{} already has items; import only into an empty project",
            project.root.display()
        );
    }
    let json_path = board_dir.join("data/items.json");
    let raw = fs::read_to_string(&json_path)
        .with_context(|| format!("no data/items.json in {}", board_dir.display()))?;
    let rows: Vec<Value> = serde_json::from_str(&raw)
        .with_context(|| format!("{} is not a list of items", json_path.display()))?;
    let mut items = rows.iter().map(item_from_row).collect::<Result<Vec<_>>>()?;
    let ids: BTreeSet<String> = items.iter().map(|i| i.id.clone()).collect();
    if ids.len() != items.len() {
        bail!("{} has an id twice", json_path.display());
    }

    // Each item's topics: the files whose Ticket line names it or that its
    // text mentions (board.py's item_docs).
    let files = source_docs(board_dir)?;
    let mut topics: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for it in &items {
        for f in &files {
            if f.ids.contains(&it.id) || it.body.contains(&f.name) {
                topics
                    .entry(it.id.clone())
                    .or_default()
                    .insert(topic(&f.name));
            }
        }
    }

    // primum's numbers: board.py's order
    let map: BTreeMap<String, String> = items
        .iter()
        .enumerate()
        .map(|(i, it)| (it.id.clone(), id_of(i as u32 + 1)))
        .collect();
    let mut report = Report {
        items: items.len(),
        ..Default::default()
    };
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut shared: Vec<(String, String)> = vec![];
    for f in &files {
        let t = topic(&f.name);
        // Ticket-line order first, then the rest in the board's order.
        let mut linked: Vec<String> = vec![];
        for id in f.ids.iter().filter(|id| ids.contains(*id)) {
            if !linked.contains(id) {
                linked.push(id.clone());
            }
        }
        for it in &items {
            if topics.get(&it.id).is_some_and(|ts| ts.contains(&t)) && !linked.contains(&it.id) {
                linked.push(it.id.clone());
            }
        }
        let Some(owner) = linked.first() else {
            report.left.push(f.rel.clone());
            continue;
        };
        let base = format!("{}.{}", stem(&map[owner]), f.doc_type);
        let date = DATE_PREFIX
            .captures(&f.name)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| t.clone());
        let name = [
            format!("{base}.md"),
            format!("{base}.{date}.md"),
            format!("{base}.{t}.md"),
        ]
        .into_iter()
        .find(|n| !taken.contains(n))
        .with_context(|| format!("no free name for {} beside {owner}", f.rel))?;
        fs::write(items_dir.join(&name), rewrite_refs(&f.text, &map))?;
        taken.insert(name.clone());
        let copy = format!("items/{name}");
        report.docs.push((f.rel.clone(), copy.clone()));
        for other in &linked[1..] {
            shared.push((map[other].clone(), copy.clone()));
        }
    }
    for (i, it) in items.iter_mut().enumerate() {
        it.num = i as u32 + 1;
        it.id = id_of(it.num);
        it.needs = it
            .needs
            .iter()
            .filter_map(|n| map.get(n).cloned())
            .collect();
        it.body = rewrite_refs(&it.body, &map);
    }
    for (id, copy) in &shared {
        if let Some(it) = items.iter_mut().find(|i| &i.id == id) {
            it.docs.push(copy.clone());
        }
    }
    report.shared = shared;

    for it in &items {
        fs::write(items_dir.join(file_name(&it.id)), item_file::write(it))?;
    }
    store::settle_all(project)?;
    if let Some(h) = crate::history::History::open(project)? {
        let message = format!("import {} items from {}", report.items, board_dir.display());
        h.commit(&message)?;
    }
    Ok(report)
}
