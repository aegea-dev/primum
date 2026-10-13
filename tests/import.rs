mod common;
use primum::import;
use primum::store::load;
use std::path::{Path, PathBuf};

/// Copies the fixture board to a temp dir, so a test can check that the
/// import left it untouched.
fn board(into: &Path) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/board");
    let dst = into.join("board");
    copy_dir(&src, &dst);
    dst
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(snapshot(&p));
        } else {
            out.push((p.clone(), std::fs::read(&p).unwrap()));
        }
    }
    out.sort();
    out
}

#[test]
fn imports_items_and_their_documents() {
    let d = tempfile::tempdir().unwrap();
    let src = board(d.path());
    let before = snapshot(&src);
    let p = common::project(&d.path().join("proje"));
    let r = import::run(&src, &p).unwrap();
    assert_eq!(r.items, 5);

    let b = load(&p).unwrap();
    let bug = b.get("BUG:1").unwrap();
    assert_eq!(bug.title, "Gökçe'nin galerisi boş açılıyor");
    assert_eq!(
        (bug.impact, bug.urgency, bug.risk, bug.effort),
        (7, 6, 4, 2)
    );
    assert_eq!(
        bug.body,
        "## BUG:1 Gökçe'nin galerisi boş açılıyor\n\nIşıl gördü.\n"
    );
    assert_eq!(bug.source, "board");
    assert_eq!(bug.docs, vec!["items/TODO-1.spec.md".to_string()]);

    let t1 = b.get("TODO:1").unwrap();
    assert_eq!(t1.status, "doing");
    assert_eq!(
        t1.needs,
        vec!["BUG:1".to_string()],
        "the closed BUG:2 is cleared"
    );
    assert!(t1.body.ends_with("## 2026-10-07\n\nBaşladı.\n"));

    let t2 = b.get("TODO:2").unwrap();
    assert_eq!((t2.impact, t2.effort), (8, 2), "crit and cplx back-fill");
    assert_eq!(t2.body, "Tek satır gövde\n");

    let t3 = b.get("TODO:3").unwrap();
    assert_eq!(
        (t3.status.as_str(), t3.finished.as_deref()),
        ("wontfix", Some("2026-10-01"))
    );

    let items = p.items_dir();
    let copy = |n: &str| std::fs::read_to_string(items.join(n)).unwrap();
    assert!(copy("TODO-2.spec.md").contains("**Ticket:** TODO:2 (foundation for BUG:1)"));
    assert!(copy("TODO-2.plan.md").starts_with("# Galeri planı"));
    assert!(copy("TODO-1.spec.md").starts_with("# Ortak"));
    assert!(copy("TODO-1.spec.2026-10-05.md").starts_with("# Gövde"));
    assert!(copy("TODO-3.spec.md").starts_with("# Arama\n"));
    assert!(copy("TODO-3.spec.2026-10-06.md").starts_with("# Arama ek"));
    let kinds = |id: &str| {
        b.docs[id]
            .iter()
            .map(|x| x.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds("BUG:1"), ["TODO-1.spec.md"], "not linked to galeri");
    assert_eq!(kinds("TODO:2"), ["TODO-2.spec.md", "TODO-2.plan.md"]);

    assert_eq!(
        r.left,
        [
            "superpowers/specs/2026-10-04-sahipsiz-design.md",
            "superpowers/specs/2026-10-08-hayalet-design.md"
        ]
    );
    assert!(!items.join("TODO-99.spec.md").exists());
    assert_eq!(r.docs.len(), 6);
    assert_eq!(
        r.shared,
        [("BUG:1".to_string(), "items/TODO-1.spec.md".to_string())]
    );
    assert!(common::read(&p.root, "BOARD.md").contains("3 open, 2 closed, 5 total."));

    assert_eq!(snapshot(&src), before, "the source board is untouched");
    let again = import::run(&src, &p).unwrap_err().to_string();
    assert!(again.contains("already has items"), "{again}");
}

#[test]
fn a_board_without_items_json_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(&d.path().join("proje"));
    let e = import::run(d.path(), &p).unwrap_err().to_string();
    assert!(e.contains("data/items.json"), "{e}");
}
