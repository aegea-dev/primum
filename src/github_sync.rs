//! One GitHub sync of one project, against any `Api`. `sync` calls it with
//! the project lock held and commits afterwards.
//!
//! For each linked item, `github/<STEM>.json` is the version both sides last
//! agreed on: the base of KILLER:2's three-way `merge_item` between the item
//! and its issue. A field both sides changed is a conflict; it is left as
//! GitHub has it until resolved here.
use crate::conflicts::{self, BodyValues, Conflict, FieldValues, Side};
use crate::github::{Api, Issue, IssueWrite, issue_write, item_from_issue, split_block};
use crate::item_file::{self, file_name, stem};
use crate::merge::merge_item;
use crate::model::Item;
use crate::store::{self, Board, Project};
use anyhow::Result;
use regex::Regex;
use std::path::PathBuf;
use std::sync::LazyLock;

#[derive(Debug, Default)]
pub struct Report {
    /// Items that became issues.
    pub created: Vec<String>,
    /// Issues that became items.
    pub added: Vec<String>,
    /// Items changed from GitHub.
    pub updated_items: Vec<String>,
    /// Issues changed from here.
    pub updated_issues: Vec<u64>,
    /// (item, pull request) closed by a primum id in a merged PR.
    pub closed_by: Vec<(String, u64)>,
    pub conflicts: Vec<String>,
}

static CLOSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s*:?\s+((?:URGENT|BUG|DEBT|KILLER|TODO):\d+)\b",
    )
    .expect("regex")
});

pub fn base_path(p: &Project, id: &str) -> PathBuf {
    p.root.join("github").join(format!("{}.json", stem(id)))
}

pub(crate) fn deleted_path(p: &Project) -> PathBuf {
    p.root.join("github").join("deleted.json")
}

/// The agreed version, kept as the item's own file text so it parses with
/// the same rules.
fn read_base(p: &Project, id: &str) -> Option<Item> {
    let text = std::fs::read_to_string(base_path(p, id)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    item_file::parse(&file_name(id), v["item"].as_str()?).ok()
}

fn write_base(p: &Project, it: &Item) -> Result<()> {
    std::fs::create_dir_all(p.root.join("github"))?;
    let v = serde_json::json!({ "item": item_file::write(it) });
    std::fs::write(
        base_path(p, &it.id),
        serde_json::to_string_pretty(&v)? + "\n",
    )?;
    Ok(())
}

fn write_item(p: &Project, it: &Item) -> Result<()> {
    std::fs::write(p.items_dir().join(file_name(&it.id)), item_file::write(it))?;
    Ok(())
}

/// The fields GitHub carries; only these are compared. The id is one: a
/// kind or number change here rewrites the issue's block.
fn carried(it: &Item) -> (&str, &str, &str, &str, &str, [u8; 4], &[String]) {
    (
        &it.id,
        &it.title,
        &it.body,
        &it.status,
        &it.kind,
        [it.impact, it.urgency, it.risk, it.effort],
        &it.needs,
    )
}

fn writer(board: &Board) -> impl Fn(&Item) -> IssueWrite + '_ {
    move |it| issue_write(it, &|id| board.get(id).and_then(|i| i.github))
}

pub fn run(p: &Project, api: &dyn Api, since: Option<&str>) -> Result<(Report, Option<String>)> {
    run_with(p, api, since, false)
}

