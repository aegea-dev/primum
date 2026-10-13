mod common;
use primum::store::*;

fn ch() -> Changes {
    Changes::default()
}

#[test]
fn insert_takes_the_next_number_per_kind_and_writes_board_md() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    assert_eq!(insert(&p, "TODO", "Özgür", ch()).unwrap(), "TODO:1");
    assert_eq!(insert(&p, "TODO", "Işıl", ch()).unwrap(), "TODO:2");
    assert_eq!(insert(&p, "BUG", "Gökçe", ch()).unwrap(), "BUG:1");
    assert!(d.path().join("items/TODO-2.md").is_file());
    assert!(common::read(d.path(), "BOARD.md").contains("TODO:2"));
    assert!(insert(&p, "IDEA", "x", ch()).is_err());
}

#[test]
fn status_change_stamps_and_clears_finished_and_frees_dependents() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "BUG", "hata", ch()).unwrap();
    let needs = Changes {
        needs: Some(vec!["BUG:1".into()]),
        ..ch()
    };
    insert(&p, "TODO", "bekler", needs).unwrap();
    let done = Changes {
        status: Some("done".into()),
        ..ch()
    };
    update(&p, "BUG:1", done).unwrap();
    let b = load(&p).unwrap();
    assert!(b.get("BUG:1").unwrap().finished.is_some());
    assert!(
        b.get("TODO:1").unwrap().needs.is_empty(),
        "a closed id leaves every needs"
    );
    let open = Changes {
        status: Some("open".into()),
        ..ch()
    };
    update(&p, "BUG:1", open).unwrap();
    assert!(load(&p).unwrap().get("BUG:1").unwrap().finished.is_none());
    let bad = Changes {
        status: Some("bitti".into()),
        ..ch()
    };
    assert_eq!(
        update(&p, "BUG:1", bad).unwrap_err().to_string(),
        "unknown status bitti"
    );
    assert_eq!(
        update(&p, "BUG:9", ch()).unwrap_err().to_string(),
        "no item BUG:9"
    );
}

#[test]
fn kind_change_renames_files_rewrites_heading_needs_and_docs() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "BUG", "eski", ch()).unwrap();
    let body = Changes {
        body: Some("## BUG:2 taşınacak\n\nmetin\n".into()),
        ..ch()
    };
    insert(&p, "BUG", "taşınacak", body).unwrap();
    let needs = Changes {
        needs: Some(vec!["BUG:2".into()]),
        ..ch()
    };
    insert(&p, "TODO", "bekler", needs).unwrap();
    set_doc(&p, "BUG:2", "spec", "# Spec\n").unwrap();
    let shared = common::read(d.path(), "items/TODO-1.md").replace(
        "needs: [BUG:2]\n",
        "needs: [BUG:2]\ndocs: [items/BUG-2.spec.md]\n",
    );
    std::fs::write(d.path().join("items/TODO-1.md"), shared).unwrap();

    assert_eq!(set_kind(&p, "BUG:2", "URGENT").unwrap(), "URGENT:1");
    assert!(d.path().join("items/URGENT-1.md").is_file());
    assert!(d.path().join("items/URGENT-1.spec.md").is_file());
    assert!(!d.path().join("items/BUG-2.md").exists());
    assert!(!d.path().join("items/BUG-2.spec.md").exists());
    let b = load(&p).unwrap();
    assert!(
        b.get("URGENT:1")
            .unwrap()
            .body
            .starts_with("## URGENT:1 taşınacak")
    );
    let todo = b.get("TODO:1").unwrap();
    assert_eq!(todo.needs, vec!["URGENT:1".to_string()]);
    assert_eq!(todo.docs, vec!["items/URGENT-1.spec.md".to_string()]);

    let done = Changes {
        status: Some("done".into()),
        ..ch()
    };
    update(&p, "BUG:1", done).unwrap();
    assert_eq!(
        set_kind(&p, "BUG:1", "TODO").unwrap_err().to_string(),
        "BUG:1 is closed; reopen it before changing kind"
    );
}

#[test]
fn heading_rewrite_follows_board_py() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    for body in [
        "### 1. Numaralı başlık\n",
        "BUG:1 başlık değil\n",
        "## BUG:10 başka\n",
    ] {
        let c = Changes {
            body: Some(body.into()),
            ..ch()
        };
        let id = insert(&p, "BUG", "x", c).unwrap();
        let new = set_kind(&p, &id, "DEBT").unwrap();
        let got = load(&p).unwrap().get(&new).unwrap().body.clone();
        match body {
            "### 1. Numaralı başlık\n" => assert_eq!(got, "### DEBT:1 Numaralı başlık\n"),
            _ => assert_eq!(got, body),
        }
    }
}

