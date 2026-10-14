mod common;
use chrono::{DateTime, Duration, Utc};
use primum::engine::daemon::run_once;
use primum::engine::journal::{self, Envelope, Journal};
use primum::store::{self, Changes, attach, insert, load};
use serde_json::json;
use std::path::Path;

const LIFECYCLE: &str = r#"{
  "name": "akış",
  "states": ["yeni", "hazir", "bitti", "zaman-asimi"],
  "start": "yeni",
  "end": ["bitti", "zaman-asimi"],
  "transitions": [
    { "from": "yeni", "on": "lifecycle.attached", "to": "hazir",
      "do": [ { "action": "ticket.update@1",
                "with": { "title": "${ticket.title} (hazır)", "note": "Hazırlandı." } } ] },
    { "from": "hazir", "on": "input.received", "to": "bitti",
      "timeout": "1h", "on_timeout": "zaman-asimi" }
  ]
}"#;

fn manager() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_primum"))
}

fn now() -> DateTime<Utc> {
    "2026-10-14T20:00:00Z".parse().unwrap()
}

/// A project with the lifecycle and one ticket attached to it.
fn attached(d: &Path) -> (primum::store::Project, String) {
    let p = common::project(d);
    std::fs::create_dir_all(d.join("lifecycles")).unwrap();
    std::fs::write(d.join("lifecycles/akış.json"), LIFECYCLE).unwrap();
    let id = insert(&p, "TODO", "Gölge", Changes::default()).unwrap();
    attach(&p, &id, "akış", None).unwrap();
    (p, id)
}

#[test]
fn an_attached_ticket_moves_by_itself() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    let pass = run_once(&p, now(), manager()).unwrap();
    assert_eq!((pass.delivered, pass.written), (1, 1));
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "hazir");
    assert_eq!(it.title, "Gölge (hazır)");
    assert_eq!(it.body.matches("Hazırlandı.").count(), 1);
    let j = Journal::open(&p).unwrap();
    let events = j.events(Some(&journal::subject("deneme", &id))).unwrap();
    assert_eq!(events[0].event.kind, "lifecycle.attached");
    assert!(events[0].handled.is_some() && events[0].error.is_none());
    assert_eq!(j.ticket(&id).unwrap().unwrap().1.state, "hazir");
    // a second pass sees its own change and does nothing
    let again = run_once(&p, now(), manager()).unwrap();
    assert_eq!((again.delivered, again.written), (0, 0));
    assert_eq!(
        load(&p)
            .unwrap()
            .get(&id)
            .unwrap()
            .body
            .matches("Hazırlandı.")
            .count(),
        1
    );
}

#[test]
fn a_timer_fires_when_it_is_due() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    run_once(&p, now(), manager()).unwrap();
    run_once(&p, now() + Duration::minutes(30), manager()).unwrap();
    assert_eq!(
        load(&p).unwrap().get(&id).unwrap().status,
        "hazir",
        "not yet"
    );
    run_once(&p, now() + Duration::hours(2), manager()).unwrap();
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "zaman-asimi");
}

#[test]
fn an_input_moves_the_ticket_on() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    run_once(&p, now(), manager()).unwrap();
    let j = Journal::open(&p).unwrap();
    let subject = journal::subject("deneme", &id);
    j.publish(&Envelope::new(
        "test",
        "input.received",
        &subject,
        "2026-10-14T20:10:00Z",
        None,
        json!({"yanıt": "evet"}),
    ))
    .unwrap();
    run_once(&p, now() + Duration::minutes(10), manager()).unwrap();
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "bitti");
    assert!(
        j.due_timers("2026-10-15T00:00:00Z").unwrap().is_empty(),
        "its timer went with the wait"
    );
}