/// One sync. `all` also files closed items as issues and takes in closed
/// issues. Returns the report and the newest `updated` seen, the next
/// `since`.
pub fn run_with(
    p: &Project,
    api: &dyn Api,
    since: Option<&str>,
    all: bool,
) -> Result<(Report, Option<String>)> {
    let mut report = Report::default();
    let incoming = api.issues_since(since)?;
    let newest = incoming
        .iter()
        .map(|i| i.updated.clone())
        .max()
        .or(since.map(str::to_string));

    close_deleted(p, api)?;
    closing_keywords(p, &incoming, &mut report)?;

    for issue in incoming.iter().filter(|i| !i.is_pr) {
        let board = store::load(p)?;
        if let Some(it) = board.items.iter().find(|i| i.github == Some(issue.number)) {
            merge_issue(p, api, &it.id, issue, &mut report)?;
            continue;
        }
        let meta = split_block(&issue.body).1;
        match meta.as_ref().and_then(|m| board.get(&m.id)) {
            // an issue filed for an item here by another machine: link it
            Some(it) if it.github.is_none() && issue.open && it.title == issue.title => {
                let mut it = it.clone();
                it.github = Some(issue.number);
                write_item(p, &it)?;
                write_base(p, &it)?;
            }
            // a second issue for an item that already has one
            Some(it) if it.github.is_some() && issue.open => {
                let first = it.github.expect("checked");
                api.comment(issue.number, &format!("Duplicate of #{first}."))?;
                api.close(issue.number, "not_planned")?;
            }
            // primum's issue for an item no longer here, or a stale one
            Some(_) => {}
            None if meta.is_some() => {}
            // an issue opened on GitHub
            None => {
                if !issue.open && !all {
                    continue;
                }
                let mut it = item_from_issue(issue, &Item::new("TODO", 0, ""));
                it.source = format!("GitHub #{}", issue.number);
                let id = store::insert_locked(p, it)?;
                let board = store::load(p)?;
                let it = board.get(&id).expect("inserted").clone();
                api.update(issue.number, &writer(&board)(&it))?;
                write_base(p, &it)?;
                report.added.push(id);
            }
        }
    }

    let board = store::load(p)?;
    let write = writer(&board);
    for it in &board.items {
        match it.github {
            None if it.is_closed() && !all => {}
            None => {
                let created = api.create(&write(it))?;
                let mut linked = it.clone();
                linked.github = Some(created.number);
                write_item(p, &linked)?;
                write_base(p, &linked)?;
                report.created.push(it.id.clone());
            }
            Some(n) => {
                let changed = read_base(p, &it.id).is_none_or(|b| carried(&b) != carried(it));
                if changed
                    && !board.conflicts.contains_key(&it.id)
                    && push(p, api, it, n, &write(it))?
                {
                    report.updated_issues.push(n);
                }
            }
        }
    }
    store::settle_locked(p)?;
    Ok((report, newest))
}

/// Closes the issues of items deleted here since the last sync.
fn close_deleted(p: &Project, api: &dyn Api) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(deleted_path(p)) else {
        return Ok(());
    };
    let numbers: Vec<u64> = serde_json::from_str(&text).unwrap_or_default();
    for n in numbers {
        // an issue already gone needs no closing
        if api.comment(n, "Deleted in primum.").is_ok() {
            api.close(n, "not_planned")?;
        }
    }
    std::fs::remove_file(deleted_path(p))?;
    Ok(())
}

/// `Fixes TODO:87` in a merged pull request closes TODO:87.
fn closing_keywords(p: &Project, incoming: &[Issue], report: &mut Report) -> Result<()> {
    for pr in incoming.iter().filter(|i| i.is_pr && i.pr_merged) {
        let text = format!("{}\n{}", pr.title, pr.body);
        for cap in CLOSING.captures_iter(&text) {
            let id = cap[1].to_uppercase();
            let board = store::load(p)?;
            let Some(it) = board.get(&id).filter(|i| !i.is_closed()) else {
                continue;
            };
            let date = chrono::Local::now().format("%Y-%m-%d");
            let note = format!("## {date}\n\nclosed by #{}\n", pr.number);
            let body = if it.body.trim().is_empty() {
                note
            } else {
                format!("{}\n\n---\n\n{note}", it.body.trim_end())
            };
            let c = store::Changes {
                status: Some("done".into()),
                body: Some(body),
                ..Default::default()
            };
            store::update_locked_by_id(p, &id, c)?;
            report.closed_by.push((id, pr.number));
        }
    }
    Ok(())
}

/// The item with `theirs`'s value for each field in `fields`.
fn with_their_values(item: &Item, theirs: &Item, fields: &[String], body: bool) -> Item {
    let mut out = item.clone();
    for f in fields {
        match f.as_str() {
            "title" => out.title = theirs.title.clone(),
            "status" => {
                out.status = theirs.status.clone();
                out.finished = theirs.finished.clone();
            }
            "impact" => out.impact = theirs.impact,
            "urgency" => out.urgency = theirs.urgency,
            "risk" => out.risk = theirs.risk,
            "effort" => out.effort = theirs.effort,
            "kind" => out.kind = theirs.kind.clone(),
            _ => {}
        }
    }
    if body {
        out.body = theirs.body.clone();
    }
    out
}

