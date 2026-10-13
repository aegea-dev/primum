mod common;
use primum::github::*;
use primum::github_sync::run;
use primum::store::*;

fn ch() -> Changes {
    Changes::default()
}

fn synced() -> (tempfile::TempDir, Project, Memory) {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let body = Changes {
        body: Some("bir\n".into()),
        ..ch()
    };
    insert(&p, "BUG", "Gökçe'nin hatası", body).unwrap();
    insert(&p, "TODO", "Özgür'ün işi", ch()).unwrap();
    let m = Memory::new();
    run(&p, &m, None).unwrap();
    (d, p, m)
}

fn link(p: &Project, id: &str) -> u64 {
    load(p).unwrap().get(id).unwrap().github.unwrap()
}

#[test]
fn items_become_issues_and_keep_their_link() {
    let (_d, p, m) = synced();
    assert_eq!(m.all().len(), 2);
    let n = link(&p, "BUG:1");
    assert_eq!(m.issue(n).labels, ["kind:bug"]);
    assert!(
        m.issue(n)
            .body
            .starts_with("bir\n\n<!-- primum\nid: BUG:1\n")
    );
    let (r, _) = run(&p, &m, None).unwrap();
    assert!(
        r.created.is_empty() && r.updated_issues.is_empty() && r.updated_items.is_empty(),
        "nothing changed: {r:?}"
    );
}

#[test]
fn edits_flow_both_ways_and_different_fields_merge() {
    let (_d, p, m) = synced();
    let n = link(&p, "BUG:1");
    let effort = Changes {
        effort: Some(2),
        ..ch()
    };
    update(&p, "BUG:1", effort).unwrap();
    m.edit(n, |i| i.title = "Işıl'ın başlığı".into());
    run(&p, &m, None).unwrap();
    let it = load(&p).unwrap().get("BUG:1").unwrap().clone();
    assert_eq!((it.title.as_str(), it.effort), ("Işıl'ın başlığı", 2));
    assert!(m.issue(n).body.contains("effort: 2"));
    assert_eq!(m.issue(n).title, "Işıl'ın başlığı");
}

#[test]
fn closing_and_reopening_on_github() {
    let (_d, p, m) = synced();
    let n = link(&p, "TODO:1");
    m.edit(n, |i| {
        i.open = false;
        i.reason = Some("not_planned".into());
    });
    run(&p, &m, None).unwrap();
    let it = load(&p).unwrap().get("TODO:1").unwrap().clone();
    assert_eq!(it.status, "wontfix");
    assert!(it.finished.is_some());
    m.edit(n, |i| {
        i.open = true;
        i.reason = None;
    });
    run(&p, &m, None).unwrap();
    let it = load(&p).unwrap().get("TODO:1").unwrap().clone();
    assert_eq!((it.status.as_str(), it.finished), ("open", None));
}

#[test]
fn a_new_issue_becomes_an_item_and_a_kind_label_moves_it() {
    let (_d, p, m) = synced();
    let plain = IssueWrite {
        title: "Gökçe'nin isteği".into(),
        body: "Lütfen.".into(),
        open: true,
        reason: None,
        labels: vec![],
    };
    let n = m.create(&plain).unwrap().number;
    let (r, _) = run(&p, &m, None).unwrap();
    assert_eq!(r.added, ["TODO:2"]);
    let it = load(&p).unwrap().get("TODO:2").unwrap().clone();
    assert_eq!((it.github, it.body.as_str()), (Some(n), "Lütfen.\n"));
    assert!(
        m.issue(n).body.contains("id: TODO:2"),
        "the issue gets primum's block"
    );
    assert_eq!(m.issue(n).labels, ["kind:todo"]);
    m.edit(n, |i| i.labels = vec!["kind:bug".into()]);
    run(&p, &m, None).unwrap();
    let b = load(&p).unwrap();
    let moved = b.items.iter().find(|i| i.github == Some(n)).unwrap();
    assert_eq!(
        (moved.id.as_str(), moved.title.as_str()),
        ("BUG:2", "Gökçe'nin isteği")
    );
    assert!(m.issue(n).body.contains("id: BUG:2"));
}

#[test]
fn the_same_field_on_both_sides_is_a_conflict_not_pushed() {
    let (_d, p, m) = synced();
    let n = link(&p, "BUG:1");
    let title = Changes {
        title: Some("burada".into()),
        ..ch()
    };
    update(&p, "BUG:1", title).unwrap();
    m.edit(n, |i| i.title = "orada".into());
    let (r, _) = run(&p, &m, None).unwrap();
    assert_eq!(r.conflicts, ["BUG:1"]);
    let c = &load(&p).unwrap().conflicts["BUG:1"];
    assert_eq!(
        c.fields[0].values,
        ["burada".to_string(), "orada".to_string()]
    );
    assert!(c.sides[1].author.starts_with("GitHub"));
    assert_eq!(
        m.issue(n).title,
        "orada",
        "the conflicting field stays as GitHub has it"
    );
    let (r, _) = run(&p, &m, None).unwrap();
    assert!(r.conflicts.is_empty(), "not detected again: {r:?}");
    resolve(&p, "BUG:1", "title", Keep::First).unwrap();
    run(&p, &m, None).unwrap();
    assert_eq!(m.issue(n).title, "burada", "the choice reaches GitHub");
}

