//! The daemon's work for one project, one pass at a time: write ticket
//! changes left from before, read the ledger for changes, fire due timers,
//! deliver every waiting event to its ticket's manager, save each answer,
//! and write the ticket changes it asks for. The daemon decides nothing:
//! every change comes from a manager's answer.
use super::journal::{self, Envelope, Handled, Journal, Recorded};
use super::{Answer, Event, Input, Runtime};
use crate::lifecycle::{File, split_pin};
use crate::store::{self, Changes, Project};
use anyhow::{Result, anyhow};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// What one pass did.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Pass {
    /// Events answered.
    pub delivered: usize,
    /// Ticket changes written to the ledger.
    pub written: usize,
}

/// More events than this in one pass wait for the next.
const MAX_EVENTS: usize = 10_000;

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// One pass over a project; `manager` is the primum binary to run as
/// `primum manager`.
pub fn run_once(p: &Project, now: DateTime<Utc>, manager: &Path) -> Result<Pass> {
    let mut j = Journal::open(p)?;
    let mut pass = Pass::default();
    // changes left from a pass that stopped early, then the feed's own
    pass.written += write_ledger(p, &j)?;
    feed(p, &j, now)?;
    pass.written += write_ledger(p, &j)?;
    for (id, ticket, data) in j.due_timers(&stamp(now))? {
        let e = Envelope::new(
            "daemon",
            "timer.fired",
            &journal::subject(&p.name, &ticket),
            &stamp(now),
            None,
            data,
        );
        j.fire_timer(id, &e)?;
    }
    while let Some(r) = j.next()? {
        deliver(p, &mut j, &r, now, manager)?;
        pass.delivered += 1;
        pass.written += write_ledger(p, &j)?;
        if pass.delivered >= MAX_EVENTS {
            break;
        }
    }
    Ok(pass)
}

/// An item's fields as the feed compares them.
fn snapshot(it: &crate::model::Item) -> Value {
    serde_json::to_value(it).expect("an item serializes")
}

fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The item file's and project.json's contents, to notice changes made
/// while a manager ran.
fn revisions(p: &Project, id: &str) -> (String, String) {
    let read = |path: std::path::PathBuf| std::fs::read_to_string(path).unwrap_or_default();
    (
        sha(&read(p.items_dir().join(crate::item_file::file_name(id)))),
        sha(&read(p.root.join("project.json"))),
    )
}

/// Turns ledger changes into events: every item is compared with the
/// snapshot the journal saw last.
fn feed(p: &Project, j: &Journal, now: DateTime<Utc>) -> Result<()> {
    let board = {
        let _l = store::lock(p)?;
        if let Some(h) = crate::history::History::open(p)? {
            h.record_outside()?;
        }
        store::load(p)?
    };
    let seen = j.seen_all()?;
    let first = seen.is_empty();
    for it in &board.items {
        let snap = snapshot(it);
        let old = seen.get(&it.id);
        if old == Some(&snap) {
            continue;
        }
        j.set_seen(&it.id, &snap)?;
        let Some(pin) = &it.lifecycle else {
            if j.ticket(&it.id)?.is_some() {
                j.forget_ticket(&it.id)?;
            }
            continue;
        };
        let subject = journal::subject(&p.name, &it.id);
        let running = j.ticket(&it.id)?;
        if running.as_ref().map(|(pin, _)| pin) != Some(pin) {
            let Some(f) = definition(p, j, pin)? else {
                let mut c = Map::new();
                c.insert("status".into(), json!(crate::lifecycle::FAILED));
                c.insert(
                    "note".into(),
                    json!(format!(
                        "Lifecycle {pin} is no longer in lifecycles/; attach it again to start it."
                    )),
                );
                j.queue_ledger(&it.id, &c)?;
                continue;
            };
            let rt = Runtime {
                state: f.start.clone(),
                chain: None,
                failed: None,
            };
            j.start_ticket(&it.id, pin, &rt)?;
            let kind = if old.is_none() && !first {
                "ticket.created"
            } else {
                "lifecycle.attached"
            };
            j.publish(&Envelope::new(
                "ledger",
                kind,
                &subject,
                &stamp(now),
                None,
                json!({}),
            ))?;
            continue;
        }
        let mut data = Map::new();
        let fields: Vec<&str> = snap
            .as_object()
            .expect("an object")
            .iter()
            .filter(|(k, v)| old.and_then(|o| o.get(k.as_str())) != Some(v))
            .map(|(k, _)| k.as_str())
            .collect();
        data.insert("fields".into(), json!(fields));
        if fields.contains(&"status") {
            data.insert("status".into(), json!(it.status));
        }
        j.publish(&Envelope::new(
            "ledger",
            "ticket.changed",
            &subject,
            &stamp(now),
            None,
            Value::Object(data),
        ))?;
    }
    for id in seen.keys() {
        if board.get(id).is_none() {
            j.forget_seen(id)?;
            j.forget_ticket(id)?;
        }
    }
    Ok(())
}

