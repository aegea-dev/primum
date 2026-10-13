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
