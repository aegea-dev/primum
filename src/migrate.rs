//! Brings a board made by an earlier version up to date, once: per-lane ids
//! (`TODO:7`, `items/TODO-7.md`) become one counter (`#3`, `items/3.md`).
//! Also the rewriting of references in text, which sync's renumbering uses.
use crate::history::History;
use crate::item_file::{self, file_name, old_style_name};
use crate::model::{id_of, kind_rank};
use crate::store::{self, Project};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::sync::LazyLock;

static REF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?P<pre>^|[^\w&/#])#(?P<n>[0-9]+)\b",
        r"|\b(?P<k>URGENT|BUG|DEBT|KILLER|TODO):(?P<kn>[0-9]+)\b",
        r"|\bitems/(?:(?P<dk>URGENT|BUG|DEBT|KILLER|TODO)-)?(?P<dn>[0-9]+)\.",
    ))
    .expect("a valid pattern")
});

/// `text` with every reference in `map` rewritten: `TODO:7` and `#7` become
/// `#N`, `items/TODO-7.` and `items/7.` become `items/N.`. Keys are old ids
/// (`TODO:7`) or numbers (`7`); others stay as they are.
pub fn rewrite_refs(text: &str, map: &BTreeMap<String, String>) -> String {
    REF.replace_all(text, |c: &regex::Captures| {
        let whole = c[0].to_string();
        if let Some(n) = c.name("n") {
            let pre = c.name("pre").map_or("", |m| m.as_str());
            return match map.get(n.as_str()) {
                Some(new) => format!("{pre}#{new}"),
                None => whole,
            };
        }
        if let (Some(k), Some(n)) = (c.name("k"), c.name("kn")) {
            return match map.get(&format!("{}:{}", k.as_str(), n.as_str())) {
                Some(new) => format!("#{new}"),
                None => whole,
            };
        }
        let n = &c["dn"];
        let key = match c.name("dk") {
            Some(k) => format!("{}:{n}", k.as_str()),
            None => n.to_string(),
        };
        match map.get(&key) {
            Some(new) => format!("items/{new}."),
            None => whole,
        }
    })
    .into_owned()
}

/// `TODO-7.md` is ("TODO", 7); `TODO-7.spec.md` is ("TODO", 7) too.
fn old_parts(name: &str) -> Option<(String, u32)> {
    let stem = name.strip_suffix(".md")?;
    let id = stem.split('.').next()?;
    let (kind, num) = id.split_once('-')?;
    Some((kind.to_string(), num.parse().ok()?))
}

/// When an old item file was first added under its name. Not `--follow`:
/// on small, similar files git takes one item for a rename of another. An
/// item that changed lane dates from its last lane change, close enough for
/// an order.
fn created(h: Option<&History>, name: &str) -> String {
    let Some(h) = h else {
        return String::new();
    };
    h.git(&[
        "log",
        "--diff-filter=A",
        "--format=%aI",
        "--",
        &format!("items/{name}"),
    ])
    .ok()
    .and_then(|out| out.lines().last().map(str::to_string))
    .unwrap_or_default()
}

