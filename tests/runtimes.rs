mod common;
use primum::engine::daemon::run_once;
use primum::store::{Changes, attach, insert, load};
use std::path::Path;

fn has(program: &str, args: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn manager() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_primum"))
}

fn now() -> chrono::DateTime<chrono::Utc> {
    "2026-10-14T20:00:00Z".parse().unwrap()
}

/// A project with an actionable `folder/actionable.json` (plus files), and
/// a lifecycle `akış` running `do` from `yeni` to `bitti`.
fn project(
    d: &Path,
    folder: &str,
    definition: &str,
    files: &[(&str, &str)],
    actions: &str,
) -> primum::store::Project {
    let p = common::project(d);
    let a = d.join("actionables").join(folder);
    std::fs::create_dir_all(&a).unwrap();
    std::fs::write(a.join("actionable.json"), definition).unwrap();
    for (name, text) in files {
        std::fs::write(a.join(name), text).unwrap();
    }
    std::fs::create_dir_all(d.join("lifecycles")).unwrap();
    std::fs::write(
        d.join("lifecycles/akış.json"),
        format!(
            r#"{{ "name": "akış", "states": ["yeni", "bitti"], "start": "yeni", "end": ["bitti"],
                 "transitions": [ {{ "from": "yeni", "on": "lifecycle.attached", "to": "bitti", "do": {actions} }} ] }}"#
        ),
    )
    .unwrap();
    p
}

const PYTHON: &str = r#"{ "name": "denetim", "version": 1, "type": "python@3.12",
  "requirements": "requirements.txt",
  "targets": {
    "is-valid": { "module": "kurallar", "callable": "is_valid" },
    "score": { "module": "kurallar", "callable": "score" },
    "boom": { "module": "kurallar", "callable": "boom" } } }"#;

const RULES: &str = r#"
import asyncio

def is_valid(job):
    title = job["ticket"]["title"]
    return {"ok": title == "Ürün", "uzunluk": len(title)}

async def score(job):
    await asyncio.sleep(0)
    return {"ok": True, "puan": len(job["with"]["söz"])}

def boom(job):
    raise ValueError("kötü veri")
"#;

fn python_ready() -> bool {
    let ok = has("uv", &["--version"]) && has("uv", &["python", "find", "3.12"]);
    if !ok {
        eprintln!("skipped: needs uv and Python 3.12");
    }
    ok
}

#[test]
fn a_python_actionable_runs_in_its_own_environment() {
    if !python_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let actions = r#"[ { "action": "ticket.update@1", "with": { "title": "Ürün" } },
                      { "action": "denetim.is-valid@1", "with": { "ticket": "${ticket}" }, "expect": "output.ok && output.uzunluk == 4" },
                      { "action": "denetim.score@1", "with": { "söz": "ışık" }, "expect": "output.puan == 4" } ]"#;
    let p = project(
        d.path(),
        "denetim",
        PYTHON,
        &[
            ("kurallar.py", RULES),
            ("requirements.txt", "# nothing yet\n"),
        ],
        actions,
    );
    let id = insert(&p, "TODO", "Gölge", Changes::default()).unwrap();
    attach(&p, &id, "akış", None).unwrap();
    run_once(&p, now(), manager()).unwrap();
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "bitti", "{}", it.body);
    let venv = d.path().join("actionables/denetim/.venv");
    assert!(
        venv.join(".primum-requirements").is_file(),
        "set up once, marked"
    );
    let tracked = common::tracked(&p);
    assert!(
        tracked
            .iter()
            .any(|f| f == "actionables/denetim/kurallar.py"),
        "{tracked:?}"
    );
    assert!(!tracked.iter().any(|f| f.contains(".venv")), "{tracked:?}");
}

#[test]
fn a_python_exception_fails_the_action_with_its_message() {
    if !python_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let p = project(
        d.path(),
        "denetim",
        PYTHON,
        &[("kurallar.py", RULES), ("requirements.txt", "")],
        r#"[ "denetim.boom@1" ]"#,
    );
    let id = insert(&p, "TODO", "Gölge", Changes::default()).unwrap();
    attach(&p, &id, "akış", None).unwrap();
    run_once(&p, now(), manager()).unwrap();
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "failed");
    assert!(it.body.contains("ValueError: kötü veri"), "{}", it.body);
}

const BUN: &str = r#"{ "name": "metin", "version": 1, "type": "bun@1.2",
  "targets": { "slug": { "file": "metin.ts", "export": "slug" },
               "yok": { "file": "metin.ts", "export": "yok" } } }"#;

const TEXT: &str = r#"export async function slug(job: { ticket: { title: string } }) {
  return { slug: job.ticket.title.toLowerCase().replace(/\s+/g, "-") };
}
"#;

fn bun_ready() -> bool {
    let ok = has("bun", &["--version"]);
    if !ok {
        eprintln!("skipped: needs bun");
    }
    ok
}

#[test]
fn a_bun_actionable_runs_typescript() {
    if !bun_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let actions = r#"[ "metin.slug@1", { "action": "ticket.update@1", "with": { "title": "${prev.slug}" } } ]"#;
    let p = project(d.path(), "metin", BUN, &[("metin.ts", TEXT)], actions);
    let id = insert(&p, "TODO", "Işık ve Gölge", Changes::default()).unwrap();
    attach(&p, &id, "akış", None).unwrap();
    run_once(&p, now(), manager()).unwrap();
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(
        (it.status.as_str(), it.title.as_str()),
        ("bitti", "işık-ve-gölge"),
        "{}",
        it.body
    );
}

#[test]
fn a_missing_bun_export_fails() {
    if !bun_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let p = project(
        d.path(),
        "metin",
        BUN,
        &[("metin.ts", TEXT)],
        r#"[ "metin.yok@1" ]"#,
    );
    let id = insert(&p, "TODO", "Gölge", Changes::default()).unwrap();
    attach(&p, &id, "akış", None).unwrap();
    run_once(&p, now(), manager()).unwrap();
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "failed");
    assert!(it.body.contains("exports no function yok"), "{}", it.body);
}

#[cfg(unix)]
#[test]
fn a_missing_uv_is_named() {
    let d = tempfile::tempdir().unwrap();
    let p = project(
        d.path(),
        "denetim",
        PYTHON,
        &[("kurallar.py", RULES)],
        r#"[ "denetim.is-valid@1" ]"#,
    );
    std::fs::write(d.path().join("project.json"), "{\"name\": \"deneme\"}\n").unwrap();
    let id = insert(&p, "TODO", "Gölge", Changes::default()).unwrap();
    attach(&p, &id, "akış", None).unwrap();
    if !Path::new("/usr/bin/git").exists()
        || Path::new("/usr/bin/uv").exists()
        || Path::new("/bin/uv").exists()
    {
        eprintln!("skipped: needs git, and no uv, in /usr/bin:/bin");
        return;
    }
    let o = std::process::Command::new(manager())
        .args(["daemon", "--once", "--now", "2026-10-14T20:00:00Z"])
        .current_dir(d.path())
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    let it = load(&p).unwrap().get(&id).unwrap().clone();
    assert_eq!(it.status, "failed");
    assert!(it.body.contains("uv is not installed"), "{}", it.body);
}
