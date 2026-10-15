mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use primum::engine::daemon::Health;
use primum::engine::http::{Engine, router};
use primum::engine::journal::{self, Journal};
use primum::projects::Entry;
use primum::store::{Changes, Project, attach, insert};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, mpsc};
use tower::ServiceExt;

const LIFECYCLE: &str = r#"{ "name": "akış", "states": ["yeni", "bitti"], "start": "yeni", "end": ["bitti"],
  "transitions": [ { "from": "yeni", "on": "input.received", "to": "bitti" } ] }"#;

/// A project `deneme` with ticket 1 on a lifecycle and ticket 2 on manual,
/// and the daemon's router over it.
fn setup(d: &std::path::Path) -> (Project, axum::Router, mpsc::Receiver<()>) {
    let p = common::project(d);
    std::fs::create_dir_all(d.join("lifecycles")).unwrap();
    std::fs::write(d.join("lifecycles/akış.json"), LIFECYCLE).unwrap();
    insert(&p, "TODO", "Gölge", Changes::default()).unwrap();
    insert(&p, "TODO", "Işık", Changes::default()).unwrap();
    attach(&p, "1", "akış", None).unwrap();
    let (wake, woken) = mpsc::channel();
    let engine = Arc::new(Engine {
        health: Mutex::new(Health::starting("2026-10-15T09:00:00Z")),
        wake,
    });
    let entries = vec![Entry {
        path: p.root.clone(),
        project: Ok(p.clone()),
    }];
    (p, router(entries, engine), woken)
}

