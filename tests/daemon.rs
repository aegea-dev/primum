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

/// Runs `primum daemon --once` in a project folder with `env` set, and the
/// OS secret store left alone.
fn daemon_with(d: &Path, env: &[(&str, &str)]) -> std::process::Output {
    let mut c = std::process::Command::new(manager());
    c.args(["daemon", "--once", "--now", "2026-10-14T20:00:00Z"])
        .current_dir(d)
        .env("PRIMUM_NO_SECRET_STORE", "1")
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
    executable_with(d, script, &format!(r#"{{ "timeout": "{timeout}" }}"#));
}

/// The `araç` actionable running `script`, with its `limits`, and the
/// `denetim` lifecycle calling it.
#[cfg(unix)]
fn executable_with(d: &Path, script: &str, limits: &str) {
    use std::os::unix::fs::PermissionsExt;
    let a = d.join("actionables/araç");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::write(a.join("run.sh"), script).unwrap();
    std::fs::set_permissions(a.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        a.join("actionable.json"),
        format!(
            r#"{{ "name": "araç", "version": 1, "type": "executable", "limits": {limits},
                     "targets": {{ "run": {{ "executable": "./run.sh" }} }} }}"#
        ),
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

/// How many times the `araç` actionable has run: its script adds a line to
/// `runs.txt` each time.
fn runs(d: &Path) -> usize {
    std::fs::read_to_string(d.join("actionables/araç/runs.txt"))
        .unwrap_or_default()
        .lines()
        .count()
}

/// A ticket on the `denetim` lifecycle.
fn on_denetim(p: &primum::store::Project) -> String {
    let id = insert(p, "TODO", "Işık", Changes::default()).unwrap();
    attach(p, &id, "denetim", None).unwrap();
    id
}

#[cfg(unix)]
#[test]
fn a_job_that_renews_its_lease_runs_past_it() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    executable_with(
        d.path(),
        "#!/bin/sh\ncat > /dev/null\nfor i in 1 2 3 4; do touch \"$PRIMUM_HEARTBEAT\"; sleep 1; done\necho '{\"ok\": true}'\n",
        r#"{ "timeout": "30s", "heartbeat": "2s" }"#,
    );
    let id = on_denetim(&p);
    let pass = run_once(&p, now(), manager()).unwrap();
    assert_eq!((pass.worked, pass.lost), (1, 0));
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "bitti");
}

#[cfg(unix)]
#[test]
fn a_job_whose_lease_runs_out_is_lost_and_runs_again() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    // the first run hangs without a heartbeat; the second answers
    executable_with(
        d.path(),
        "#!/bin/sh\ncat > /dev/null\necho run >> runs.txt\n[ \"$(wc -l < runs.txt)\" -gt 1 ] || sleep 30\necho '{\"ok\": true}'\n",
        r#"{ "timeout": "30s", "heartbeat": "1s" }"#,
    );
    let id = on_denetim(&p);
    let started = std::time::Instant::now();
    let pass = run_once(&p, now(), manager()).unwrap();
    assert!(started.elapsed().as_secs() < 15, "{:?}", started.elapsed());
    assert_eq!((pass.worked, pass.lost), (1, 1));
    assert_eq!(runs(d.path()), 2);
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "bitti");
}

#[cfg(unix)]
#[test]
fn a_worker_killed_from_outside_runs_again() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    // the first run kills its worker (the script's parent)
    executable(
        d.path(),
        "#!/bin/sh\ncat > /dev/null\necho run >> runs.txt\n[ \"$(wc -l < runs.txt)\" -gt 1 ] || kill -9 $PPID\necho '{\"ok\": true}'\n",
        "30s",
    );
    let id = on_denetim(&p);
    let pass = run_once(&p, now(), manager()).unwrap();
    assert_eq!((pass.worked, pass.lost), (1, 1));
    assert_eq!(runs(d.path()), 2);
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "bitti");
    // the idempotency key is the same on the second run
    let keys: Vec<String> = Journal::open(&p)
        .unwrap()
        .events(None)
        .unwrap()
        .into_iter()
        .filter(|r| r.event.kind == "action.requested")
        .map(|r| r.event.data["key"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(keys.len(), 1, "one job, run twice");
}

#[cfg(unix)]
#[test]
fn a_job_lost_three_times_fails() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    executable(
        d.path(),
        "#!/bin/sh\ncat > /dev/null\necho run >> runs.txt\nkill -9 $PPID\n",
        "30s",
    );
    let id = on_denetim(&p);
    let pass = run_once(&p, now(), manager()).unwrap();
    assert_eq!((pass.worked, pass.lost), (1, 2));
    assert_eq!(runs(d.path()), 3);
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "failed");
    assert!(it.body.contains("lost 3 times"), "{}", it.body);
}

