//! The engine's journal: one SQLite file per project,
//! `.primum/journal.sqlite`, local and never synced. It is the event queue
//! and log, and keeps each running ticket's place, the lifecycle versions
//! tickets are pinned to, timers, ticket changes waiting to be written to
//! the ledger, and the snapshots the ledger feed compares against.
use super::{Runtime, Timer};
use crate::lifecycle::File;
use crate::store::Project;
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  id TEXT NOT NULL UNIQUE,
  time TEXT NOT NULL,
  type TEXT NOT NULL,
  source TEXT NOT NULL,
  subject TEXT NOT NULL,
  cause TEXT,
  data TEXT NOT NULL,
  handled TEXT,
  error TEXT
);
CREATE INDEX IF NOT EXISTS events_open ON events (handled, seq);
CREATE TABLE IF NOT EXISTS tickets (id TEXT PRIMARY KEY, pin TEXT NOT NULL, runtime TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS lifecycles (pin TEXT PRIMARY KEY, definition TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS timers (id INTEGER PRIMARY KEY AUTOINCREMENT, ticket TEXT NOT NULL, at TEXT NOT NULL, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS ledger (id INTEGER PRIMARY KEY AUTOINCREMENT, ticket TEXT NOT NULL, changes TEXT NOT NULL, written TEXT);
CREATE TABLE IF NOT EXISTS seen (id TEXT PRIMARY KEY, snapshot TEXT NOT NULL);
";

/// An event: the CloudEvents fields, and the event that led to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub id: String,
    pub source: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub subject: String,
    pub time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    #[serde(default)]
    pub data: Value,
}

impl Envelope {
    /// A new event with a fresh UUID v7 id.
    pub fn new(
        source: &str,
        kind: &str,
        subject: &str,
        time: &str,
        cause: Option<String>,
        data: Value,
    ) -> Envelope {
        Envelope {
            id: uuid::Uuid::now_v7().to_string(),
            source: source.into(),
            kind: kind.into(),
            subject: subject.into(),
            time: time.into(),
            cause,
            data,
        }
    }
}

/// An event as the journal holds it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Recorded {
    pub seq: i64,
    pub event: Envelope,
    pub handled: Option<String>,
    pub error: Option<String>,
}

/// A subject: `primum.<project>.ticket.<id>`, `.project` or `.worker`.
pub fn subject(project: &str, target: &str) -> String {
    match target {
        "project" | "worker" => format!("primum.{project}.{target}"),
        id => format!("primum.{project}.ticket.{id}"),
    }
}

/// The ticket a subject names.
pub fn ticket_of(subject: &str) -> Option<&str> {
    subject.split_once(".ticket.").map(|(_, id)| id)
}

/// What one answer changes, saved in one transaction.
pub struct Handled<'a> {
    /// The event answered.
    pub seq: i64,
    pub error: Option<String>,
    /// The ticket's runtime state after it.
    pub ticket: Option<(&'a str, &'a Runtime)>,
    /// The ticket's timers from now on; `None` leaves them.
    pub timers: Option<&'a [Timer]>,
    pub events: Vec<Envelope>,
    /// Ticket changes to write to the ledger.
    pub ledger: Option<(&'a str, Map<String, Value>)>,
}

pub struct Journal {
    db: Connection,
}

fn json<T: Serialize>(v: &T) -> Result<String> {
    Ok(serde_json::to_string(v)?)
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Recorded> {
    let data: String = r.get(7)?;
    Ok(Recorded {
        seq: r.get(0)?,
        event: Envelope {
            id: r.get(1)?,
            time: r.get(2)?,
            kind: r.get(3)?,
            source: r.get(4)?,
            subject: r.get(5)?,
            cause: r.get(6)?,
            data: serde_json::from_str(&data).unwrap_or(Value::Null),
        },
        handled: r.get(8)?,
        error: r.get(9)?,
    })
}

const COLUMNS: &str = "seq, id, time, type, source, subject, cause, data, handled, error";

impl Journal {
    /// The journal's file in a project.
    pub fn path(p: &Project) -> PathBuf {
        p.root.join(".primum").join("journal.sqlite")
    }

    /// Opens (and creates) a project's journal.
    pub fn open(p: &Project) -> Result<Journal> {
        let path = Journal::path(p);
        std::fs::create_dir_all(path.parent().expect("a folder"))?;
        let db =
            Connection::open(&path).with_context(|| format!("cannot open {}", path.display()))?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        db.execute_batch(SCHEMA)?;
        Ok(Journal { db })
    }

    /// Saves an event; an id already saved is kept as it was.
    pub fn publish(&self, e: &Envelope) -> Result<()> {
        publish(&self.db, e)
    }

    /// The oldest event not yet handled, apart from those for workers.
    pub fn next(&self) -> Result<Option<Recorded>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM events WHERE handled IS NULL AND subject NOT LIKE '%.worker' ORDER BY seq LIMIT 1"
        );
        Ok(self.db.query_row(&sql, [], row).optional()?)
    }

    /// Every event, or those about one subject, in order.
    pub fn events(&self, subject: Option<&str>) -> Result<Vec<Recorded>> {
        let sql =
            format!("SELECT {COLUMNS} FROM events WHERE ?1 IS NULL OR subject = ?1 ORDER BY seq");
        let mut s = self.db.prepare(&sql)?;
        let out = s
            .query_map(params![subject], row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// A running ticket's pin and runtime state.
    pub fn ticket(&self, id: &str) -> Result<Option<(String, Runtime)>> {
        let r: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT pin, runtime FROM tickets WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        r.map(|(pin, rt)| Ok((pin, serde_json::from_str(&rt)?)))
            .transpose()
    }

    /// Starts (or restarts) a ticket on a pinned lifecycle; its timers go.
    pub fn start_ticket(&self, id: &str, pin: &str, rt: &Runtime) -> Result<()> {
        self.db
            .execute("DELETE FROM timers WHERE ticket = ?1", params![id])?;
        self.db.execute(
            "INSERT OR REPLACE INTO tickets (id, pin, runtime) VALUES (?1, ?2, ?3)",
            params![id, pin, json(rt)?],
        )?;
        Ok(())
    }

    /// Forgets a ticket that left its lifecycle, and its timers.
    pub fn forget_ticket(&self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM timers WHERE ticket = ?1", params![id])?;
        self.db
            .execute("DELETE FROM tickets WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// The definition of a pinned lifecycle version.
    pub fn lifecycle(&self, pin: &str) -> Result<Option<File>> {
        let t: Option<String> = self
            .db
            .query_row(
                "SELECT definition FROM lifecycles WHERE pin = ?1",
                params![pin],
                |r| r.get(0),
            )
            .optional()?;
        t.map(|t| Ok(serde_json::from_str(&t)?)).transpose()
    }

    /// Keeps a lifecycle version's definition, so tickets finish on it.
    pub fn pin_lifecycle(&self, pin: &str, f: &File) -> Result<()> {
        self.db.execute(
            "INSERT OR IGNORE INTO lifecycles (pin, definition) VALUES (?1, ?2)",
            params![pin, json(f)?],
        )?;
        Ok(())
    }

    /// Timers due at `now` (RFC 3339, UTC): id, ticket, data.
    pub fn due_timers(&self, now: &str) -> Result<Vec<(i64, String, Value)>> {
        let mut s = self
            .db
            .prepare("SELECT id, ticket, data FROM timers WHERE at <= ?1 ORDER BY at, id")?;
        let out = s
            .query_map(params![now], |r| {
                let d: String = r.get(2)?;
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    serde_json::from_str(&d).unwrap_or(Value::Null),
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(out)
    }

    /// Turns a due timer into its event, in one transaction.
    pub fn fire_timer(&mut self, id: i64, e: &Envelope) -> Result<()> {
        let tx = self.db.transaction()?;
        publish(&tx, e)?;
        tx.execute("DELETE FROM timers WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    /// The last snapshot of every item the ledger feed has seen.
    pub fn seen_all(&self) -> Result<Map<String, Value>> {
        let mut s = self.db.prepare("SELECT id, snapshot FROM seen")?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Map::new();
        for r in rows {
            let (id, snap) = r?;
            out.insert(id, serde_json::from_str(&snap)?);
        }
        Ok(out)
    }

    pub fn set_seen(&self, id: &str, snapshot: &Value) -> Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO seen (id, snapshot) VALUES (?1, ?2)",
            params![id, json(snapshot)?],
        )?;
        Ok(())
    }

    pub fn forget_seen(&self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM seen WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Queues ticket changes for the ledger.
    pub fn queue_ledger(&self, ticket: &str, changes: &Map<String, Value>) -> Result<()> {
        self.db.execute(
            "INSERT INTO ledger (ticket, changes) VALUES (?1, ?2)",
            params![ticket, json(changes)?],
        )?;
        Ok(())
    }

    /// Ticket changes not yet written to the ledger, in order.
    pub fn pending_ledger(&self) -> Result<Vec<(i64, String, Map<String, Value>)>> {
        let mut s = self
            .db
            .prepare("SELECT id, ticket, changes FROM ledger WHERE written IS NULL ORDER BY id")?;
        let rows = s.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = vec![];
        for r in rows {
            let (id, ticket, changes) = r?;
            out.push((id, ticket, serde_json::from_str(&changes)?));
        }
        Ok(out)
    }

    pub fn ledger_written(&self, id: i64) -> Result<()> {
        self.db.execute(
            "UPDATE ledger SET written = ?2 WHERE id = ?1",
            params![
                id,
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            ],
        )?;
        Ok(())
    }

    /// Saves an answer: all of it, or (on any error) none of it.
    pub fn handle(&mut self, h: Handled) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let tx = self.db.transaction()?;
        tx.execute(
            "UPDATE events SET handled = ?2, error = ?3 WHERE seq = ?1",
            params![h.seq, now, h.error],
        )?;
        if let Some((id, rt)) = h.ticket {
            tx.execute(
                "UPDATE tickets SET runtime = ?2 WHERE id = ?1",
                params![id, json(rt)?],
            )?;
            if let Some(timers) = h.timers {
                tx.execute("DELETE FROM timers WHERE ticket = ?1", params![id])?;
                for t in timers {
                    tx.execute(
                        "INSERT INTO timers (ticket, at, data) VALUES (?1, ?2, ?3)",
                        params![id, t.at, json(&t.data)?],
                    )?;
                }
            }
        }
        for e in &h.events {
            publish(&tx, e)?;
        }
        if let Some((ticket, changes)) = &h.ledger {
            tx.execute(
                "INSERT INTO ledger (ticket, changes) VALUES (?1, ?2)",
                params![ticket, json(changes)?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn publish(db: &Connection, e: &Envelope) -> Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO events (id, time, type, source, subject, cause, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![e.id, e.time, e.kind, e.source, e.subject, e.cause, json(&e.data)?],
    )?;
    Ok(())
}