#[test]
fn a_merged_pull_request_with_a_primum_id_closes_the_item() {
    let (_d, p, m) = synced();
    let pr = m.open_pr("Galeri", "Bu değişiklik Fixes BUG:1 ve fazlası", true);
    let (r, _) = run(&p, &m, None).unwrap();
    assert_eq!(r.closed_by, [("BUG:1".to_string(), pr)]);
    let it = load(&p).unwrap().get("BUG:1").unwrap().clone();
    assert_eq!(it.status, "done");
    assert!(it.body.contains(&format!("closed by #{pr}")), "{}", it.body);
    let issue = m.issue(it.github.unwrap());
    assert_eq!(
        (issue.open, issue.reason.as_deref()),
        (false, Some("completed"))
    );
    m.open_pr("Açık", "Fixes TODO:1", false);
    run(&p, &m, None).unwrap();
    let status = load(&p).unwrap().get("TODO:1").unwrap().status.clone();
    assert_eq!(status, "open", "an unmerged PR closes nothing");
}

#[test]
fn deleting_closes_the_issue_and_duplicates_are_linked_not_created() {
    let (_d, p, m) = synced();
    let n = link(&p, "TODO:1");
    delete(&p, "TODO:1").unwrap();
    run(&p, &m, None).unwrap();
    let i = m.issue(n);
    assert_eq!((i.open, i.reason.as_deref()), (false, Some("not_planned")));
    run(&p, &m, None).unwrap();
    assert!(
        load(&p).unwrap().get("TODO:1").is_none(),
        "not brought back"
    );
    // the number is free again; the old closed issue must not claim it
    let id = insert(&p, "TODO", "ikinci", ch()).unwrap();
    assert_eq!(id, "TODO:1");
    // another machine already filed the new item's issue
    let twin = issue_write(load(&p).unwrap().get(&id).unwrap(), &|_| None);
    let existing = m.create(&twin).unwrap().number;
    run(&p, &m, None).unwrap();
    assert_eq!(link(&p, &id), existing);
    let open_copies = m
        .all()
        .iter()
        .filter(|i| i.open && i.body.contains("id: TODO:1"))
        .count();
    assert_eq!(open_copies, 1);
}

#[test]
fn two_issues_with_one_id_close_the_newer() {
    let (_d, p, m) = synced();
    let first = link(&p, "BUG:1");
    let twin = issue_write(load(&p).unwrap().get("BUG:1").unwrap(), &|_| None);
    let second = m.create(&twin).unwrap().number;
    run(&p, &m, None).unwrap();
    assert!(m.issue(first).open);
    let dup = m.issue(second);
    assert_eq!(
        (dup.open, dup.reason.as_deref()),
        (false, Some("not_planned"))
    );
    assert_eq!(link(&p, "BUG:1"), first);
}

#[test]
fn an_issue_that_disappears_leaves_its_item_unlinked() {
    let (_d, p, m) = synced();
    let n = link(&p, "BUG:1");
    m.remove(n);
    let risk = Changes {
        risk: Some(9),
        ..ch()
    };
    update(&p, "BUG:1", risk).unwrap();
    run(&p, &m, None).unwrap();
    let it = load(&p).unwrap().get("BUG:1").unwrap().clone();
    assert_eq!((it.github, it.risk), (None, 9));
}

#[test]
fn the_title_style_rewrites_issues_and_old_id_labels_go() {
    use primum::github_sync::{Options, run_with};
    let (_d, p, m) = synced();
    let n = link(&p, "TODO:1");
    // an earlier version left per-item labels behind
    m.create_label("TODO:1", "75689A").unwrap();
    m.create_label("kind:todo", "75689A").unwrap();
    let title = Options {
        style: IdStyle::Title,
        rewrite: true,
        ..Default::default()
    };
    run_with(&p, &m, &title).unwrap();
    assert_eq!(m.issue(n).title, "[TODO:1] Özgür'ün işi");
    assert_eq!(m.issue(n).labels, ["kind:todo"]);
    assert_eq!(
        m.labels().unwrap(),
        ["kind:todo"],
        "only id labels are removed"
    );
    // a title edited on GitHub keeps its prefix out of the item
    m.edit(n, |i| i.title = "[TODO:1] Işıl'ın işi".into());
    let keep = Options {
        style: IdStyle::Title,
        ..Default::default()
    };
    run_with(&p, &m, &keep).unwrap();
    assert_eq!(
        load(&p).unwrap().get("TODO:1").unwrap().title,
        "Işıl'ın işi"
    );
    // a lane change updates the prefix, and is not read back as GitHub's
    set_kind(&p, "TODO:1", "DEBT").unwrap();
    run_with(&p, &m, &keep).unwrap();
    assert_eq!(m.issue(n).title, "[DEBT:1] Işıl'ın işi");
    assert!(load(&p).unwrap().get("DEBT:1").is_some());
    // back to no prefix
    let none = Options {
        rewrite: true,
        ..Default::default()
    };
    run_with(&p, &m, &none).unwrap();
    assert_eq!(m.issue(n).title, "Işıl'ın işi");
}
