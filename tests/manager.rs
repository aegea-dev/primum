use primum::engine::manager::decide;
use primum::engine::{Answer, Event, Input, Runtime};
use primum::lifecycle::File;
use serde_json::{Map, Value, json};
use std::path::Path;

fn example() -> File {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lifecycles/enrich-ticket.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn input(rt: Runtime, kind: &str, id: &str, data: Value) -> Input {
    Input {
        now: "2026-10-14T20:00:00Z".into(),
        lifecycle: example(),
        ticket: json!({"id": "12", "title": "Gölge", "status": rt.state.clone()})
            .as_object()
            .unwrap()
            .clone(),
        args: Map::new(),
        project: Map::new(),
        runtime: rt,
        event: Event {
            id: id.into(),
            kind: kind.into(),
            data,
        },
    }
}

fn at(state: &str) -> Runtime {
    Runtime {
        state: state.into(),
        chain: None,
        failed: None,
    }
}

/// From `create`, through `ticket.created`, to the API request.
fn requested() -> Answer {
    decide(&input(at("create"), "ticket.created", "e1", json!({}))).unwrap()
}

/// The API answered; the chain to `finished` waits on the check.
fn checking() -> Answer {
    let a = requested();
    let data = json!({"run": "e1.2", "index": 0, "output": {"status": 200, "json": {"data": {"name": "Ürün"}}}});
    decide(&input(a.runtime, "action.finished", "e2", data)).unwrap()
}

#[test]
fn ticket_created_moves_on_and_requests_the_api() {
    let a = requested();
    assert_eq!(a.runtime.state, "idle");
    assert_eq!(a.status, "waiting", "while the request runs");
    let chain = a.runtime.chain.as_ref().unwrap();
    assert_eq!(
        (chain.run.as_str(), chain.transition, chain.index),
        ("e1.2", 1, 0)
    );
    assert_eq!(a.events.len(), 1);
    let e = &a.events[0];
    assert_eq!(
        (e.kind.as_str(), e.target.as_str()),
        ("action.requested", "worker")
    );
    assert_eq!(e.data["action"], "http.request@1");
    assert_eq!(e.data["with"]["url"], "https://api.example.com/items/12");
    assert_eq!(
        e.data["with"]["headers"]["Authorization"], "Bearer ${env.API_KEY}",
        "secrets stay for the daemon"
    );
    assert_eq!(e.data["key"], "12:e1.2:0");
    let timers = a.timers.as_ref().unwrap();
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].at, "2026-10-14T20:05:00Z");
    assert_eq!(timers[0].data, json!({"run": "e1.2", "transition": 1}));
    assert!(a.changes.is_empty());
}

#[test]
fn a_good_answer_runs_the_pure_actions_and_asks_for_the_check() {
    let a = checking();
    assert_eq!(a.runtime.state, "processing");
    assert_eq!(a.status, "processing");
    let chain = a.runtime.chain.as_ref().unwrap();
    assert_eq!(
        (chain.transition, chain.index),
        (2, 2),
        "json.pick and ticket.update ran inline"
    );
    assert_eq!(chain.changes["title"], "Ürün", "held back");
    assert!(
        a.changes.is_empty(),
        "nothing applied before the chain ends"
    );
    let e = &a.events[0];
    assert_eq!(e.data["action"], "checks.is-valid@1");
    assert_eq!(
        e.data["with"]["ticket"]["title"], "Ürün",
        "later actions see held-back changes"
    );
    assert_eq!(
        a.timers.as_deref(),
        Some(&[][..]),
        "the old timer is cleared; this transition has none"
    );
}

#[test]
fn the_last_result_applies_the_changes_and_finishes() {
    let a = checking();
    let run = a.runtime.chain.as_ref().unwrap().run.clone();
    let data = json!({"run": run, "index": 2, "output": {"ok": true}});
    let b = decide(&input(a.runtime, "action.finished", "e3", data)).unwrap();
    assert_eq!(
        (b.runtime.state.as_str(), b.status.as_str()),
        ("finished", "finished")
    );
    assert!(b.runtime.chain.is_none());
    assert_eq!(b.changes["title"], "Ürün");
    assert_eq!(b.changes["note"], "Filled in from the API.");
    assert!(b.events.is_empty());
}

