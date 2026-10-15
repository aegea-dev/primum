mod common;
use primum::store::*;

fn ch() -> Changes {
    Changes::default()
}

#[test]
fn insert_takes_the_next_number_across_lanes_and_writes_board_md() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    assert_eq!(insert(&p, "TODO", "Çay", ch()).unwrap(), "1");
    assert_eq!(insert(&p, "TODO", "Kahve", ch()).unwrap(), "2");
    assert_eq!(insert(&p, "BUG", "Şeker", ch()).unwrap(), "3");
    assert!(d.path().join("items/2.md").is_file());
    assert!(common::read(d.path(), "items/3.md").contains("\nkind: BUG\n"));
    assert!(common::read(d.path(), "BOARD.md").contains("[#2](items/2.md)"));
    assert!(insert(&p, "IDEA", "x", ch()).is_err());
}

#[test]
fn a_deleted_number_is_not_reused() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "BUG", "Çay", ch()).unwrap();
    insert(&p, "TODO", "Kahve", ch()).unwrap();
    delete(&p, "2").unwrap();
    assert_eq!(
        insert(&p, "KILLER", "Süt", ch()).unwrap(),
        "3",
        "2 was used"
    );
    assert_eq!(common::subjects(&p)[0], "#3: add \"Süt\"");
}

#[test]
fn status_change_stamps_and_clears_finished_and_frees_dependents() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "BUG", "hata", ch()).unwrap();
    let needs = Changes {
        needs: Some(vec!["1".into()]),
        ..ch()
    };
    insert(&p, "TODO", "bekler", needs).unwrap();
    let done = Changes {
        status: Some("done".into()),
        ..ch()
    };
    update(&p, "1", done).unwrap();
    let b = load(&p).unwrap();
    assert!(b.get("1").unwrap().finished.is_some());
    assert!(
        b.get("2").unwrap().needs.is_empty(),
        "a closed id leaves every needs"
    );
    let open = Changes {
        status: Some("idle".into()),
        ..ch()
    };
    update(&p, "1", open).unwrap();
    assert!(load(&p).unwrap().get("1").unwrap().finished.is_none());
    let bad = Changes {
        status: Some("bitti".into()),
        ..ch()
    };
    assert_eq!(
        update(&p, "1", bad).unwrap_err().to_string(),
        "#1: status \"bitti\" is not one of idle, doing, done, wontfix"
    );
    assert_eq!(update(&p, "9", ch()).unwrap_err().to_string(), "no item 9");
}

#[test]
fn changing_the_kind_changes_one_line() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Ekmek", ch()).unwrap();
    set_doc(&p, "1", "spec", "# ışık\n").unwrap();
    let before = common::read(d.path(), "items/1.md");
    set_kind(&p, "1", "BUG").unwrap();
    let after = common::read(d.path(), "items/1.md");
    assert_eq!(after, before.replace("kind: TODO\n", "kind: BUG\n"));
    assert!(
        d.path().join("items/1.spec.md").is_file(),
        "nothing renamed"
    );
    assert_eq!(common::subjects(&p)[0], "#1: kind BUG");
    assert_eq!(
        set_kind(&p, "1", "EPIC").unwrap_err().to_string(),
        "unknown kind EPIC"
    );
}

#[test]
fn an_old_board_says_to_migrate() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    std::fs::write(d.path().join("items/TODO-1.md"), "---\ntitle: x\n---\n").unwrap();
    let e = load(&p).unwrap_err().to_string();
    assert!(e.contains("run primum migrate"), "{e}");
}

#[test]
fn documents_beside_and_shared() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "a", ch()).unwrap();
    insert(&p, "TODO", "b", ch()).unwrap();
    set_doc(&p, "1", "spec", "# S\n").unwrap();
    set_doc(&p, "1", "plan", "# P\n").unwrap();
    assert!(set_doc(&p, "1", "Spec Dosyası", "x").is_err());
    let shared = common::read(d.path(), "items/2.md")
        .replace("needs: []\n", "needs: []\ndocs: [items/1.spec.md]\n");
    std::fs::write(d.path().join("items/2.md"), shared).unwrap();
    let b = load(&p).unwrap();
    let kinds = |id: &str| {
        b.docs[id]
            .iter()
            .map(|x| x.kind.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds("1"), ["spec", "plan"]);
    assert_eq!(kinds("2"), ["spec"]);
    assert_eq!(read_doc(&p, "items/1.plan.md").as_deref(), Some("# P\n"));
    for bad in [
        "items/1.md",
        "primum.toml",
        "../x.md",
        "/etc/passwd",
        "items/../BOARD.md",
        "items/9.spec.md",
    ] {
        assert!(read_doc(&p, bad).is_none(), "{bad}");
    }
}