#[test]
fn documents_beside_and_shared() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "a", ch()).unwrap();
    insert(&p, "TODO", "b", ch()).unwrap();
    set_doc(&p, "TODO:1", "spec", "# S\n").unwrap();
    set_doc(&p, "TODO:1", "plan", "# P\n").unwrap();
    assert!(set_doc(&p, "TODO:1", "Spec Dosyası", "x").is_err());
    let shared = common::read(d.path(), "items/TODO-2.md")
        .replace("needs: []\n", "needs: []\ndocs: [items/TODO-1.spec.md]\n");
    std::fs::write(d.path().join("items/TODO-2.md"), shared).unwrap();
    let b = load(&p).unwrap();
    let kinds = |id: &str| {
        b.docs[id]
            .iter()
            .map(|x| x.kind.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds("TODO:1"), ["spec", "plan"]);
    assert_eq!(kinds("TODO:2"), ["spec"]);
    assert_eq!(
        read_doc(&p, "items/TODO-1.plan.md").as_deref(),
        Some("# P\n")
    );
    for bad in [
        "items/TODO-1.md",
        "primum.toml",
        "../x.md",
        "/etc/passwd",
        "items/../BOARD.md",
        "items/TODO-9.spec.md",
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
        needs: Some(vec!["BUG:1".into()]),
        ..ch()
    };
    insert(&p, "TODO", "bekler", needs).unwrap();
    set_doc(&p, "BUG:1", "report", "# R\n").unwrap();
    let gone = delete(&p, "BUG:1").unwrap();
    assert_eq!(gone, ["items/BUG-1.md", "items/BUG-1.report.md"]);
    assert!(!d.path().join("items/BUG-1.report.md").exists());
    assert!(load(&p).unwrap().get("TODO:1").unwrap().needs.is_empty());
    assert!(delete(&p, "BUG:1").is_err());
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
    append_note(&p, "TODO:1", "Ölçüldü.\n").unwrap();
    let body = load(&p).unwrap().get("TODO:1").unwrap().body.clone();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert_eq!(body, format!("İlk.\n\n---\n\n## {today}\n\nÖlçüldü.\n"));
}

#[test]
fn a_malformed_file_stops_the_load_with_its_name() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    std::fs::write(d.path().join("items/TODO-1.md"), "title: x\n").unwrap();
    assert!(load(&p).unwrap_err().to_string().contains("TODO-1.md:1"));
}

#[test]
fn board_md_lists_open_by_lane_and_closed() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Gökçe'nin işi", ch()).unwrap();
    insert(&p, "BUG", "Işıl'ın hatası", ch()).unwrap();
    let done = Changes {
        status: Some("wontfix".into()),
        ..ch()
    };
    update(&p, "TODO:1", done).unwrap();
    let md = common::read(d.path(), "BOARD.md");
    assert!(md.starts_with("# deneme\n"));
    assert!(md.contains("1 open, 1 closed, 2 total."));
    assert!(md.contains("### Bugs (1)"));
    assert!(
        md.contains("| [BUG:1](items/BUG-1.md) | 5 | 3 | 3 | 5 | 22.0 | open | Işıl'ın hatası |")
    );
    assert!(md.contains("## Closed"));
    assert!(!md.contains("### TODOs"));
}

#[test]
fn every_change_is_one_commit_with_its_message() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Özgür'ün listesi", ch()).unwrap();
    let doing = Changes {
        status: Some("doing".into()),
        ..ch()
    };
    update(&p, "TODO:1", doing).unwrap();
    let scores = Changes {
        impact: Some(7),
        effort: Some(3),
        ..ch()
    };
    update(&p, "TODO:1", scores).unwrap();
    append_note(&p, "TODO:1", "Ölçüldü.").unwrap();
    set_doc(&p, "TODO:1", "spec", "# S\n").unwrap();
    set_kind(&p, "TODO:1", "BUG").unwrap();
    delete(&p, "BUG:1").unwrap();
    assert_eq!(
        common::subjects(&p),
        [
            "BUG:1: delete",
            "TODO:1 -> BUG:1",
            "TODO:1: spec document",
            "TODO:1: note",
            "TODO:1: impact, effort",
            "TODO:1: status doing",
            "TODO:1: add \"Özgür'ün listesi\"",
        ],
        "an empty project has nothing for a first commit"
    );
}

