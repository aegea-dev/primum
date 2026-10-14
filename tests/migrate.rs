mod common;
use primum::migrate::{migrate, rewrite_refs};
use std::collections::BTreeMap;
use std::fs;

fn old_item(title: &str, needs: &str, body: &str) -> String {
    format!(
        "---\ntitle: \"{title}\"\nstatus: open\nimpact: 5\nurgency: 3\nrisk: 3\neffort: 5\nneeds: [{needs}]\n---\n\n{body}"
    )
}

#[test]
fn references_are_rewritten_in_text() {
    let map: BTreeMap<String, String> =
        [("TODO:7".into(), "3".into()), ("BUG:1".into(), "1".into())].into();
    assert_eq!(
        rewrite_refs(
            "see TODO:7 and BUG:1, not TODO:70; items/TODO-7.spec.md",
            &map
        ),
        "see #3 and #1, not TODO:70; items/3.spec.md"
    );
    let renumber: BTreeMap<String, String> = [("4".into(), "9".into())].into();
    assert_eq!(
        rewrite_refs("#4, #40, a&#4; and items/4.plan.md", &renumber),
        "#9, #40, a&#4; and items/9.plan.md"
    );
}

#[test]
fn a_board_is_numbered_once_in_creation_order() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let h = common::history(&p);
    let items = d.path().join("items");
    // created in this order, across lanes
    fs::write(
        items.join("TODO-7.md"),
        old_item("Çay", "", "Needs BUG:1 first.\n"),
    )
    .unwrap();
    h.commit("first").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(items.join("BUG-1.md"), old_item("Şeker", "", "")).unwrap();
    fs::write(items.join("BUG-1.spec.md"), "# Şeker spec\nTODO:7\n").unwrap();
    h.commit("second").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(
        items.join("KILLER-2.md"),
        old_item("Gölge", "TODO:7, BUG:1", ""),
    )
    .unwrap();
    h.commit("third").unwrap();

    let moved = migrate(&p).unwrap();
    let pairs: Vec<(&str, &str)> = moved
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_eq!(pairs, [("TODO:7", "1"), ("BUG:1", "2"), ("KILLER:2", "3")]);
    let one = fs::read_to_string(items.join("1.md")).unwrap();
    assert!(
        one.contains("kind: TODO\n") && one.contains("Needs #2 first."),
        "{one}"
    );
    let three = fs::read_to_string(items.join("3.md")).unwrap();
    assert!(
        three.contains("kind: KILLER\n") && three.contains("needs: [1, 2]\n"),
        "{three}"
    );
    assert_eq!(
        fs::read_to_string(items.join("2.spec.md")).unwrap(),
        "# Şeker spec\n#1\n"
    );
    assert!(!items.join("TODO-7.md").exists());
    assert_eq!(common::subjects(&p)[0], "number every item");
    assert!(primum::store::load(&p).is_ok());
    assert!(migrate(&p).unwrap().is_empty(), "twice changes nothing");
}

#[test]
fn conflict_records_follow_their_items() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let items = d.path().join("items");
    fs::write(items.join("BUG-4.md"), old_item("Tuz", "", "")).unwrap();
    fs::create_dir_all(d.path().join("conflicts")).unwrap();
    fs::write(
        d.path().join("conflicts/BUG-4.json"),
        r#"{"id":"BUG:4","sides":[{"author":"Ada","date":""},{"author":"Can","date":""}],"documents":[{"path":"items/BUG-4.spec.md","other":"items/BUG-4.spec.abc.md"}]}"#,
    )
    .unwrap();
    migrate(&p).unwrap();
    let c = primum::store::load(&p).unwrap().conflicts["1"].clone();
    assert_eq!(c.id, "1");
    assert_eq!(c.documents[0].path, "items/1.spec.md");
    assert_eq!(c.documents[0].other, "items/1.spec.abc.md");
    assert!(!d.path().join("conflicts/BUG-4.json").exists());
}

#[test]
fn documents_of_items_not_on_the_board_get_numbers_too() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let items = d.path().join("items");
    // KILLER:36 was left behind (closed); its spec is still shown by BUG:2
    fs::write(
        items.join("BUG-2.md"),
        old_item("Çay", "", "See KILLER:36.\n"),
    )
    .unwrap();
    let shown = fs::read_to_string(items.join("BUG-2.md")).unwrap().replace(
        "needs: []\n",
        "needs: []\ndocs: [items/KILLER-36.spec.md]\n",
    );
    fs::write(items.join("BUG-2.md"), shown).unwrap();
    fs::write(items.join("KILLER-36.spec.md"), "# Gölge\n").unwrap();
    let moved = migrate(&p).unwrap();
    assert_eq!(moved, [("BUG:2".to_string(), "1".to_string())]);
    assert_eq!(
        fs::read_to_string(items.join("2.spec.md")).unwrap(),
        "# Gölge\n"
    );
    let one = fs::read_to_string(items.join("1.md")).unwrap();
    assert!(
        one.contains("docs: [items/2.spec.md]") && one.contains("See #2."),
        "{one}"
    );
    let b = primum::store::load(&p).unwrap();
    assert_eq!(b.docs["1"][0].path, "items/2.spec.md");
    // a number a document holds is taken: a new item never inherits it
    let id = primum::store::insert(&p, "TODO", "Yeni", Default::default()).unwrap();
    assert_eq!(id, "3");
}

#[test]
fn a_failed_migration_changes_nothing() {
    let d = tempfile::tempdir().unwrap();
    let p = common::project(d.path());
    let items = d.path().join("items");
    fs::write(items.join("TODO-1.md"), old_item("Çay", "", "")).unwrap();
    fs::write(items.join("TODO-2.md"), "not front matter\n").unwrap();
    fs::write(items.join("TODO-1.spec.md"), "# Çay\n").unwrap();
    assert!(migrate(&p).is_err());
    let mut names: Vec<String> = fs::read_dir(&items)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["TODO-1.md", "TODO-1.spec.md", "TODO-2.md"]);
}