#[test]
fn delete_removes_item_and_documents_and_needs() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "BUG", "silinecek", ch()).unwrap();
    let needs = Changes {
        needs: Some(vec!["1".into()]),
        ..ch()
    };
    insert(&p, "TODO", "bekler", needs).unwrap();
    set_doc(&p, "1", "report", "# R\n").unwrap();
    let gone = delete(&p, "1").unwrap();
    assert_eq!(gone, ["items/1.md", "items/1.report.md"]);
    assert!(!d.path().join("items/1.report.md").exists());
    assert!(load(&p).unwrap().get("2").unwrap().needs.is_empty());
    assert!(delete(&p, "1").is_err());
}

#[test]
fn a_note_goes_under_a_dated_heading() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let body = Changes {
        body: Some("İlk.\n".into()),
        ..ch()
    };
    insert(&p, "TODO", "x", body).unwrap();
    append_note(&p, "1", "Ölçüldü.\n").unwrap();
    let body = load(&p).unwrap().get("1").unwrap().body.clone();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert_eq!(body, format!("İlk.\n\n---\n\n## {today}\n\nÖlçüldü.\n"));
}

#[test]
fn a_malformed_file_stops_the_load_with_its_name() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    std::fs::write(d.path().join("items/1.md"), "title: x\n").unwrap();
    assert!(load(&p).unwrap_err().to_string().contains("1.md:1"));
}

#[test]
fn board_md_lists_open_by_lane_and_closed() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Kahve işi", ch()).unwrap();
    insert(&p, "BUG", "Çay hatası", ch()).unwrap();
    let done = Changes {
        status: Some("wontfix".into()),
        ..ch()
    };
    update(&p, "1", done).unwrap();
    let md = common::read(d.path(), "BOARD.md");
    assert!(md.starts_with("# deneme\n"));
    assert!(md.contains("1 open, 1 closed, 2 total."));
    assert!(md.contains("### Bugs (1)"));
    assert!(
        md.contains("| [#2](items/2.md) | 5 | 3 | 3 | 5 | 22.0 | idle | Çay hatası |"),
        "{md}"
    );
    assert!(md.contains("## Closed"));
    assert!(!md.contains("### TODOs"));
}

#[test]
fn every_change_is_one_commit_with_its_message() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Çay listesi", ch()).unwrap();
    let doing = Changes {
        status: Some("doing".into()),
        ..ch()
    };
    update(&p, "1", doing).unwrap();
    let scores = Changes {
        impact: Some(7),
        effort: Some(3),
        ..ch()
    };
    update(&p, "1", scores).unwrap();
    append_note(&p, "1", "Ölçüldü.").unwrap();
    set_doc(&p, "1", "spec", "# S\n").unwrap();
    set_kind(&p, "1", "BUG").unwrap();
    delete(&p, "1").unwrap();
    assert_eq!(
        common::subjects(&p),
        [
            "#1: delete",
            "#1: kind BUG",
            "#1: spec document",
            "#1: note",
            "#1: impact, effort",
            "#1: status doing",
            "#1: add \"Çay listesi\"",
        ],
        "an empty project has nothing for a first commit"
    );
}

#[test]
fn an_outside_edit_is_committed_before_the_next_change() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "x", ch()).unwrap();
    let f = d.path().join("items/1.md");
    let edited = std::fs::read_to_string(&f)
        .unwrap()
        .replace("risk: 3", "risk: 9");
    std::fs::write(&f, edited).unwrap();
    let done = Changes {
        status: Some("done".into()),
        ..ch()
    };
    update(&p, "1", done).unwrap();
    assert_eq!(
        common::subjects(&p)[..2],
        ["#1: status done", "edited outside primum"]
    );
}

#[test]
fn a_failed_change_commits_nothing() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "x", ch()).unwrap();
    assert!(update(&p, "9", ch()).is_err());
    assert_eq!(common::subjects(&p).len(), 1);
}

#[test]
fn conflicts_show_on_the_board_and_go_with_their_item() {
    use primum::conflicts::*;
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Çay", ch()).unwrap();
    let side = |a: &str| Side {
        author: a.into(),
        date: "2026-10-14T09:00:00+01:00".into(),
    };
    let c = Conflict {
        id: "1".into(),
        sides: [side("Ada"), side("Can")],
        fields: vec![FieldValues {
            field: "status".into(),
            values: ["done".into(), "wontfix".into()],
        }],
        body: None,
        documents: vec![],
    };
    write(&p, &c).unwrap();
    assert!(d.path().join("conflicts/1.json").is_file());
    let b = load(&p).unwrap();
    assert_eq!(b.conflicts["1"].fields[0].values[1], "wontfix");
    let risk = Changes {
        risk: Some(4),
        ..ch()
    };
    update(&p, "1", risk).unwrap();
    let md = common::read(d.path(), "BOARD.md");
    assert!(
        md.contains("## Conflicts\n\n- [#1](items/1.md): status"),
        "{md}"
    );
    set_kind(&p, "1", "BUG").unwrap();
    assert!(
        d.path().join("conflicts/1.json").is_file(),
        "the kind is a field"
    );
    delete(&p, "1").unwrap();
    assert!(!d.path().join("conflicts/1.json").exists());
}

