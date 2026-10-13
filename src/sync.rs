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

/// Whether changes to `p` sync by themselves: it has a remote, and sync
/// is not manual.
pub fn is_auto(p: &Project) -> bool {
    History::open(p)
        .ok()
        .flatten()
        .and_then(|h| h.remote().ok().flatten())
        .is_some_and(|r| !r.manual)
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
        if h.head()?.is_none() {
            return Ok(report);
        }
        let known = h
            .run(&["rev-parse", "--verify", "--quiet", &theirs])?
            .status
            .success();
        let ahead = !known
            || h.git(&["rev-list", "--count", &format!("{theirs}..HEAD")])?
                .trim()
                != "0";
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

fn pull(
    p: &Project,
    h: &History,
    theirs: &str,
    remote: &Remote,
    report: &mut Report,
) -> Result<()> {
    if h.head()?.is_none() {
        // a project with nothing yet takes the remote's history
        h.git(&["merge", "--quiet", "--ff-only", theirs])?;
        report.pulled = true;
        return Ok(());
    }
    let is_ancestor = |a: &str, b: &str| -> Result<bool> {
        Ok(h.run(&["merge-base", "--is-ancestor", a, b])?
            .status
            .success())
    };
    if is_ancestor(theirs, "HEAD")? {
        return Ok(()); // nothing new on the remote
    }
    let base = h.run(&["merge-base", "HEAD", theirs])?;
    if !base.status.success() {
        bail!(
            "this project's history and the remote's ({} {}) are unrelated; \
             make the project with primum clone instead",
            remote.url,
            remote.branch
        );
    }
    let base = String::from_utf8_lossy(&base.stdout).trim().to_string();
    if is_ancestor("HEAD", theirs)? {
        h.git(&["merge", "--quiet", "--ff-only", theirs])?;
        report.pulled = true;
        return Ok(());
    }
    renumber(p, h, &base, theirs, report)?;
    merge(p, h, theirs, remote, report)?;
    report.pulled = true;
    Ok(())
}

/// Item files added between `base` and `rev`.
fn added_items(h: &History, base: &str, rev: &str) -> Result<BTreeSet<String>> {
    let out = h.git(&[
        "diff",
        "--name-only",
        "--no-renames",
        "--diff-filter=A",
        base,
        rev,
        "--",
        "items/",
    ])?;
    Ok(out
        .lines()
        .filter_map(|l| l.strip_prefix("items/"))
        .filter(|n| parse_item_name(n).is_some())
        .map(str::to_string)
        .collect())
}

/// A new local item whose id was also taken on the remote moves to the
/// next number free on both sides. The remote's numbers never change,
/// since others may already use them.
fn renumber(p: &Project, h: &History, base: &str, theirs: &str, report: &mut Report) -> Result<()> {
    let ours = added_items(h, base, "HEAD")?;
    let remote_new = added_items(h, base, theirs)?;
    let remote_names: Vec<(String, u32)> = h
        .git(&["ls-tree", "--name-only", theirs, "items/"])?
        .lines()
        .filter_map(|l| parse_item_name(l.strip_prefix("items/")?))
        .collect();
    for name in ours.intersection(&remote_new) {
        let (kind, num) = parse_item_name(name).expect("filtered above");
        let old = id_of(&kind, num);
        let board = store::load(p)?;
        let taken = |n: u32| {
            board.get(&id_of(&kind, n)).is_some() || remote_names.contains(&(kind.clone(), n))
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

fn merge(
    p: &Project,
    h: &History,
    theirs: &str,
    remote: &Remote,
    report: &mut Report,
) -> Result<()> {
    let out = h.run(&["merge", "--no-ff", "--no-commit", "--quiet", theirs])?;
    let unmerged: Vec<String> = h
        .git(&["diff", "--name-only", "--diff-filter=U"])?
        .lines()
        .map(str::to_string)
        .collect();
    if !out.status.success() && unmerged.is_empty() {
        let _ = h.run(&["merge", "--abort"]);
        bail!(
            "git merge failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    if let Err(e) = resolve_all(p, h, theirs, &unmerged, report) {
        let _ = h.run(&["merge", "--abort"]);
        return Err(e);
    }
    store::settle_locked(p)?;
    h.git(&["add", "--all", "--", "."])?;
    let message = format!("sync with {} {}", remote.branch, remote.url);
    h.git_as_author(&["commit", "--quiet", "--no-verify", "-m", &message])?;
    Ok(())
}

/// Settles every path git left unmerged, by primum's rules.
fn resolve_all(
    p: &Project,
    h: &History,
    theirs: &str,
    paths: &[String],
    report: &mut Report,
) -> Result<()> {
    let text = |b: &str, o: &str, t: &str| h.merge_text(b, o, t).ok().flatten();
    for path in paths {
        let (base, ours, other) = (h.stage(1, path)?, h.stage(2, path)?, h.stage(3, path)?);
        let file = p.root.join(path);
        let name = path.rsplit('/').next().unwrap_or(path);
        if path == "BOARD.md" {
            // rebuilt by settle after the merge
            std::fs::write(&file, ours.or(other).unwrap_or_default())?;
        } else if path.starts_with("items/") && parse_item_name(name).is_some() {
            match (ours, other) {
                (Some(o), Some(t)) => {
                    let parse = |s: &str| item_file::parse(name, s);
                    let base = base.as_deref().map(parse).transpose()?;
                    let (o, t) = (parse(&o)?, parse(&t)?);
                    let m = merge_item(base.as_ref(), &o, &t, &text);
                    std::fs::write(&file, item_file::write(&m.item))?;
                    if !m.fields.is_empty() || m.body.is_some() {
                        let fields = m
                            .fields
                            .into_iter()
                            .map(|f| FieldValues {
                                field: f.field,
                                values: f.values,
                            })
                            .collect();
                        record(
                            p,
                            Conflict {
                                id: m.item.id.clone(),
                                sides: [h.side("HEAD", path)?, h.side(theirs, path)?],
                                fields,
                                body: m.body.map(|values| BodyValues { values }),
                                documents: vec![],
                            },
                        )?;
                        report.conflicts.push(m.item.id.clone());
                    }
                }
                (Some(kept), None) | (None, Some(kept)) => {
                    std::fs::write(&file, &kept)?;
                    let id = parse_item_name(name)
                        .map(|(k, n)| id_of(&k, n))
                        .unwrap_or_default();
                    report.kept.push(format!(
                        "{id} was deleted on one side and changed on the other; kept"
                    ));
                }
                (None, None) => {}
            }
        } else if path.starts_with("items/") && parse_doc_name(name).is_some() {
            match (ours, other) {
                (Some(o), Some(t)) => match text(base.as_deref().unwrap_or(""), &o, &t) {
                    Some(merged) => std::fs::write(&file, merged)?,
                    None => {
                        std::fs::write(&file, &o)?;
                        let short = h.git(&["rev-parse", "--short", theirs])?;
                        let stem = name.strip_suffix(".md").unwrap_or(name);
                        let other_name = format!("{stem}.{}.md", short.trim());
                        std::fs::write(p.items_dir().join(&other_name), &t)?;
                        let (kind, num, _) = parse_doc_name(name).expect("checked");
                        let id = id_of(&kind, num);
                        record(
                            p,
                            Conflict {
                                id: id.clone(),
                                sides: [h.side("HEAD", path)?, h.side(theirs, path)?],
                                fields: vec![],
                                body: None,
                                documents: vec![DocConflict {
                                    path: path.clone(),
                                    other: format!("items/{other_name}"),
                                }],
                            },
                        )?;
                        report.conflicts.push(id);
                    }
                },
                (Some(kept), None) | (None, Some(kept)) => std::fs::write(&file, kept)?,
                (None, None) => {}
            }
        } else if path.starts_with("conflicts/") {
            let parse = |s: &Option<String>| {
                s.as_deref()
                    .and_then(|t| serde_json::from_str::<Conflict>(t).ok())
            };
            if let Some(mut c) = parse(&ours).or_else(|| parse(&other)) {
                if let Some(t) = parse(&other) {
                    c.add(t);
                }
                std::fs::write(&file, serde_json::to_string_pretty(&c)? + "\n")?;
            }
        } else {
            std::fs::write(&file, ours.or(other).unwrap_or_default())?;
            report
                .kept
                .push(format!("{path} changed on both sides; kept this side's"));
        }
        h.git(&["add", "--all", "--", path])?;
    }
    report.conflicts.sort();
    report.conflicts.dedup();
    Ok(())
}

/// Adds `c` to the item's conflict record.
fn record(p: &Project, c: Conflict) -> Result<()> {
    let merged = match conflicts::read_all(p)?.remove(&c.id) {
        Some(mut existing) => {
            existing.add(c);
            existing
        }
        None => c,
    };
    conflicts::write(p, &merged)
}
