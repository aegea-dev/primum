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
    fs::write(d.path().join("items/1.md.tmp"), "").unwrap();
    fs::write(d.path().join("items/1.md"), "---\ntitle: x\n---\n").unwrap();
    let h = History::open(&p).unwrap().unwrap();
    assert!(d.path().join(".primum/history.git/HEAD").is_file());
    assert_eq!(
        fs::read_to_string(d.path().join(".primum/.gitignore")).unwrap(),
        "*\n"
    );
    assert_eq!(common::subjects(&p), ["start history"]);
    assert_eq!(common::tracked(&p), ["items/1.md", "primum.toml"]);
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
    fs::write(d.path().join("items/1.md"), "x").unwrap();
    fs::write(d.path().join("BOARD.md"), "# deneme\n").unwrap();
    fs::create_dir_all(d.path().join("conflicts")).unwrap();
    fs::write(d.path().join("conflicts/1.json"), "{}").unwrap();
    fs::write(d.path().join("başka.md"), "x").unwrap();
    assert!(h.commit("#1: add \"Çay\"").unwrap());
    assert_eq!(common::subjects(&p)[0], "#1: add \"Çay\"");
    assert_eq!(
        common::tracked(&p),
        ["BOARD.md", "conflicts/1.json", "items/1.md"]
    );
    fs::remove_file(d.path().join("items/1.md")).unwrap();
    assert!(h.commit("#1: delete").unwrap());
    assert_eq!(common::tracked(&p), ["BOARD.md", "conflicts/1.json"]);
    assert_eq!(h.deleted_numbers().unwrap(), [1]);
}

#[test]
fn outside_edits_are_committed_apart() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let h = History::open(&p).unwrap().unwrap();
    assert!(!h.record_outside().unwrap());
    fs::write(d.path().join("items/1.md"), "elle yazıldı").unwrap();
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
    fs::write(d.path().join("items/1.md"), "x").unwrap();
    h.commit("x").unwrap();
    assert_eq!(
        h.git(&["log", "-1", "--format=%an"]).unwrap().trim(),
        "Özgür"
    );
}

#[test]
fn log_lists_an_items_changes_with_their_diffs() {
    use primum::store::*;
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Çay", Changes::default()).unwrap();
    insert(&p, "BUG", "başka", Changes::default()).unwrap();
    let risk = Changes {
        risk: Some(8),
        ..Default::default()
    };
    update(&p, "1", risk).unwrap();
    set_kind(&p, "1", "BUG").unwrap();
    let h = common::history(&p);
    let log = h.log(Some("items/1.md"), 50).unwrap();
    let msgs: Vec<&str> = log.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(msgs, ["#1: kind BUG", "#1: risk", "#1: add \"Çay\""]);
    assert_eq!(log[2].path.as_deref(), Some("items/1.md"));
    let diff = h.diff(&log[1].commit, "items/1.md").unwrap();
    assert!(
        diff.contains("-risk: 3") && diff.contains("+risk: 8"),
        "{diff}"
    );
    assert_eq!(h.log(None, 2).unwrap().len(), 2);
    assert!(h.diff("../x", "items/1.md").is_err());
    assert!(h.diff(&log[1].commit, "../primum.toml").is_err());
}

#[test]
fn remote_settings_live_in_the_history_not_primum_toml() {
    let d = tempfile::tempdir().unwrap();
    let url = common::remote(d.path());
    let p = common::person(d.path(), "Özgür");
    let h = common::history(&p);
    assert!(h.remote().unwrap().is_none());
    let r = h.set_remote(&url, Some("videre"), false).unwrap();
    assert_eq!((r.branch.as_str(), r.manual), ("videre", false));
    assert_eq!(h.remote().unwrap().unwrap().url, url);
    let head = h.git(&["symbolic-ref", "--short", "HEAD"]).unwrap();
    assert_eq!(head.trim(), "videre");
    h.set_remote(&url, None, true).unwrap();
    let r = h.remote().unwrap().unwrap();
    assert!(r.manual && r.branch == "videre");
    h.remove_remote().unwrap();
    assert!(h.remote().unwrap().is_none());
}

#[test]
fn push_fetch_and_clone() {
    use primum::history::{Fetched, Pushed, clone};
    let d = tempfile::tempdir().unwrap();
    let url = common::remote(d.path());
    let p = common::person(d.path(), "Özgür");
    fs::write(p.root.join("primum.toml"), "name = \"ortak\"\n").unwrap();
    primum::store::insert(&p, "TODO", "Gökçe'nin işi", Default::default()).unwrap();
    let h = common::history(&p);
    let r = h.set_remote(&url, None, false).unwrap();
    assert!(matches!(h.fetch(&r).unwrap(), Fetched::Empty));
    assert!(matches!(h.push(&r).unwrap(), Pushed::Ok));
    assert!(matches!(h.fetch(&r).unwrap(), Fetched::Ok));
    let other = d.path().join("ışıl");
    clone(&url, None, &other).unwrap();
    assert!(other.join("items/1.md").is_file());
    assert!(other.join("BOARD.md").is_file());
    assert!(clone(&url, None, &other).is_err(), "already a project");
    let gone = h.set_remote("/yok/remote.git", None, false).unwrap();
    assert!(matches!(h.fetch(&gone).unwrap(), Fetched::Offline(_)));
}

#[test]
fn a_history_from_an_earlier_version_tracks_project_json() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    fs::write(d.path().join("primum.toml"), "name = \"deneme\"\n").unwrap();
    History::open(&p).unwrap().unwrap();
    // the allowlist an earlier version wrote
    let exclude = d.path().join(".primum/history.git/info/exclude");
    fs::write(
        &exclude,
        "/*\n!/primum.toml\n!/BOARD.md\n!/items/\n/items/*\n!/items/*.md\n",
    )
    .unwrap();
    fs::write(d.path().join("project.json"), "{\"name\": \"deneme\"}\n").unwrap();
    fs::remove_file(d.path().join("primum.toml")).unwrap();
    let h = History::open(&p).unwrap().unwrap();
    h.commit("move settings to project.json").unwrap();
    assert_eq!(common::tracked(&p), ["project.json"]);
}

#[test]
fn the_github_folder_is_not_tracked() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    fs::write(d.path().join("project.json"), "{\"name\": \"deneme\"}\n").unwrap();
    fs::create_dir_all(d.path().join("github")).unwrap();
    fs::write(d.path().join("github/1.json"), "{}\n").unwrap();
    History::open(&p).unwrap().unwrap();
    assert_eq!(common::tracked(&p), ["project.json"]);
}

#[test]
fn lifecycle_files_are_tracked() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    fs::write(d.path().join("project.json"), "{\"name\": \"deneme\"}\n").unwrap();
    fs::create_dir_all(d.path().join("lifecycles")).unwrap();
    fs::write(d.path().join("lifecycles/çay.toml"), "name = \"çay\"\n").unwrap();
    fs::write(d.path().join("lifecycles/notes.txt"), "not a lifecycle\n").unwrap();
    History::open(&p).unwrap().unwrap();
    assert_eq!(common::tracked(&p), ["lifecycles/çay.toml", "project.json"]);
}