#[test]
fn resolving_writes_the_choice_and_clears_the_record() {
    use primum::conflicts::*;
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let body = Changes {
        body: Some("bizim\n".into()),
        ..ch()
    };
    insert(&p, "TODO", "x", body).unwrap();
    set_doc(&p, "1", "spec", "# bizim\n").unwrap();
    let other = d.path().join("items/1.spec.abc1234.md");
    std::fs::write(&other, "# onların\n").unwrap();
    let side = |a: &str| Side {
        author: a.into(),
        date: String::new(),
    };
    let c = Conflict {
        id: "1".into(),
        sides: [side("Ada"), side("Can")],
        fields: vec![
            FieldValues {
                field: "impact".into(),
                values: ["5".into(), "8".into()],
            },
            FieldValues {
                field: "kind".into(),
                values: ["TODO".into(), "DEBT".into()],
            },
        ],
        body: Some(BodyValues {
            values: ["bizim\n".into(), "onların\n".into()],
        }),
        documents: vec![DocConflict {
            path: "items/1.spec.md".into(),
            other: "items/1.spec.abc1234.md".into(),
        }],
    };
    write(&p, &c).unwrap();
    resolve(&p, "1", "impact", Keep::Second).unwrap();
    assert_eq!(load(&p).unwrap().get("1").unwrap().impact, 8);
    resolve(&p, "1", "kind", Keep::Second).unwrap();
    assert_eq!(load(&p).unwrap().get("1").unwrap().kind, "DEBT");
    assert!(resolve(&p, "1", "body", Keep::Both).is_err());
    resolve(&p, "1", "body", Keep::First).unwrap();
    resolve(&p, "1", "items/1.spec.md", Keep::Second).unwrap();
    assert_eq!(common::read(d.path(), "items/1.spec.md"), "# onların\n");
    assert!(!other.exists());
    assert!(
        !d.path().join("conflicts/1.json").exists(),
        "empty record removed"
    );
    assert_eq!(common::subjects(&p)[0], "#1: resolve items/1.spec.md");
    assert!(resolve(&p, "1", "impact", Keep::First).is_err());
}

#[test]
fn a_missing_title_comes_from_the_first_heading() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let body = |b: &str| Changes {
        body: Some(b.into()),
        ..ch()
    };
    let id = insert(
        &p,
        "TODO",
        "  ",
        body("Intro line\n\n### Işık ve gölge #2\n\ntext\n"),
    )
    .unwrap();
    let board = load(&p).unwrap();
    let it = board.get(&id).unwrap();
    assert_eq!(it.title, "Işık ve gölge #2");
    assert!(
        it.body.contains("### Işık ve gölge #2"),
        "the heading stays"
    );
    let e = insert(&p, "TODO", "", body("no heading here\n")).unwrap_err();
    assert_eq!(
        e.to_string(),
        "give a title, or start the text with a heading"
    );
    assert!(insert(&p, "TODO", "", ch()).is_err());
    assert_eq!(load(&p).unwrap().items.len(), 1, "nothing added");
}

#[test]
fn statuses_are_idle_doing_done_and_wontfix() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let id = insert(&p, "TODO", "Çay", ch()).unwrap();
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "idle");
    for s in ["doing", "done", "idle", "wontfix", "idle"] {
        let c = Changes {
            status: Some(s.into()),
            ..ch()
        };
        update(&p, &id, c).unwrap();
    }
    let open = Changes {
        status: Some("open".into()),
        ..ch()
    };
    assert_eq!(
        update(&p, &id, open).unwrap_err().to_string(),
        "#1: status \"open\" is not one of idle, doing, done, wontfix"
    );
    // a board from before `idle` says how to convert it
    let f = d.path().join("items/1.md");
    let text = std::fs::read_to_string(&f)
        .unwrap()
        .replace("status: idle", "status: open");
    std::fs::write(&f, text).unwrap();
    let e = load(&p).unwrap_err().to_string();
    assert!(
        e.contains("items/1.md") && e.contains("run primum migrate"),
        "{e}"
    );
}