#[test]
fn an_outside_edit_is_committed_before_the_next_change() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "x", ch()).unwrap();
    let f = d.path().join("items/TODO-1.md");
    let edited = std::fs::read_to_string(&f)
        .unwrap()
        .replace("risk: 3", "risk: 9");
    std::fs::write(&f, edited).unwrap();
    let done = Changes {
        status: Some("done".into()),
        ..ch()
    };
    update(&p, "TODO:1", done).unwrap();
    assert_eq!(
        common::subjects(&p)[..2],
        ["TODO:1: status done", "edited outside primum"]
    );
}

#[test]
fn a_failed_change_commits_nothing() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "x", ch()).unwrap();
    assert!(update(&p, "TODO:9", ch()).is_err());
    assert_eq!(common::subjects(&p).len(), 1);
}

#[test]
fn conflicts_show_on_the_board_and_follow_their_item() {
    use primum::conflicts::*;
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    insert(&p, "TODO", "Özgür", ch()).unwrap();
    let side = |a: &str| Side {
        author: a.into(),
        date: "2026-10-14T09:00:00+01:00".into(),
    };
    let c = Conflict {
        id: "TODO:1".into(),
        sides: [side("Özgür"), side("Işıl")],
        fields: vec![FieldValues {
            field: "status".into(),
            values: ["done".into(), "wontfix".into()],
        }],
        body: None,
        documents: vec![],
    };
    write(&p, &c).unwrap();
    assert!(d.path().join("conflicts/TODO-1.json").is_file());
    let b = load(&p).unwrap();
    assert_eq!(b.conflicts["TODO:1"].fields[0].values[1], "wontfix");
    let risk = Changes {
        risk: Some(4),
        ..ch()
    };
    update(&p, "TODO:1", risk).unwrap();
    let md = common::read(d.path(), "BOARD.md");
    assert!(
        md.contains("## Conflicts\n\n- [TODO:1](items/TODO-1.md): status"),
        "{md}"
    );
    set_kind(&p, "TODO:1", "BUG").unwrap();
    assert!(d.path().join("conflicts/BUG-1.json").is_file());
    assert_eq!(load(&p).unwrap().conflicts["BUG:1"].id, "BUG:1");
    delete(&p, "BUG:1").unwrap();
    assert!(!d.path().join("conflicts/BUG-1.json").exists());
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
    set_doc(&p, "TODO:1", "spec", "# bizim\n").unwrap();
    let other = d.path().join("items/TODO-1.spec.abc1234.md");
    std::fs::write(&other, "# onların\n").unwrap();
    let side = |a: &str| Side {
        author: a.into(),
        date: String::new(),
    };
    let c = Conflict {
        id: "TODO:1".into(),
        sides: [side("Özgür"), side("Işıl")],
        fields: vec![FieldValues {
            field: "impact".into(),
            values: ["5".into(), "8".into()],
        }],
        body: Some(BodyValues {
            values: ["bizim\n".into(), "onların\n".into()],
        }),
        documents: vec![DocConflict {
            path: "items/TODO-1.spec.md".into(),
            other: "items/TODO-1.spec.abc1234.md".into(),
        }],
    };
    write(&p, &c).unwrap();
    resolve(&p, "TODO:1", "impact", Keep::Second).unwrap();
    assert_eq!(load(&p).unwrap().get("TODO:1").unwrap().impact, 8);
    assert!(resolve(&p, "TODO:1", "body", Keep::Both).is_err());
    resolve(&p, "TODO:1", "body", Keep::First).unwrap();
    resolve(&p, "TODO:1", "items/TODO-1.spec.md", Keep::Second).unwrap();
    assert_eq!(
        common::read(d.path(), "items/TODO-1.spec.md"),
        "# onların\n"
    );
    assert!(!other.exists());
    assert!(
        !d.path().join("conflicts/TODO-1.json").exists(),
        "empty record removed"
    );
    assert_eq!(
        common::subjects(&p)[0],
        "TODO:1: resolve items/TODO-1.spec.md"
    );
    assert!(resolve(&p, "TODO:1", "impact", Keep::First).is_err());
}