/// Numbers a board's items once, in creation order, and rewrites every
/// reference. Returns (old id, new id) per item; nothing when the board is
/// already numbered.
pub fn migrate(p: &Project) -> Result<Vec<(String, String)>> {
    let _l = store::lock(p)?;
    let names: Vec<String> = fs::read_dir(p.items_dir())
        .with_context(|| format!("{} has no items/ folder", p.root.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    if !names.iter().any(|n| old_style_name(n)) {
        return Ok(vec![]);
    }
    let history = History::open(p)?;
    if let Some(h) = &history {
        h.record_outside()?;
    }
    let mut old_items: Vec<(String, (String, u32), String)> = names
        .iter()
        .filter(|n| old_style_name(n) && n.matches('.').count() == 1)
        .filter_map(|n| Some((n.clone(), old_parts(n)?)))
        .map(|(n, parts)| {
            let at = created(history.as_ref(), &n);
            (n, parts, at)
        })
        .collect();
    old_items.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then(kind_rank(&a.1.0).cmp(&kind_rank(&b.1.0)))
            .then(a.1.1.cmp(&b.1.1))
    });
    // documents whose item is not on the board (left behind) keep their
    // owner's place: a number after the items, so references still work
    let mut orphans: Vec<(String, u32)> = names
        .iter()
        .filter(|n| old_style_name(n) && n.matches('.').count() > 1)
        .filter_map(|n| old_parts(n))
        .filter(|(k, num)| {
            !old_items
                .iter()
                .any(|(_, (ik, inum), _)| ik == k && inum == num)
        })
        .collect();
    orphans.sort_by(|a, b| kind_rank(&a.0).cmp(&kind_rank(&b.0)).then(a.1.cmp(&b.1)));
    orphans.dedup();
    let map: BTreeMap<String, String> = old_items
        .iter()
        .map(|(_, (kind, num), _)| format!("{kind}:{num}"))
        .chain(orphans.iter().map(|(kind, num)| format!("{kind}:{num}")))
        .enumerate()
        .map(|(i, old)| (old, id_of(i as u32 + 1)))
        .collect();

    // read and convert everything first: an error leaves the board as it was
    let mut writes: Vec<(String, String)> = vec![]; // (new name, text)
    let mut removes: Vec<String> = vec![];
    let mut moved = vec![];
    for (name, (kind, num), _) in &old_items {
        let old = format!("{kind}:{num}");
        let new = map[&old].clone();
        let text = fs::read_to_string(p.items_dir().join(name))?;
        // old files have no kind: it was in the name
        let text = match text.find("\ntitle:") {
            Some(i) => {
                let end = text[i + 1..].find('\n').map_or(text.len(), |j| i + 1 + j);
                format!("{}\nkind: {kind}{}", &text[..end], &text[end..])
            }
            None => text,
        };
        let mut it = item_file::parse(&file_name(&new), &text)
            .with_context(|| format!("cannot read items/{name}"))?;
        it.needs = it
            .needs
            .iter()
            .map(|n| map.get(n).cloned().unwrap_or_else(|| n.clone()))
            .collect();
        it.docs = it.docs.iter().map(|d| rewrite_refs(d, &map)).collect();
        it.body = rewrite_refs(&it.body, &map);
        writes.push((file_name(&new), item_file::write(&it)));
        removes.push(name.clone());
        moved.push((old, new));
    }
    for name in names
        .iter()
        .filter(|n| old_style_name(n) && n.matches('.').count() > 1)
    {
        let Some((kind, num)) = old_parts(name) else {
            continue;
        };
        let new = &map[&format!("{kind}:{num}")];
        let rest = &name[name.find('.').expect("a document name has a dot")..];
        let text = fs::read_to_string(p.items_dir().join(name))?;
        writes.push((format!("{new}{rest}"), rewrite_refs(&text, &map)));
        removes.push(name.clone());
    }
    let mut conflicts = vec![];
    for (old, new) in &moved {
        let file = p
            .root
            .join("conflicts")
            .join(format!("{}.json", old.replacen(':', "-", 1)));
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let mut c: crate::conflicts::Conflict = serde_json::from_str(&text)
            .with_context(|| format!("{} is not a conflict record", file.display()))?;
        c.id = new.clone();
        for d in c.documents.iter_mut() {
            d.path = rewrite_refs(&d.path, &map);
            d.other = rewrite_refs(&d.other, &map);
        }
        conflicts.push((file, c));
    }

    // then write
    for (name, text) in &writes {
        store::write_atomic(&p.items_dir().join(name), text)?;
    }
    for name in &removes {
        fs::remove_file(p.items_dir().join(name))?;
    }
    for (file, c) in &conflicts {
        fs::remove_file(file)?;
        crate::conflicts::write(p, c)?;
    }
    store::settle_locked(p)?;
    if let Some(h) = &history {
        h.commit("number every item")?;
    }
    Ok(moved)
}