/// Whether a process is still running (a zombie waiting to be reaped is
/// not).
#[cfg(unix)]
fn running(pid: i32) -> bool {
    let o = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let stat = String::from_utf8_lossy(&o.stdout);
    !stat.trim().is_empty() && !stat.trim().starts_with('Z')
}

#[cfg(unix)]
#[test]
fn a_job_runs_again_after_its_daemon_is_killed_and_its_worker_stops() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    std::fs::write(d.path().join("project.json"), "{\"name\": \"deneme\"}\n").unwrap();
    // the first run says where it is, then hangs
    executable(
        d.path(),
        "#!/bin/sh\ncat > /dev/null\necho run >> runs.txt\n[ \"$(wc -l < runs.txt)\" -gt 1 ] || { echo $$ > pid.txt; sleep 30; }\necho '{\"ok\": true}'\n",
        "60s",
    );
    let id = on_denetim(&p);
    let mut daemon = std::process::Command::new(manager())
        .args(["daemon", "--once", "--now", "2026-10-14T20:00:00Z"])
        .current_dir(d.path())
        .env("PRIMUM_NO_SECRET_STORE", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid_file = d.path().join("actionables/araç/pid.txt");
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let pid: i32 = loop {
        if let Some(pid) = std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            break pid;
        }
        assert!(std::time::Instant::now() < until, "the job never started");
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    // the worker sees its daemon gone and stops its job
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while running(pid) {
        assert!(
            std::time::Instant::now() < until,
            "the job outlived its daemon"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    // a new daemon runs the job again, once
    let o = daemon_with(d.path(), &[]);
    assert!(o.status.success(), "{o:?}");
    assert_eq!(runs(d.path()), 2);
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "bitti");
}

/// A stand-in for `primum manager` that changes the ticket's file every
/// time it is asked, then answers as the real one does.
#[cfg(unix)]
fn restless_manager(d: &Path, id: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let item = d.join("items").join(format!("{id}.md"));
    let script = d.join("huzursuz.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf 'bir satır daha\\n' >> '{}'\nexec '{}' \"$@\"\n",
            item.display(),
            manager().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[cfg(unix)]
#[test]
fn an_answer_the_ticket_keeps_outdating_waits_instead_of_being_saved() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    let pass = run_once(&p, now(), &restless_manager(d.path(), &id)).unwrap();
    assert_eq!((pass.delivered, pass.deferred), (0, 1));
    let j = Journal::open(&p).unwrap();
    let events = j.events(Some(&journal::subject("deneme", &id))).unwrap();
    assert_eq!(events[0].event.kind, "lifecycle.attached");
    assert!(events[0].handled.is_none(), "left for the next pass");
    assert_eq!(
        j.ticket(&id).unwrap().unwrap().1.state,
        "yeni",
        "no stale answer saved"
    );
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "yeni");
    // once the ticket settles, the next pass delivers it
    let pass = run_once(&p, now(), manager()).unwrap();
    assert_eq!(pass.deferred, 0);
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "hazir");
}