#[test]
fn a_failed_check_fails_the_ticket_and_applies_nothing() {
    let a = requested();
    let data = json!({"run": "e1.2", "index": 0, "output": {"status": 404, "json": {}}});
    let b = decide(&input(a.runtime, "action.finished", "e2", data)).unwrap();
    assert_eq!(
        (b.runtime.state.as_str(), b.status.as_str()),
        ("failed", "failed")
    );
    let f = b.runtime.failed.as_ref().unwrap();
    assert_eq!(
        (f.from.as_str(), f.transition, f.run.as_str()),
        ("idle", 1, "e1.2")
    );
    let note = b.changes["note"].as_str().unwrap();
    assert!(
        note.contains("idle -> processing") && note.contains("http.request@1"),
        "{note}"
    );
    assert!(b.changes.get("title").is_none());
    assert_eq!(b.timers.as_deref(), Some(&[][..]));
}

#[test]
fn a_failed_action_fails_the_ticket() {
    let a = requested();
    let data = json!({"run": "e1.2", "index": 0, "error": "bağlantı kurulamadı"});
    let b = decide(&input(a.runtime, "action.failed", "e2", data)).unwrap();
    assert_eq!(b.runtime.state, "failed");
    assert!(
        b.changes["note"]
            .as_str()
            .unwrap()
            .contains("bağlantı kurulamadı")
    );
}

#[test]
fn on_fail_goes_to_its_state_instead() {
    let a = requested();
    let mut i = input(
        a.runtime,
        "action.failed",
        "e2",
        json!({"run": "e1.2", "index": 0, "error": "x"}),
    );
    i.lifecycle.transitions[1].on_fail = Some("timed-out".into());
    let b = decide(&i).unwrap();
    assert_eq!(b.runtime.state, "timed-out");
    assert!(b.runtime.failed.is_none());
    assert!(b.changes.is_empty());
}

#[test]
fn a_timeout_goes_to_on_timeout() {
    let a = requested();
    let b = decide(&input(
        a.runtime,
        "timer.fired",
        "e2",
        json!({"run": "e1.2", "transition": 1}),
    ))
    .unwrap();
    assert_eq!(b.runtime.state, "timed-out");
    assert!(b.runtime.chain.is_none());
}

#[test]
fn stale_results_and_timers_change_nothing() {
    let a = requested();
    for (kind, data) in [
        (
            "action.finished",
            json!({"run": "eski", "index": 0, "output": {}}),
        ),
        (
            "action.finished",
            json!({"run": "e1.2", "index": 5, "output": {}}),
        ),
        ("timer.fired", json!({"run": "eski", "transition": 1})),
    ] {
        let b = decide(&input(a.runtime.clone(), kind, "e9", data)).unwrap();
        assert_eq!(b.runtime, a.runtime);
        assert!(b.timers.is_none() && b.events.is_empty() && b.changes.is_empty());
        assert!(b.log.iter().any(|l| l.contains("stale")), "{:?}", b.log);
    }
}

#[test]
fn a_person_moving_the_ticket_drops_the_chain() {
    let a = requested();
    let b = decide(&input(
        a.runtime,
        "ticket.changed",
        "e2",
        json!({"status": "finished"}),
    ))
    .unwrap();
    assert_eq!(b.runtime.state, "finished");
    assert!(b.runtime.chain.is_none());
    assert_eq!(b.timers.as_deref(), Some(&[][..]));
}

#[test]
fn resume_runs_the_failed_transition_again_with_the_same_keys() {
    let a = requested();
    let data = json!({"run": "e1.2", "index": 0, "output": {"status": 500}});
    let failed = decide(&input(a.runtime, "action.finished", "e2", data)).unwrap();
    let b = decide(&input(failed.runtime, "lifecycle.resume", "e3", json!({}))).unwrap();
    assert_eq!(
        (b.runtime.state.as_str(), b.status.as_str()),
        ("idle", "waiting")
    );
    assert_eq!(b.events[0].data["key"], "12:e1.2:0");
    assert!(b.runtime.failed.is_none());
}