/// A pinned lifecycle's definition: the journal's copy, else the file's
/// when it is still at that version (then kept in the journal).
fn definition(p: &Project, j: &Journal, pin: &str) -> Result<Option<File>> {
    if let Some(f) = j.lifecycle(pin)? {
        return Ok(Some(f));
    }
    let (name, version) = split_pin(pin);
    match crate::lifecycle::resolve(p, name) {
        Some(Ok(l)) if l.version == version => {
            j.pin_lifecycle(pin, &l.file)?;
            Ok(Some(l.file))
        }
        _ => Ok(None),
    }
}

/// Runs `primum manager` on one input.
fn ask(manager: &Path, input: &Input) -> Result<Answer, String> {
    let mut c = Command::new(manager)
        .arg("manager")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start the manager: {e}"))?;
    let text = serde_json::to_string(input).map_err(|e| e.to_string())?;
    c.stdin
        .take()
        .expect("piped")
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = c.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("the manager's answer: {e}"))
}

fn give_up(j: &mut Journal, seq: i64, why: String) -> Result<()> {
    j.handle(Handled {
        seq,
        error: Some(why),
        ticket: None,
        timers: None,
        events: vec![],
        ledger: None,
    })
}

/// Delivers one event to its ticket's manager and saves the answer.
fn deliver(
    p: &Project,
    j: &mut Journal,
    r: &Recorded,
    now: DateTime<Utc>,
    manager: &Path,
) -> Result<()> {
    let Some(id) = journal::ticket_of(&r.event.subject).map(str::to_string) else {
        return give_up(j, r.seq, "project lifecycles are not run yet".into());
    };
    let Some((pin, rt)) = j.ticket(&id)? else {
        return give_up(j, r.seq, format!("#{id} is not on a lifecycle"));
    };
    let Some(lifecycle) = j.lifecycle(&pin)? else {
        return give_up(j, r.seq, format!("lifecycle {pin} is not in the journal"));
    };
    let mut tries = 0;
    let (answer, it) = loop {
        tries += 1;
        let before = revisions(p, &id);
        let board = store::load(p)?;
        let Some(it) = board.get(&id).cloned() else {
            return give_up(j, r.seq, format!("#{id} is gone"));
        };
        let mut project = crate::projects::context(p)?;
        project.extend(store::computed_context(&board));
        let input = Input {
            now: stamp(now),
            lifecycle: lifecycle.clone(),
            ticket: snapshot(&it).as_object().cloned().unwrap_or_default(),
            args: it.args.clone().unwrap_or_default(),
            project,
            runtime: rt.clone(),
            event: Event {
                id: r.event.id.clone(),
                kind: r.event.kind.clone(),
                data: r.event.data.clone(),
            },
        };
        let answer = match ask(manager, &input) {
            Ok(a) => a,
            Err(why) => return give_up(j, r.seq, why),
        };
        if revisions(p, &id) == before || tries == 3 {
            break (answer, it);
        }
    };
    let mut changes = answer.changes.clone();
    if answer.status != it.status {
        changes.insert("status".into(), json!(answer.status));
    }
    let events = answer
        .events
        .iter()
        .map(|e| {
            Envelope::new(
                &format!("manager/{id}"),
                &e.kind,
                &journal::subject(&p.name, &e.target),
                &stamp(now),
                Some(r.event.id.clone()),
                e.data.clone(),
            )
        })
        .collect();
    for line in &answer.log {
        eprintln!("{}: #{id}: {line}", p.name);
    }
    j.handle(Handled {
        seq: r.seq,
        error: None,
        ticket: Some((&id, &answer.runtime)),
        timers: answer.timers.as_deref(),
        events,
        ledger: (!changes.is_empty()).then_some((&id, changes)),
    })
}

/// Writes queued ticket changes to the ledger, each once, and refreshes the
/// ticket's snapshot so the feed does not see the daemon's own change.
fn write_ledger(p: &Project, j: &Journal) -> Result<usize> {
    let pending = j.pending_ledger()?;
    for (row, id, changes) in &pending {
        if let Err(e) = write_one(p, id, changes) {
            eprintln!(
                "{}: #{id}: cannot write {}: {e:#}",
                p.name,
                Value::Object(changes.clone())
            );
        }
        j.ledger_written(*row)?;
        if let Some(it) = store::load(p)?.get(id) {
            j.set_seen(id, &snapshot(it))?;
        }
    }
    Ok(pending.len())
}

fn write_one(p: &Project, id: &str, changes: &Map<String, Value>) -> Result<()> {
    let text = |k: &str| changes.get(k).and_then(Value::as_str).map(str::to_string);
    let num = |k: &str| changes.get(k).and_then(Value::as_i64);
    let c = Changes {
        title: text("title"),
        status: text("status"),
        impact: num("impact"),
        urgency: num("urgency"),
        risk: num("risk"),
        effort: num("effort"),
        needs: changes.get("needs").and_then(Value::as_array).map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        }),
        body: text("body"),
        kind: None,
    };
    if !c.fields().is_empty() {
        store::update(p, id, c)?;
    }
    if let Some(note) = text("note") {
        let board = store::load(p)?;
        let it = board.get(id).ok_or_else(|| anyhow!("no item {id}"))?;
        if !it.body.contains(note.trim()) {
            store::append_note(p, id, &note)?;
        }
    }
    Ok(())
}
