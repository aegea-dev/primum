mod common;
use primum::engine::journal::{self, Envelope, Handled, Journal};
use primum::engine::{Runtime, Timer};
use serde_json::{Map, json};

fn ev(kind: &str, subject: &str) -> Envelope {
    Envelope::new(
        "test",
        kind,
        subject,
        "2026-10-14T20:00:00Z",
        None,
        json!({"söz": "ışık"}),
    )
}

#[test]
fn events_queue_in_order_and_skip_workers() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let j = Journal::open(&p).unwrap();
    assert!(d.path().join(".primum/journal.sqlite").is_file());
    let a = ev("ticket.changed", "primum.deneme.ticket.1");
    j.publish(&a).unwrap();
    j.publish(&a).unwrap(); // the same id twice: kept once
    j.publish(&ev("action.requested", "primum.deneme.worker"))
        .unwrap();
    j.publish(&ev("input.received", "primum.deneme.ticket.2"))
        .unwrap();
    assert_eq!(j.events(None).unwrap().len(), 3);
    let first = j.next().unwrap().unwrap();
    assert_eq!(first.event, a);
    assert_eq!(first.event.data["söz"], "ışık");
    assert_eq!(a.id.len(), 36, "a UUID");
    assert_eq!(journal::subject("deneme", "12"), "primum.deneme.ticket.12");
    assert_eq!(
        journal::subject("deneme", "project"),
        "primum.deneme.project"
    );
    assert_eq!(journal::subject("deneme", "worker"), "primum.deneme.worker");
    assert_eq!(journal::ticket_of("primum.deneme.ticket.12"), Some("12"));
    assert_eq!(journal::ticket_of("primum.deneme.worker"), None);
}

#[test]
fn an_answer_is_saved_in_one_transaction() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let mut j = Journal::open(&p).unwrap();
    let a = ev("lifecycle.attached", "primum.deneme.ticket.1");
    j.publish(&a).unwrap();
    let rt = Runtime {
        state: "yeni".into(),
        chain: None,
        failed: None,
    };
    j.start_ticket("1", "akış@abc", &rt).unwrap();
    let seq = j.next().unwrap().unwrap().seq;
    let after = Runtime {
        state: "hazır".into(),
        chain: None,
        failed: None,
    };
    let timers = vec![Timer {
        at: "2026-10-14T21:00:00Z".into(),
        data: json!({"state": "hazır", "transition": 1}),
    }];
    let mut changes = Map::new();
    changes.insert("note".into(), json!("Hazırlandı."));
    j.handle(Handled {
        seq,
        error: None,
        ticket: Some(("1", &after)),
        timers: Some(&timers),
        events: vec![ev("input.received", "primum.deneme.ticket.2")],
        ledger: Some(("1", changes.clone())),
    })
    .unwrap();
    assert_eq!(
        j.ticket("1").unwrap().unwrap(),
        ("akış@abc".to_string(), after)
    );
    assert_eq!(j.next().unwrap().unwrap().event.kind, "input.received");
    assert!(j.due_timers("2026-10-14T20:30:00Z").unwrap().is_empty());
    let due = j.due_timers("2026-10-14T21:00:00Z").unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].1, "1");
    let pending = j.pending_ledger().unwrap();
    assert_eq!((pending[0].1.as_str(), &pending[0].2), ("1", &changes));
    j.ledger_written(pending[0].0).unwrap();
    assert!(j.pending_ledger().unwrap().is_empty());
    let handled = j.events(Some("primum.deneme.ticket.1")).unwrap();
    assert!(handled[0].handled.is_some());
    j.fire_timer(due[0].0, &ev("timer.fired", "primum.deneme.ticket.1"))
        .unwrap();
    assert!(j.due_timers("2026-10-15T00:00:00Z").unwrap().is_empty());
    j.forget_ticket("1").unwrap();
    assert!(j.ticket("1").unwrap().is_none());
}

#[test]
fn pinned_lifecycles_and_snapshots_are_kept() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let j = Journal::open(&p).unwrap();
    let f = primum::lifecycle::manual();
    j.pin_lifecycle("manual@1", &f).unwrap();
    assert_eq!(j.lifecycle("manual@1").unwrap(), Some(f));
    assert_eq!(j.lifecycle("yok@1").unwrap(), None);
    j.set_seen("1", &json!({"title": "Çay"})).unwrap();
    assert_eq!(j.seen_all().unwrap()["1"], json!({"title": "Çay"}));
    j.forget_seen("1").unwrap();
    assert!(j.seen_all().unwrap().is_empty());
}

#[test]
fn a_new_journal_carries_its_schema_version() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let j = Journal::open(&p).unwrap();
    assert_eq!(j.schema().unwrap(), journal::SCHEMA_VERSION);
    drop(j);
    // opening again changes nothing
    assert_eq!(
        Journal::open(&p).unwrap().schema().unwrap(),
        journal::SCHEMA_VERSION
    );
}

#[test]
fn a_journal_from_a_newer_primum_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    drop(Journal::open(&p).unwrap());
    let db = rusqlite::Connection::open(Journal::path(&p)).unwrap();
    db.execute("UPDATE meta SET value = '99' WHERE key = 'schema'", [])
        .unwrap();
    drop(db);
    let e = Journal::open(&p).err().unwrap().to_string();
    assert!(e.contains("newer primum") && e.contains("99"), "{e}");
}

#[test]
fn a_journal_from_before_versions_is_brought_up_to_date() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    std::fs::create_dir_all(d.path().join(".primum")).unwrap();
    // the tables as the first journals made them, with a change waiting
    let db = rusqlite::Connection::open(Journal::path(&p)).unwrap();
    db.execute_batch(
        "CREATE TABLE events (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, time TEXT NOT NULL, type TEXT NOT NULL, source TEXT NOT NULL, subject TEXT NOT NULL, cause TEXT, data TEXT NOT NULL, handled TEXT, error TEXT);
         CREATE TABLE ledger (id INTEGER PRIMARY KEY AUTOINCREMENT, ticket TEXT NOT NULL, changes TEXT NOT NULL, written TEXT);
         INSERT INTO ledger (ticket, changes) VALUES ('1', '{\"note\": \"Bekleyen değişiklik.\"}');",
    )
    .unwrap();
    drop(db);
    let j = Journal::open(&p).unwrap();
    assert_eq!(j.schema().unwrap(), journal::SCHEMA_VERSION);
    let pending = j.pending_ledger().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].2["note"], "Bekleyen değişiklik.");
}