/// Merges a changed issue into its item.
fn merge_issue(
    p: &Project,
    api: &dyn Api,
    id: &str,
    issue: &Issue,
    report: &mut Report,
) -> Result<()> {
    let ours = store::load(p)?.get(id).expect("linked").clone();
    let theirs = item_from_issue(issue, &ours);
    let base = read_base(p, id);
    if base
        .as_ref()
        .is_some_and(|b| carried(b) == carried(&theirs))
    {
        return Ok(()); // nothing changed on GitHub; our changes go out after
    }
    if carried(&ours) == carried(&theirs) {
        return write_base(p, &ours);
    }
    let text = |b: &str, o: &str, t: &str| -> Option<String> {
        if o == b {
            Some(t.to_string())
        } else if t == b {
            Some(o.to_string())
        } else {
            None
        }
    };
    let m = merge_item(base.as_ref(), &ours, &theirs, &text);
    let mut item = m.item.clone();
    let mut fields: Vec<FieldValues> = m
        .fields
        .into_iter()
        .map(|f| FieldValues {
            field: f.field,
            values: f.values,
        })
        .collect();

    // kind: changed on GitHub only moves the item; on both, a conflict
    let base_kind = base
        .as_ref()
        .map_or(ours.kind.as_str(), |b| b.kind.as_str());
    let mut id = id.to_string();
    if theirs.kind != ours.kind {
        if ours.kind == base_kind {
            let num = store::load(p)?.next_num_of(&theirs.kind);
            id = store::move_locked(p, &id, &theirs.kind, num)?;
            let moved = store::load(p)?.get(&id).expect("moved").clone();
            item = moved.with_fields_of(&item);
        } else if theirs.kind != base_kind {
            fields.push(FieldValues {
                field: "kind".into(),
                values: [ours.kind.clone(), theirs.kind.clone()],
            });
        }
    }
    write_item(p, &item)?;
    report.updated_items.push(id.clone());
    if fields.is_empty() && m.body.is_none() {
        let board = store::load(p)?;
        if carried(&item) != carried(&theirs)
            && push(p, api, &item, issue.number, &writer(&board)(&item))?
        {
            report.updated_issues.push(issue.number);
        } else {
            write_base(p, &item)?;
        }
        return Ok(());
    }
    let names: Vec<String> = fields.iter().map(|f| f.field.clone()).collect();
    write_base(
        p,
        &with_their_values(&item, &theirs, &names, m.body.is_some()),
    )?;
    let conflict = Conflict {
        id: id.clone(),
        sides: [
            Side {
                author: "this board".into(),
                date: String::new(),
            },
            Side {
                author: format!("GitHub ({})", issue.author),
                date: issue.updated.clone(),
            },
        ],
        fields,
        body: m.body.map(|values| BodyValues { values }),
        documents: vec![],
    };
    let mut record = store::load(p)?.conflicts.remove(&id);
    let merged = match record.take() {
        Some(mut existing) => {
            existing.add(conflict);
            existing
        }
        None => conflict,
    };
    conflicts::write(p, &merged)?;
    report.conflicts.push(id);
    Ok(())
}

/// Writes an item's issue and records the agreed version. An issue that no
/// longer exists (deleted on GitHub) leaves the item without its link;
/// returns false then.
fn push(p: &Project, api: &dyn Api, it: &Item, n: u64, w: &IssueWrite) -> Result<bool> {
    match api.update(n, w) {
        Ok(_) => {
            write_base(p, it)?;
            Ok(true)
        }
        Err(e) if e.to_string().contains("404") => {
            let mut unlinked = it.clone();
            unlinked.github = None;
            write_item(p, &unlinked)?;
            let _ = std::fs::remove_file(base_path(p, &it.id));
            Ok(false)
        }
        Err(e) => Err(e),
    }
}
