//! The demo (examples/demo) end to end, and under a kill at every step of
//! it: the daemon, a manager or a worker killed as `kill -9` would. However
//! it is killed, no event is lost, no effect happens twice, and every
//! ticket on `publish` ends `published`.
#![cfg(unix)]
use serde_json::Value;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn primum() -> &'static str {
    env!("CARGO_BIN_EXE_primum")
}

/// A fresh demo workspace in `d`, made by examples/demo/setup.sh; returns
/// its project's folder.
fn demo(d: &Path) -> PathBuf {
    let o = Command::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/demo/setup.sh"
    ))
    .arg(d.join("ws"))
    .arg(primum())
    .output()
    .unwrap();
    assert!(o.status.success(), "{o:?}");
    d.join("ws/demo")
}

/// Runs `primum` in the project, with `env` set and the demo's steps quick.
fn run(project: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> Output {
    Command::new(primum())
        .args(args)
        .current_dir(project)
        .env("PRIMUM_NO_SECRET_STORE", "1")
        .env("DEMO_STEP", "0")
        .envs(env.iter().copied())
        .output()
        .unwrap()
}

/// One daemon pass.
fn pass(project: &Path, env: &[(&str, &OsStr)]) -> Output {
    run(
        project,
        &["daemon", "--once", "--now", "2026-10-15T14:00:00Z"],
        env,
    )
}

/// Each ticket's status, by id.
fn statuses(project: &Path) -> Vec<(String, String)> {
    let o = run(project, &["list", "--all"], &[]);
    assert!(o.status.success(), "{o:?}");
    String::from_utf8(o.stdout)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with('#'))
        .map(|l| {
            let id = l.split_whitespace().next().unwrap().trim_start_matches('#');
            let status = l.split_whitespace().last().unwrap();
            (id.to_string(), status.to_string())
        })
        .collect()
}

fn lines(path: PathBuf) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The demo finished as it should, whatever happened on the way.
fn finished(project: &Path, why: &str) {
    let expected = [
        ("1", "published"),
        ("2", "published"),
        ("3", "waiting"),
        ("4", "idle"),
    ]
    .map(|(a, b)| (a.to_string(), b.to_string()));
    assert_eq!(statuses(project), expected, "{why}");
    // each ticket was published once, whatever ran twice
    // (the two run side by side: either may finish first)
    let mut effects = lines(project.join(".primum/demo/effects.log"));
    effects.sort();
    assert_eq!(effects.len(), 2, "{why}: {effects:?}");
    for (id, key) in ["1", "2"].iter().zip(&effects) {
        assert!(key.starts_with(&format!("{id}:")), "{why}: {effects:?}");
        let body = std::fs::read_to_string(project.join(format!("items/{id}.md"))).unwrap();
        assert_eq!(body.matches("Published as").count(), 1, "{why}: {body}");
    }
    // every event was handled, none given up on
    let o = run(project, &["events", "--json"], &[]);
    let events: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap();
    for e in &events {
        assert!(
            e["handled"].is_string() && e["error"].is_null(),
            "{why}: {e}"
        );
    }
}

#[test]
fn the_demo_runs_end_to_end() {
    let d = tempfile::tempdir().unwrap();
    let p = demo(d.path());
    let o = pass(&p, &[]);
    assert!(o.status.success(), "{o:?}");
    finished(&p, "no kill");
    assert_eq!(lines(p.join(".primum/demo/runs.log")).len(), 2);
}

/// Where a kill lands: in the daemon after each of its writes, in a
/// manager before it answers, and in a worker before it runs and after
/// its effect is done.
const POINTS: [&str; 10] = [
    "feed",
    "answer",
    "ledger",
    "written",
    "job-started",
    "job-done",
    "result",
    "manager",
    "worker",
    "worker-done",
];

#[test]
fn the_demo_survives_a_kill_at_every_step() {
    for point in POINTS {
        let d = tempfile::tempdir().unwrap();
        let p = demo(d.path());
        let died = d.path().join("died");
        pass(
            &p,
            &[
                ("PRIMUM_TEST_DIE_AT", OsStr::new(point)),
                ("PRIMUM_TEST_DIE_ONCE", died.as_os_str()),
            ],
        );
        assert!(died.exists(), "{point}: the kill never happened");
        // a killed daemon is started again, as its watchdog would
        for _ in 0..5 {
            let o = pass(&p, &[]);
            assert!(o.status.success(), "{point}: {o:?}");
        }
        finished(&p, point);
    }
}

#[test]
fn a_worker_killed_after_its_effect_runs_again_and_does_not_repeat_it() {
    let d = tempfile::tempdir().unwrap();
    let p = demo(d.path());
    let died = d.path().join("died");
    let o = pass(
        &p,
        &[
            (
                "PRIMUM_TEST_DIE_AT",
                OsStr::new("worker-done:steps.publish@1"),
            ),
            ("PRIMUM_TEST_DIE_ONCE", died.as_os_str()),
        ],
    );
    assert!(o.status.success(), "{o:?}");
    assert!(died.exists(), "the kill never happened");
    finished(&p, "worker-done:steps.publish@1");
    // one publish ran twice; its effect happened once
    assert_eq!(lines(p.join(".primum/demo/runs.log")).len(), 3);
}
