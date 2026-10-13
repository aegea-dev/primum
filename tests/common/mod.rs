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
