//! The daemon's work for one project, one pass at a time: write ticket
//! changes left from before, read the ledger for changes, fire due timers,
//! deliver every waiting event to its ticket's manager, save each answer,
//! write the ticket changes it asks for, and run the jobs it asks for in
//! workers. The daemon decides nothing: every change comes from a
//! manager's answer.
use super::journal::{self, Envelope, Handled, Journal, Recorded};
use super::{Answer, Event, Input, Interpreter, Job, Program, Runtime};
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
    /// Jobs run in workers.
    pub worked: usize,
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
    // tickets' events first; then one job, whose result is the next event
    loop {
        while let Some(r) = j.next()? {
            deliver(p, &mut j, &r, now, manager)?;
            pass.delivered += 1;
            pass.written += write_ledger(p, &j)?;
            if pass.delivered >= MAX_EVENTS {
                return Ok(pass);
            }
        }
        let Some(r) = j.next_job()? else { break };
        work(p, &mut j, &r, now, manager)?;
        pass.worked += 1;
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

/// Seconds a built-in may take.
const BUILT_IN_TIMEOUT: u64 = 60;

/// `${env.NAME}` in every string, filled in from the OS secret store, else
/// the daemon's environment.
fn fill_secrets(v: &Value) -> Result<Value, String> {
    Ok(match v {
        Value::String(s) => {
            use super::eval::{Piece, pieces, secret_name};
            let mut out = String::new();
            for piece in pieces(s)? {
                match piece {
                    Piece::Text(t) => out += t,
                    Piece::Expr(e) => match secret_name(e) {
                        Some(name) => {
                            out += &super::secrets::get(name)
                                .ok_or_else(|| format!("secret {name} is not set"))?
                        }
                        // any other template was filled in by the manager
                        None => out += e,
                    },
                }
            }
            Value::String(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(fill_secrets).collect::<Result<_, _>>()?),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| Ok((k.clone(), fill_secrets(x)?)))
                .collect::<Result<_, String>>()?,
        ),
        other => other.clone(),
    })
}

/// The job for an `action.requested` event: arguments with secrets, and
/// for a project actionable, the program that runs it.
fn job(p: &Project, data: &Value) -> Result<Job, String> {
    let get = |k: &str| data.get(k).cloned().unwrap_or(Value::Null);
    let obj = |k: &str| get(k).as_object().cloned().unwrap_or_default();
    let action = get("action").as_str().unwrap_or("").to_string();
    let with = match fill_secrets(&get("with"))? {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    let mut job = Job {
        action: action.clone(),
        with,
        key: get("key").as_str().unwrap_or("").to_string(),
        input: get("input"),
        prev: get("prev"),
        ticket: obj("ticket"),
        project: obj("project"),
        args: obj("args"),
        root: p.root.clone(),
        program: None,
        timeout: BUILT_IN_TIMEOUT,
    };
    if action == "http.request@1" || action == "command@1" {
        return Ok(job);
    }
    let (what, version) = action
        .rsplit_once('@')
        .ok_or_else(|| format!("unknown actionable {action}"))?;
    let (name, target) = what
        .split_once('.')
        .ok_or_else(|| format!("unknown actionable {action}"))?;
    let defs = crate::lifecycle::definitions::load_all(p).map_err(|e| e.to_string())?;
    let d = defs
        .iter()
        .find(|d| d.name == name)
        .ok_or_else(|| format!("no actionable {name} in actionables/"))?;
    if d.version.to_string() != version {
        return Err(format!(
            "actionable {name} is at version {} now, not {version}",
            d.version
        ));
    }
    let t = d
        .targets
        .get(target)
        .ok_or_else(|| format!("actionable {name} has no target {target}"))?;
    if let Some(limit) = d.limits.as_ref().and_then(|l| l.timeout.as_deref()) {
        job.timeout = crate::lifecycle::expr::duration(limit)?.as_secs().max(1);
    }
    let folder = crate::lifecycle::definitions::dir(p).join(&d.name);
    let folder = match &d.working_directory {
        Some(w) => folder.join(w),
        None => folder,
    };
    let (kind, version) = d.kind.split_once('@').unwrap_or((d.kind.as_str(), ""));
    let interpreter = match kind {
        "executable" => None,
        "python" => Some(Interpreter::Python {
            version: version.to_string(),
            home: d.home.clone().unwrap_or_else(|| ".venv".into()),
            requirements: d.requirements.clone(),
            module: t.module.clone().unwrap_or_default(),
            callable: t.callable.clone().unwrap_or_default(),
        }),
        "bun" => Some(Interpreter::Bun {
            file: t.file.clone().unwrap_or_default(),
            export: t.export.clone().unwrap_or_default(),
        }),
        other => return Err(format!("unknown actionable type {other}")),
    };
    let mut environment = std::collections::BTreeMap::new();
    for (k, v) in &d.environment {
        let filled = fill_secrets(&Value::String(v.clone()))?;
        environment.insert(k.clone(), filled.as_str().unwrap_or("").to_string());
    }
    job.program = Some(Program {
        folder,
        executable: t.executable.clone().unwrap_or_default(),
        arguments: t.arguments.clone(),
        environment,
        interpreter,
    });
    Ok(job)
}

/// Runs `primum worker` on a job in its own process group, killing the
/// group at the job's time limit.
fn spawn_worker(manager: &Path, job: &Job) -> Result<Value, String> {
    let mut c = Command::new(manager);
    c.arg("worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut c, 0);
    let mut child = c
        .spawn()
        .map_err(|e| format!("cannot start a worker: {e}"))?;
    let text = serde_json::to_string(job).map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .expect("piped")
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    let read = |r: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = vec![];
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = read(
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    );
    let err = read(
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(job.timeout);
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if std::time::Instant::now() >= deadline {
            #[cfg(unix)]
            // SAFETY: killpg only sends a signal to the worker's own group
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{}: timed out after {}s", job.action, job.timeout));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let (out, err) = (
        out.join().unwrap_or_default(),
        err.join().unwrap_or_default(),
    );
    if !status.success() {
        let why = String::from_utf8_lossy(&err).trim().to_string();
        let why = why.strip_prefix("error: ").unwrap_or(&why).to_string();
        return Err(if why.is_empty() {
            "the worker failed".into()
        } else {
            why
        });
    }
    let v: Value = serde_json::from_slice(&out).map_err(|e| format!("the worker's answer: {e}"))?;
    Ok(v.get("output").cloned().unwrap_or(Value::Null))
}

/// Runs one job and sends its result to the ticket, in one transaction.
fn work(
    p: &Project,
    j: &mut Journal,
    r: &Recorded,
    now: DateTime<Utc>,
    manager: &Path,
) -> Result<()> {
    let data = &r.event.data;
    let ticket = data
        .pointer("/ticket/id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let outcome = job(p, data).and_then(|job| spawn_worker(manager, &job));
    let (kind, result) = match outcome {
        Ok(output) => ("action.finished", json!({"output": output})),
        Err(why) => ("action.failed", json!({"error": why})),
    };
    let mut result = result;
    result["run"] = data.get("run").cloned().unwrap_or(Value::Null);
    result["index"] = data.get("index").cloned().unwrap_or(Value::Null);
    let e = Envelope::new(
        &format!("worker/{}", r.event.id),
        kind,
        &journal::subject(&p.name, &ticket),
        &stamp(now),
        Some(r.event.id.clone()),
        result,
    );
    j.handle(Handled {
        seq: r.seq,
        error: None,
        ticket: None,
        timers: None,
        events: vec![e],
        ledger: None,
    })
}