#[test]
fn a_change_that_cannot_be_written_is_retried_then_given_up_with_a_note() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    run_once(&p, now(), manager()).unwrap();
    let j = Journal::open(&p).unwrap();
    let change = |k: &str, v: &str| {
        let mut m = serde_json::Map::new();
        m.insert(k.into(), json!(v));
        m
    };
    // a status the ticket's lifecycle does not have, then a good change
    j.queue_ledger(&id, &change("status", "yok-boyle")).unwrap();
    j.queue_ledger(&id, &change("note", "Sonraki değişiklik."))
        .unwrap();
    let attempts = || -> Vec<(i64, Option<String>)> {
        let db = rusqlite::Connection::open(Journal::path(&p)).unwrap();
        let mut s = db
            .prepare("SELECT attempts, error FROM ledger WHERE changes LIKE '%yok-boyle%'")
            .unwrap();
        s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let pass = run_once(&p, now(), manager()).unwrap();
    assert_eq!(
        pass.written, 0,
        "the bad change fails; the next one waits behind it"
    );
    assert_eq!(attempts()[0].0, 1, "tried once this pass");
    assert!(attempts()[0].1.as_deref().unwrap().contains("yok-boyle"));
    assert!(
        !load(&p)
            .unwrap()
            .get(&id)
            .unwrap()
            .body
            .contains("Sonraki değişiklik.")
    );
    run_once(&p, now(), manager()).unwrap();
    assert_eq!(attempts()[0].0, 2);
    run_once(&p, now(), manager()).unwrap();
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert!(it.body.contains("could not be written"), "{}", it.body);
    assert_eq!(it.status, "hazir", "the bad status never landed");
    run_once(&p, now(), manager()).unwrap();
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert!(it.body.contains("Sonraki değişiklik."), "{}", it.body);
    assert!(
        Journal::open(&p)
            .unwrap()
            .pending_ledger()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_file_dropped_in_the_inbox_is_an_input_for_its_ticket() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    run_once(&p, now(), manager()).unwrap();
    let inbox = d.path().join("inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    let name = format!("{id}.yanıt.txt");
    std::fs::write(inbox.join(&name), "Evet, onaylıyorum.\n").unwrap();
    std::fs::write(inbox.join(".yarım"), "henüz yazılıyor").unwrap();
    run_once(&p, now(), manager()).unwrap();
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "bitti");
    assert!(
        inbox.join("done").join(&name).is_file(),
        "moved when handled"
    );
    assert!(!inbox.join(&name).exists());
    assert!(inbox.join(".yarım").is_file(), "dot-files are left alone");
    let j = Journal::open(&p).unwrap();
    let inputs: Vec<_> = j
        .events(Some(&journal::subject("deneme", &id)))
        .unwrap()
        .into_iter()
        .filter(|r| r.event.kind == "input.received")
        .collect();
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].event.source, "inbox");
    assert_eq!(inputs[0].event.data["name"], name.as_str());
    assert_eq!(
        inputs[0].event.data["file"],
        format!("inbox/done/{name}").as_str()
    );
    // nothing new on the next pass, nor when the same file is found again
    // (as after a stop between saving its event and moving it)
    run_once(&p, now(), manager()).unwrap();
    std::fs::rename(inbox.join("done").join(&name), inbox.join(&name)).unwrap();
    run_once(&p, now(), manager()).unwrap();
    assert_eq!(
        j.events(None)
            .unwrap()
            .iter()
            .filter(|r| r.event.kind == "input.received")
            .count(),
        1
    );
}

#[test]
fn an_inbox_file_for_no_ticket_goes_to_the_project_and_names_never_clash() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    let inbox = d.path().join("inbox");
    std::fs::create_dir_all(inbox.join("done")).unwrap();
    std::fs::write(inbox.join("done/rapor.csv"), "eski\n").unwrap();
    std::fs::write(inbox.join("rapor.csv"), "yeni\n").unwrap();
    run_once(&p, now(), manager()).unwrap();
    let j = Journal::open(&p).unwrap();
    let input = j
        .events(Some(&journal::subject("deneme", "project")))
        .unwrap()
        .into_iter()
        .find(|r| r.event.kind == "input.received")
        .unwrap();
    assert!(
        input
            .error
            .as_deref()
            .unwrap()
            .contains("project lifecycles"),
        "{input:?}"
    );
    assert_eq!(
        common::read(d.path(), "inbox/done/rapor.csv"),
        "eski\n",
        "not overwritten"
    );
    let moved = input.event.data["file"].as_str().unwrap().to_string();
    assert_ne!(moved, "inbox/done/rapor.csv");
    assert_eq!(common::read(d.path(), &moved), "yeni\n");
}

