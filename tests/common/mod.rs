#![allow(dead_code)]
use primum::store::Project;
use std::path::Path;

pub fn project(dir: &Path) -> Project {
    std::fs::create_dir_all(dir.join("items")).unwrap();
    Project {
        root: dir.to_path_buf(),
        name: "deneme".into(),
    }
}

pub fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

/// A project's history, which must exist.
pub fn history(p: &Project) -> primum::history::History {
    primum::history::History::open(p)
        .unwrap()
        .expect("git is installed")
}

/// Commit subjects, newest first.
pub fn subjects(p: &Project) -> Vec<String> {
    history(p)
        .git(&["log", "--format=%s"])
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The files the history tracks, sorted.
pub fn tracked(p: &Project) -> Vec<String> {
    let mut v: Vec<String> = history(p)
        .git(&["ls-files"])
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    v.sort();
    v
}

/// An empty bare repository to use as a remote, as a path URL.
pub fn remote(dir: &Path) -> String {
    let r = dir.join("remote.git");
    let out = std::process::Command::new("git")
        .args(["init", "--quiet", "--bare", "-b", "main"])
        .arg(&r)
        .output()
        .unwrap();
    assert!(out.status.success());
    r.to_string_lossy().to_string()
}

/// A person: a project folder `name` with its history's identity set.
pub fn person(dir: &Path, name: &str) -> Project {
    let mut p = project(&dir.join(name));
    p.name = "ortak".into();
    let h = history(&p);
    h.git(&["config", "user.name", name]).unwrap();
    let email = format!("{}@example.com", name.to_lowercase());
    h.git(&["config", "user.email", &email]).unwrap();
    p
}

/// The example lifecycle (`tests/fixtures/lifecycles/enrich-ticket.json`)
/// with its project actionable replaced by a built-in, for projects that
/// define no actionables.
pub fn example_json() -> String {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lifecycles/enrich-ticket.json");
    std::fs::read_to_string(p).unwrap().replace(
        r#"{ "action": "checks.is-valid@1", "with": { "ticket": "${ticket}" },
          "expect": "output.ok" }"#,
        r#"{ "action": "command@1", "with": { "run": ["./check.sh"] } }"#,
    )
}

/// Writes the example lifecycle into a project folder.
pub fn with_example(dir: &Path) {
    std::fs::create_dir_all(dir.join("lifecycles")).unwrap();
    std::fs::write(dir.join("lifecycles/enrich-ticket.json"), example_json()).unwrap();
}