#[test]
fn a_person_moving_the_ticket_is_an_event_too() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    run_once(&p, now(), manager()).unwrap();
    let c = Changes {
        status: Some("bitti".into()),
        ..Changes::default()
    };
    store::update(&p, &id, c).unwrap();
    run_once(&p, now(), manager()).unwrap();
    let j = Journal::open(&p).unwrap();
    assert_eq!(j.ticket(&id).unwrap().unwrap().1.state, "bitti");
    let kinds: Vec<String> = j
        .events(Some(&journal::subject("deneme", &id)))
        .unwrap()
        .into_iter()
        .map(|r| r.event.kind)
        .collect();
    assert_eq!(kinds, ["lifecycle.attached", "ticket.changed"]);
}

#[test]
fn changes_left_unwritten_are_written_on_the_next_pass_once() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    run_once(&p, now(), manager()).unwrap();
    let j = Journal::open(&p).unwrap();
    let mut c = serde_json::Map::new();
    c.insert("note".into(), json!("Hazırlandı."));
    j.queue_ledger(&id, &c).unwrap(); // as if the daemon stopped before marking it written
    let pass = run_once(&p, now(), manager()).unwrap();
    assert_eq!(pass.written, 1);
    assert_eq!(
        load(&p)
            .unwrap()
            .get(&id)
            .unwrap()
            .body
            .matches("Hazırlandı.")
            .count(),
        1,
        "not added twice"
    );
}

#[test]
fn a_version_no_longer_in_the_file_cannot_start() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    let changed = LIFECYCLE.replace("\"1h\"", "\"2h\"");
    std::fs::write(d.path().join("lifecycles/akış.json"), changed).unwrap();
    run_once(&p, now(), manager()).unwrap();
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "failed");
    assert!(it.body.contains("attach it again"), "{}", it.body);
}

#[test]
fn manual_tickets_make_no_events() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Çay", Changes::default()).unwrap();
    run_once(&p, now(), manager()).unwrap();
    store::update(
        &p,
        "1",
        Changes {
            status: Some("doing".into()),
            ..Changes::default()
        },
    )
    .unwrap();
    run_once(&p, now(), manager()).unwrap();
    assert!(Journal::open(&p).unwrap().events(None).unwrap().is_empty());
}

fn lifecycle(d: &Path, name: &str, text: &str) {
    std::fs::create_dir_all(d.join("lifecycles")).unwrap();
    std::fs::write(d.join(format!("lifecycles/{name}.json")), text).unwrap();
}

/// Runs `primum daemon --once` in a project folder with `env` set.
fn daemon_with(d: &Path, env: &[(&str, &str)]) -> std::process::Output {
    let mut c = std::process::Command::new(manager());
    c.args(["daemon", "--once", "--now", "2026-10-14T20:00:00Z"])
        .current_dir(d)
        .env_remove("API_KEY");
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

const FETCH: &str = r#"{
  "name": "getir",
  "states": ["yeni", "bitti"],
  "start": "yeni",
  "end": ["bitti"],
  "transitions": [
    { "from": "yeni", "on": "lifecycle.attached", "to": "bitti",
      "do": [
        { "action": "http.request@1",
          "with": { "url": "URL/items/${ticket.id}", "headers": { "Authorization": "Bearer ${env.API_KEY}" } } },
        { "action": "json.pick@1", "with": { "path": "json.data" } },
        { "action": "ticket.update@1", "with": { "title": "${prev.name}" } }
      ] }
  ]
}"#;

#[test]
fn an_http_lifecycle_runs_end_to_end_with_its_secret() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    std::fs::write(d.path().join("project.json"), "{\"name\": \"deneme\"}\n").unwrap();
    let (url, server) = common::http_server();
    lifecycle(d.path(), "getir", &FETCH.replace("URL", &url));
    let id = insert(&p, "TODO", "Gölge", Changes::default()).unwrap();
    attach(&p, &id, "getir", None).unwrap();
    let o = daemon_with(d.path(), &[("API_KEY", "gizli-anahtar")]);
    assert!(o.status.success(), "{o:?}");
    let headers = server.join().unwrap();
    assert_eq!(headers["authorization"], "Bearer gizli-anahtar");
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!((it.status.as_str(), it.title.as_str()), ("bitti", "Ürün"));
    for f in ["journal.sqlite", "journal.sqlite-wal"] {
        let bytes = std::fs::read(d.path().join(".primum").join(f)).unwrap_or_default();
        assert!(
            !String::from_utf8_lossy(&bytes).contains("gizli-anahtar"),
            "the secret is not in {f}"
        );
    }
    let kinds: Vec<String> = Journal::open(&p)
        .unwrap()
        .events(None)
        .unwrap()
        .into_iter()
        .map(|r| r.event.kind)
        .collect();
    assert_eq!(
        kinds,
        ["lifecycle.attached", "action.requested", "action.finished"]
    );
}