#[test]
fn events_that_match_nothing_are_logged() {
    let b = decide(&input(at("create"), "input.received", "e1", json!({}))).unwrap();
    assert_eq!(b.runtime, at("create"));
    assert!(
        b.log.iter().any(|l| l.contains("no transition")),
        "{:?}",
        b.log
    );
}

#[test]
fn when_chooses_among_transitions() {
    let mut i = input(at("create"), "ticket.created", "e1", json!({"öncelik": 1}));
    i.lifecycle.transitions.insert(
        0,
        primum::lifecycle::format::Transition {
            from: "create".into(),
            to: "finished".into(),
            description: String::new(),
            on: Some("ticket.created".into()),
            when: Some("input['öncelik'] > 5".into()),
            actions: vec![],
            showing: None,
            timeout: None,
            on_timeout: None,
            on_fail: None,
        },
    );
    assert_eq!(
        decide(&i).unwrap().runtime.state,
        "idle",
        "1 is not > 5: the next one"
    );
    i.event.data = json!({"öncelik": 9});
    assert_eq!(decide(&i).unwrap().runtime.state, "finished");
}

#[test]
fn an_endless_loop_fails_the_ticket() {
    let mut i = input(at("create"), "ticket.created", "e1", json!({}));
    i.lifecycle.transitions = serde_json::from_value(json!([
        { "from": "create", "on": "ticket.created", "to": "idle" },
        { "from": "idle", "to": "processing", "do": [{ "action": "ticket.update@1", "with": { "note": "döngü" } }] },
        { "from": "processing", "to": "idle", "do": [{ "action": "ticket.update@1", "with": { "note": "döngü" } }] }
    ])).unwrap();
    let a = decide(&i).unwrap();
    assert_eq!(a.runtime.state, "failed");
    assert!(a.changes["note"].as_str().unwrap().contains("loop"));
}

#[test]
fn emitted_events_wait_for_the_chain_to_succeed() {
    let mut i = input(at("create"), "ticket.created", "e1", json!({}));
    i.lifecycle.transitions = serde_json::from_value(json!([
        { "from": "create", "on": "ticket.created", "to": "finished",
          "do": [ { "action": "event.emit@1", "with": { "type": "iş.bitti", "target": "project", "data": { "id": "${ticket.id}" } } } ] }
    ])).unwrap();
    let a = decide(&i).unwrap();
    assert_eq!(a.runtime.state, "finished");
    assert_eq!(a.events.len(), 1);
    assert_eq!(
        (a.events[0].kind.as_str(), a.events[0].target.as_str()),
        ("iş.bitti", "project")
    );
    assert_eq!(a.events[0].data, json!({"id": "12"}));
}

#[test]
fn a_waiting_state_gets_a_timer_for_each_timed_wait() {
    let mut i = input(at("create"), "ticket.created", "e1", json!({}));
    i.lifecycle.transitions = serde_json::from_value(json!([
        { "from": "create", "on": "ticket.created", "to": "idle" },
        { "from": "idle", "on": "input.received", "to": "finished", "timeout": "2d", "on_timeout": "timed-out" }
    ])).unwrap();
    let a = decide(&i).unwrap();
    assert_eq!(a.runtime.state, "idle");
    let t = &a.timers.as_ref().unwrap()[0];
    assert_eq!(t.at, "2026-10-16T20:00:00Z");
    assert_eq!(t.data, json!({"state": "idle", "transition": 1}));
    let b = decide(&input(a.runtime, "timer.fired", "e2", t.data.clone())).unwrap();
    assert_eq!(b.runtime.state, "timed-out");
}

#[test]
fn the_same_input_gives_the_same_answer() {
    let i = input(at("create"), "ticket.created", "e1", json!({}));
    assert_eq!(decide(&i).unwrap(), decide(&i).unwrap());
}

#[test]
fn bad_input_is_an_error() {
    let mut i = input(at("create"), "ticket.created", "e1", json!({}));
    i.now = "dün".into();
    assert!(decide(&i).unwrap_err().contains("now"));
}