#[test]
fn the_loop_records_its_passes_and_stops_when_nothing_can_wake_it() {
    use primum::engine::daemon::{Health, run_loop};
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    let health = std::sync::Mutex::new(Health::starting("2026-10-15T09:00:00Z"));
    let (wake, woken) = std::sync::mpsc::channel::<()>();
    drop(wake);
    let reported = std::cell::Cell::new(0);
    run_loop(
        std::slice::from_ref(&p),
        std::time::Duration::from_secs(60),
        manager(),
        &woken,
        &health,
        |_, r| {
            assert!(r.is_ok());
            reported.set(reported.get() + 1);
        },
    );
    let h = health.into_inner().unwrap();
    assert_eq!((h.passes, reported.get()), (1, 1));
    assert!(h.last_pass.is_some() && h.last_error.is_none());
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "hazir");
}

#[cfg(unix)]
#[test]
fn jobs_for_different_tickets_run_at_the_same_time() {
    let d = tempfile::tempdir().unwrap();
    let (p, _) = attached(d.path());
    executable(
        d.path(),
        "#!/bin/sh\ncat > /dev/null\nsleep 2\necho '{\"ok\": true}'\n",
        "20s",
    );
    let ids: Vec<String> = ["Işık", "Gölge", "Ağaç"]
        .iter()
        .map(|t| {
            let id = insert(&p, "TODO", t, Changes::default()).unwrap();
            attach(&p, &id, "denetim", None).unwrap();
            id
        })
        .collect();
    let started = std::time::Instant::now();
    let pass = run_once(&p, now(), manager()).unwrap();
    assert_eq!(pass.worked, 3);
    assert!(
        started.elapsed().as_secs() < 5,
        "three 2s jobs side by side, not one after another: {:?}",
        started.elapsed()
    );
    let board = load(&p).unwrap();
    for id in &ids {
        assert_eq!(board.get(id).unwrap().status, "bitti");
    }
}

/// Sets `runner` in the project's `project.json`, as a synced machine would.
fn set_runner(d: &Path, host: &str) {
    let v = json!({"name": "deneme", "runner": host});
    std::fs::write(d.join("project.json"), v.to_string()).unwrap();
}

fn runner_of(d: &Path) -> Option<String> {
    let text = std::fs::read_to_string(d.join("project.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["runner"].as_str().map(str::to_string)
}

#[test]
fn the_first_daemon_to_run_a_project_becomes_its_runner() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    std::fs::write(d.path().join("project.json"), r#"{"name": "deneme"}"#).unwrap();
    assert_eq!(runner_of(d.path()), None);
    let out = daemon_with(d.path(), &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let host = runner_of(d.path()).expect("a runner");
    assert!(!host.is_empty());
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "hazir");
    // the claim is primum's own commit, not an edit made outside it
    let log = primum::history::History::open(&p)
        .unwrap()
        .unwrap()
        .git(&["log", "--format=%s"])
        .unwrap();
    assert!(log.lines().any(|m| m == format!("runner: {host}")), "{log}");
}

#[test]
fn a_project_another_machine_runs_is_skipped_unless_taken_over() {
    let d = tempfile::tempdir().unwrap();
    let (p, id) = attached(d.path());
    set_runner(d.path(), "başka-makine");
    let out = daemon_with(d.path(), &[]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "nothing left to run");
    assert!(err.contains("runs on başka-makine"), "{err}");
    assert!(err.contains("--take-over"), "{err}");
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "yeni");

    let out = std::process::Command::new(manager())
        .args([
            "daemon",
            "--once",
            "--take-over",
            "--now",
            "2026-10-14T20:00:00Z",
        ])
        .current_dir(d.path())
        .env("PRIMUM_NO_SECRET_STORE", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let host = runner_of(d.path()).expect("a runner");
    assert_ne!(host, "başka-makine");
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "hazir");
}