async fn call(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn post(app: &axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::post(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    call(app, req).await
}

#[tokio::test]
async fn health_says_the_daemon_is_up_and_how_far_it_got() {
    let d = tempfile::tempdir().unwrap();
    let (_, app, _) = setup(d.path());
    let (status, v) = call(&app, Request::get("/health").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["ok"], true);
    assert_eq!(v["projects"], json!(["deneme"]));
    assert_eq!(v["started"], "2026-10-15T09:00:00Z");
    assert_eq!(v["passes"], 0);
    assert_eq!(v["last_pass"], Value::Null);
}

#[tokio::test]
async fn an_outside_event_is_saved_and_wakes_the_daemon() {
    let d = tempfile::tempdir().unwrap();
    let (p, app, woken) = setup(d.path());
    let body = json!({"type": "input.received", "ticket": "#1", "data": {"yanıt": "evet"}});
    let (status, v) = post(&app, "/p/deneme/api/events", body).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    let events = Journal::open(&p)
        .unwrap()
        .events(Some(&journal::subject("deneme", "1")))
        .unwrap();
    let e = events.iter().find(|r| r.event.id == id).unwrap();
    assert_eq!(
        (e.event.kind.as_str(), e.event.source.as_str()),
        ("input.received", "http")
    );
    assert_eq!(e.event.data, json!({"yanıt": "evet"}));
    assert!(woken.try_recv().is_ok(), "the loop is woken");
    // without a ticket, it is the project's
    let (status, _) = post(
        &app,
        "/p/deneme/api/events",
        json!({"type": "kampanya.başladı"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "type words are lowercase ASCII"
    );
    let (status, _) = post(
        &app,
        "/p/deneme/api/events",
        json!({"type": "campaign.started"}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let project = Journal::open(&p)
        .unwrap()
        .events(Some(&journal::subject("deneme", "project")))
        .unwrap();
    assert_eq!(project[0].event.kind, "campaign.started");
}

#[tokio::test]
async fn events_the_engine_makes_itself_cannot_come_from_outside() {
    let d = tempfile::tempdir().unwrap();
    let (p, app, _) = setup(d.path());
    for kind in [
        "action.finished",
        "action.failed",
        "timer.fired",
        "ticket.changed",
        "lifecycle.attached",
    ] {
        let (status, v) = post(
            &app,
            "/p/deneme/api/events",
            json!({"type": kind, "ticket": "1"}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{kind}");
        assert!(
            v["error"].as_str().unwrap().contains("the engine's own"),
            "{v}"
        );
    }
    let (status, _) = post(
        &app,
        "/p/deneme/api/events",
        json!({"type": "lifecycle.resume", "ticket": "1"}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "a person's resume is allowed");
    let saved: Vec<String> = Journal::open(&p)
        .unwrap()
        .events(None)
        .unwrap()
        .into_iter()
        .map(|r| r.event.kind)
        .collect();
    assert!(
        !saved
            .iter()
            .any(|k| k.starts_with("action.") || k == "timer.fired"),
        "{saved:?}"
    );
}

#[tokio::test]
async fn bad_events_say_what_is_wrong() {
    let d = tempfile::tempdir().unwrap();
    let (_, app, _) = setup(d.path());
    let cases = [
        (
            "/p/deneme/api/events",
            json!({"type": "Input Received"}),
            StatusCode::BAD_REQUEST,
            "an event type",
        ),
        (
            "/p/deneme/api/events",
            json!({"type": "input.received", "ticket": "2"}),
            StatusCode::CONFLICT,
            "not on a lifecycle",
        ),
        (
            "/p/deneme/api/events",
            json!({"type": "input.received", "ticket": "99"}),
            StatusCode::NOT_FOUND,
            "no item 99",
        ),
        (
            "/p/deneme/api/events",
            json!({"type": "input.received", "ticket": "on iki"}),
            StatusCode::BAD_REQUEST,
            "not an item id",
        ),
        (
            "/p/deneme/api/events",
            json!({"type": "input.received", "data": [1, 2]}),
            StatusCode::BAD_REQUEST,
            "data must be a JSON object",
        ),
        (
            "/p/deneme/api/events",
            json!({"type": "input.received", "renk": 1}),
            StatusCode::BAD_REQUEST,
            "renk",
        ),
        (
            "/p/yok/api/events",
            json!({"type": "input.received"}),
            StatusCode::NOT_FOUND,
            "no project yok",
        ),
    ];
    for (uri, body, want, says) in cases {
        let (status, v) = post(&app, uri, body.clone()).await;
        assert_eq!(status, want, "{body}: {v}");
        assert!(
            v["error"].as_str().unwrap_or("").contains(says),
            "{body}: {v}"
        );
    }
}

#[tokio::test]
async fn the_board_is_still_served() {
    let d = tempfile::tempdir().unwrap();
    let (_, app, _) = setup(d.path());
    let (status, v) = call(
        &app,
        Request::get("/p/deneme/api/items")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 2);
}

/// A running `primum daemon`, killed when dropped (a failing test too).
struct Daemon(std::process::Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Polls `f` every 100 ms for up to 15 seconds.
fn eventually<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(
            std::time::Instant::now() < until,
            "timed out waiting for {what}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[test]
fn a_running_daemon_serves_health_and_acts_on_an_event_at_once() {
    let d = tempfile::tempdir().unwrap();
    let (p, _, _) = setup(d.path());
    std::fs::write(d.path().join("project.json"), "{\"name\": \"deneme\"}\n").unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    // a long interval: only the wake-up can make the ticket move quickly
    let _daemon = Daemon(
        std::process::Command::new(env!("CARGO_BIN_EXE_primum"))
            .args(["daemon", "--port", &port.to_string(), "--interval", "300"])
            .current_dir(d.path())
            .env("PRIMUM_NO_SECRET_STORE", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::new();
    let health: Value = eventually("/health", || {
        let r = client.get(format!("{base}/health")).send().ok()?;
        let v: Value = r.json().ok()?;
        (v["passes"].as_u64()? >= 1).then_some(v)
    });
    assert_eq!(health["projects"], json!(["deneme"]));
    let started = std::time::Instant::now();
    let r = client
        .post(format!("{base}/p/deneme/api/events"))
        .json(&json!({"type": "input.received", "ticket": "1", "data": {"söz": "tamam"}}))
        .send()
        .unwrap();
    assert_eq!(r.status().as_u16(), 202);
    eventually("the ticket to move", || {
        let board = primum::store::load(&p).ok()?;
        (board.get("1")?.status == "bitti").then_some(())
    });
    assert!(started.elapsed().as_secs() < 15, "woken, not waiting 300s");
}
