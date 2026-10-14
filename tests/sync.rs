mod common;
use primum::history::clone;
use primum::store::*;
use primum::sync::sync;
use std::path::Path;

fn ch() -> Changes {
    Changes::default()
}

/// Özgür's project on a fresh remote with #1 (a BUG) and #2 (a TODO), and Işıl's
/// clone of it.
fn two(dir: &Path) -> (Project, Project) {
    let url = common::remote(dir);
    let oz = common::person(dir, "Özgür");
    std::fs::write(oz.root.join("primum.toml"), "name = \"ortak\"\n").unwrap();
    insert(&oz, "BUG", "Gökçe'nin hatası", ch()).unwrap();
    let body = Changes {
        body: Some("bir\niki\nüç\n".into()),
        ..ch()
    };
    insert(&oz, "TODO", "Özgür'ün işi", body).unwrap();
    common::history(&oz).set_remote(&url, None, false).unwrap();
    assert!(sync(&oz).unwrap().pushed);
    clone(&url, None, &dir.join("Işıl")).unwrap();
    let is = Project {
        root: dir.join("Işıl"),
        name: "ortak".into(),
    };
    let h = common::history(&is);
    h.git(&["config", "user.name", "Işıl"]).unwrap();
    h.git(&["config", "user.email", "isil@example.com"])
        .unwrap();
    (oz, is)
}

fn set(p: &Project, id: &str, c: Changes) {
    update(p, id, c).unwrap();
}

#[test]
fn different_items_and_adjacent_fields_merge_without_asking() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    let doing = Changes {
        status: Some("doing".into()),
        ..ch()
    };
    set(&oz, "2", doing);
    let effort = Changes {
        effort: Some(2),
        ..ch()
    };
    set(&oz, "1", effort);
    sync(&oz).unwrap();
    let impact = Changes {
        impact: Some(9),
        ..ch()
    };
    set(&is, "2", impact);
    let r = sync(&is).unwrap();
    assert!(r.pulled && r.pushed && r.conflicts.is_empty(), "{r:?}");
    let b = load(&is).unwrap();
    let t = b.get("2").unwrap();
    assert_eq!((t.status.as_str(), t.impact), ("doing", 9));
    assert_eq!(b.get("1").unwrap().effort, 2);
    sync(&oz).unwrap();
    assert_eq!(load(&oz).unwrap().get("2").unwrap().impact, 9);
    let subjects = common::subjects(&is);
    assert!(
        subjects.iter().any(|s| s.starts_with("sync with main")),
        "{subjects:?}"
    );
}

#[test]
fn bodies_merge_by_line() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    let a = Changes {
        body: Some("BİR\niki\nüç\n".into()),
        ..ch()
    };
    set(&oz, "2", a);
    sync(&oz).unwrap();
    let b = Changes {
        body: Some("bir\niki\nÜÇ\n".into()),
        ..ch()
    };
    set(&is, "2", b);
    assert!(sync(&is).unwrap().conflicts.is_empty());
    let body = load(&is).unwrap().get("2").unwrap().body.clone();
    assert_eq!(body, "BİR\niki\nÜÇ\n");
}

#[test]
fn the_same_new_id_on_both_sides_renumbers_the_local_one() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    insert(&oz, "TODO", "Özgür'ün yenisi", ch()).unwrap();
    sync(&oz).unwrap();
    insert(&is, "TODO", "Işıl'ın yenisi", ch()).unwrap();
    let needs = Changes {
        needs: Some(vec!["3".into()]),
        body: Some("After #3.\n".into()),
        ..ch()
    };
    insert(&is, "BUG", "bekler", needs).unwrap();
    set_doc(&is, "3", "spec", "# Işıl\n").unwrap();
    let r = sync(&is).unwrap();
    // #3 was taken on the remote; the local #3 moves above every number
    // on both sides (#4 is local)
    assert_eq!(r.renumbered, [("3".to_string(), "5".to_string())]);
    let b = load(&is).unwrap();
    assert_eq!(b.get("3").unwrap().title, "Özgür'ün yenisi");
    assert_eq!(b.get("5").unwrap().title, "Işıl'ın yenisi");
    let waiting = b.get("4").unwrap();
    assert_eq!(waiting.needs, vec!["5".to_string()]);
    assert_eq!(waiting.body, "After #5.\n");
    assert!(is.root.join("items/5.spec.md").is_file());
}

#[test]
fn a_delete_against_an_edit_keeps_the_item() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    delete(&oz, "1").unwrap();
    sync(&oz).unwrap();
    let risk = Changes {
        risk: Some(9),
        ..ch()
    };
    set(&is, "1", risk);
    let r = sync(&is).unwrap();
    assert_eq!(r.kept.len(), 1, "{r:?}");
    assert_eq!(load(&is).unwrap().get("1").unwrap().risk, 9);
}

#[test]
fn a_true_conflict_is_recorded_everywhere() {
    let d = tempfile::tempdir().unwrap();
    let (oz, is) = two(d.path());
    let done = Changes {
        status: Some("done".into()),
        ..ch()
    };
    set(&oz, "2", done);
    sync(&oz).unwrap();
    let wontfix = Changes {
        status: Some("wontfix".into()),
        ..ch()
    };
    set(&is, "2", wontfix);
    let r = sync(&is).unwrap();
    assert_eq!(r.conflicts, ["2"]);
    let b = load(&is).unwrap();
    let c = &b.conflicts["2"];
    assert_eq!(
        c.fields[0].values,
        ["wontfix".to_string(), "done".to_string()]
    );
    let authors = (c.sides[0].author.as_str(), c.sides[1].author.as_str());
    assert_eq!(authors, ("Işıl", "Özgür"));
    assert_eq!(b.get("2").unwrap().status, "wontfix");
    sync(&oz).unwrap();
    assert!(
        load(&oz).unwrap().conflicts.contains_key("2"),
        "the conflict reaches Özgür"
    );
}

#[test]
fn offline_keeps_the_change() {
    let d = tempfile::tempdir().unwrap();
    let (oz, _is) = two(d.path());
    let h = common::history(&oz);
    let real = h.remote().unwrap().unwrap().url;
    h.set_remote("/yok/remote.git", None, false).unwrap();
    let urgency = Changes {
        urgency: Some(8),
        ..ch()
    };
    set(&oz, "1", urgency);
    let r = sync(&oz).unwrap();
    assert!(r.offline.is_some() && !r.pushed);
    assert_eq!(common::subjects(&oz)[0], "#1: urgency");
    h.set_remote(&real, None, false).unwrap();
    assert!(sync(&oz).unwrap().pushed);
}

#[test]
fn an_unrelated_history_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let (_oz, _is) = two(d.path());
    let url = d.path().join("remote.git").to_string_lossy().to_string();
    let gokce = common::person(d.path(), "Gökçe");
    insert(&gokce, "TODO", "ayrı başladı", ch()).unwrap();
    common::history(&gokce)
        .set_remote(&url, None, false)
        .unwrap();
    let e = sync(&gokce).unwrap_err().to_string();
    assert!(e.contains("primum clone"), "{e}");
}
