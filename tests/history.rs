mod common;
use primum::history::History;
use std::fs;

#[test]
fn open_creates_the_bare_repo_with_a_first_commit_of_what_is_there() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    fs::write(d.path().join("primum.toml"), "name = \"deneme\"\n").unwrap();
    fs::write(d.path().join("notlar.txt"), "Gökçe'nin notu\n").unwrap();
    fs::write(d.path().join(".primum.lock"), "").unwrap();
    fs::write(d.path().join("items/TODO-1.md.tmp"), "").unwrap();
    fs::write(d.path().join("items/TODO-1.md"), "---\ntitle: x\n---\n").unwrap();
    let h = History::open(&p).unwrap().unwrap();
    assert!(d.path().join(".primum/history.git/HEAD").is_file());
    assert_eq!(
        fs::read_to_string(d.path().join(".primum/.gitignore")).unwrap(),
        "*\n"
    );
    assert_eq!(common::subjects(&p), ["start history"]);
    assert_eq!(common::tracked(&p), ["items/TODO-1.md", "primum.toml"]);
    assert!(h.head().unwrap().is_some());
    History::open(&p).unwrap().unwrap();
    assert_eq!(
        common::subjects(&p).len(),
        1,
        "opening again changes nothing"
    );
}

#[test]
fn commit_takes_only_primums_files_and_reports_whether_it_committed() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let h = History::open(&p).unwrap().unwrap();
    assert!(!h.commit("boş").unwrap(), "nothing to commit");
    fs::write(d.path().join("items/BUG-1.md"), "x").unwrap();
    fs::write(d.path().join("BOARD.md"), "# deneme\n").unwrap();
    fs::create_dir_all(d.path().join("conflicts")).unwrap();
    fs::write(d.path().join("conflicts/BUG-1.json"), "{}").unwrap();
    fs::write(d.path().join("başka.md"), "x").unwrap();
    assert!(h.commit("BUG:1: add \"Işıl\"").unwrap());
    assert_eq!(common::subjects(&p)[0], "BUG:1: add \"Işıl\"");
    assert_eq!(
        common::tracked(&p),
        ["BOARD.md", "conflicts/BUG-1.json", "items/BUG-1.md"]
    );
    fs::remove_file(d.path().join("items/BUG-1.md")).unwrap();
    assert!(h.commit("BUG:1: delete").unwrap());
    assert_eq!(common::tracked(&p), ["BOARD.md", "conflicts/BUG-1.json"]);
}

#[test]
fn outside_edits_are_committed_apart() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let h = History::open(&p).unwrap().unwrap();
    assert!(!h.record_outside().unwrap());
    fs::write(d.path().join("items/TODO-1.md"), "elle yazıldı").unwrap();
    assert!(h.record_outside().unwrap());
    assert_eq!(common::subjects(&p)[0], "edited outside primum");
}

#[test]
fn author_is_the_configured_identity() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let h = History::open(&p).unwrap().unwrap();
    h.git(&["config", "user.name", "Özgür"]).unwrap();
    h.git(&["config", "user.email", "ozgur@example.com"])
        .unwrap();
    fs::write(d.path().join("items/TODO-1.md"), "x").unwrap();
    h.commit("x").unwrap();
    assert_eq!(
        h.git(&["log", "-1", "--format=%an"]).unwrap().trim(),
        "Özgür"
    );
}