#[test]
fn a_missing_secret_fails_the_action() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    std::fs::write(d.path().join("project.json"), "{\"name\": \"deneme\"}\n").unwrap();
    lifecycle(
        d.path(),
        "getir",
        &FETCH.replace("URL", "http://127.0.0.1:9"),
    );
    let id = insert(&p, "TODO", "Gölge", Changes::default()).unwrap();
    attach(&p, &id, "getir", None).unwrap();
    let o = daemon_with(d.path(), &[]);
    assert!(o.status.success(), "{o:?}");
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "failed");
    assert!(it.body.contains("secret API_KEY is not set"), "{}", it.body);
}

#[cfg(unix)]
fn executable(d: &Path, script: &str, timeout: &str) {
    use std::os::unix::fs::PermissionsExt;
    let a = d.join("actionables/araç");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::write(a.join("run.sh"), script).unwrap();
    std::fs::set_permissions(a.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        a.join("actionable.json"),
        format!(r#"{{ "name": "araç", "version": 1, "type": "executable", "limits": {{ "timeout": "{timeout}" }},
                     "targets": {{ "run": {{ "executable": "./run.sh" }} }} }}"#),
    )
    .unwrap();
    lifecycle(
        d,
        "denetim",
        r#"{ "name": "denetim", "states": ["yeni", "bitti"], "start": "yeni", "end": ["bitti"],
             "transitions": [ { "from": "yeni", "on": "lifecycle.attached", "to": "bitti",
               "do": [ { "action": "araç.run@1", "expect": "output.ok" } ] } ] }"#,
    );
}

#[cfg(unix)]
#[test]
fn a_project_executable_runs() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    executable(
        d.path(),
        "#!/bin/sh\ncat > /dev/null\necho '{\"ok\": true}'\n",
        "10s",
    );
    let id = insert(&p, "TODO", "Işık", Changes::default()).unwrap();
    attach(&p, &id, "denetim", None).unwrap();
    let pass = run_once(&p, now(), manager()).unwrap();
    assert_eq!(pass.worked, 1);
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "bitti");
}

#[cfg(unix)]
#[test]
fn a_job_past_its_time_limit_is_killed_and_fails() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    executable(
        d.path(),
        "#!/bin/sh\nsleep 30\necho '{\"ok\": true}'\n",
        "1s",
    );
    let id = insert(&p, "TODO", "Işık", Changes::default()).unwrap();
    attach(&p, &id, "denetim", None).unwrap();
    let started = std::time::Instant::now();
    run_once(&p, now(), manager()).unwrap();
    assert!(started.elapsed().as_secs() < 15, "killed, not waited for");
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "failed");
    assert!(it.body.contains("timed out"), "{}", it.body);
}

#[test]
fn python_actionables_wait_for_a_later_version() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    let a = d.path().join("actionables/checks");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/actionables/checks/actionable.json"),
        a.join("actionable.json"),
    )
    .unwrap();
    lifecycle(
        d.path(),
        "py",
        r#"{ "name": "py", "states": ["yeni", "bitti"], "start": "yeni", "end": ["bitti"],
             "transitions": [ { "from": "yeni", "on": "lifecycle.attached", "to": "bitti",
               "do": [ "checks.is-valid@1" ] } ] }"#,
    );
    let id = insert(&p, "TODO", "Işık", Changes::default()).unwrap();
    attach(&p, &id, "py", None).unwrap();
    run_once(&p, now(), manager()).unwrap();
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "failed");
    assert!(it.body.contains("run in a later version"), "{}", it.body);
}
