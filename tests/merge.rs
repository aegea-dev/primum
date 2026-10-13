use primum::merge::*;
use primum::model::Item;

fn base() -> Item {
    let mut it = Item::new("TODO", 88, "Özgür'ün listesi");
    it.needs = vec!["BUG:1".into(), "BUG:2".into()];
    it.body = "bir\niki\nüç\n".into();
    it
}

/// A stand-in for git merge-file: clean only when one side kept the base.
fn text(b: &str, o: &str, t: &str) -> Option<String> {
    if o == b {
        Some(t.into())
    } else if t == b {
        Some(o.into())
    } else {
        None
    }
}

#[test]
fn different_fields_merge_cleanly() {
    let b = base();
    let mut o = b.clone();
    o.status = "doing".into();
    let mut t = b.clone();
    t.impact = 9;
    t.title = "Işıl'ın listesi".into();
    let m = merge_item(Some(&b), &o, &t, &text);
    assert!(m.fields.is_empty() && m.body.is_none());
    assert_eq!(
        (m.item.status.as_str(), m.item.impact, m.item.title.as_str()),
        ("doing", 9, "Işıl'ın listesi")
    );
}

#[test]
fn the_same_change_on_both_sides_is_no_conflict() {
    let b = base();
    let mut o = b.clone();
    o.effort = 2;
    let t = o.clone();
    let m = merge_item(Some(&b), &o, &t, &text);
    assert!(m.fields.is_empty());
    assert_eq!(m.item.effort, 2);
}

#[test]
fn needs_merge_as_sets() {
    let b = base();
    let mut o = b.clone();
    o.needs = vec!["BUG:1".into(), "BUG:2".into(), "DEBT:3".into()];
    let mut t = b.clone();
    t.needs = vec!["BUG:2".into()];
    let m = merge_item(Some(&b), &o, &t, &text);
    assert_eq!(
        m.item.needs,
        vec!["BUG:2".to_string(), "DEBT:3".to_string()]
    );
}

#[test]
fn status_carries_finished_and_conflicts_keep_ours() {
    let b = base();
    let mut o = b.clone();
    o.status = "done".into();
    o.finished = Some("2026-10-14".into());
    let mut t = b.clone();
    t.status = "wontfix".into();
    t.finished = Some("2026-10-15".into());
    let m = merge_item(Some(&b), &o, &t, &text);
    let want = FieldConflict {
        field: "status".into(),
        values: ["done".into(), "wontfix".into()],
    };
    assert_eq!(m.fields, vec![want]);
    assert_eq!(
        (m.item.status.as_str(), m.item.finished.as_deref()),
        ("done", Some("2026-10-14"))
    );
    let mut t2 = b.clone();
    t2.status = "done".into();
    t2.finished = Some("2026-10-15".into());
    let mut o2 = b.clone();
    o2.risk = 7;
    let m = merge_item(Some(&b), &o2, &t2, &text);
    assert_eq!(
        m.item.finished.as_deref(),
        Some("2026-10-15"),
        "finished follows the status it came with"
    );
}

#[test]
fn bodies_merge_as_text_or_conflict() {
    let b = base();
    let mut o = b.clone();
    o.body = "bir\nİKİ\nüç\n".into();
    let t = b.clone();
    assert_eq!(
        merge_item(Some(&b), &o, &t, &text).item.body,
        "bir\nİKİ\nüç\n"
    );
    let mut t = b.clone();
    t.body = "bir\nikki\nüç\n".into();
    let m = merge_item(Some(&b), &o, &t, &text);
    assert_eq!(
        m.body,
        Some(["bir\nİKİ\nüç\n".to_string(), "bir\nikki\nüç\n".to_string()])
    );
    assert_eq!(m.item.body, "bir\nİKİ\nüç\n");
}

#[test]
fn without_a_base_differences_are_conflicts() {
    let o = base();
    let mut t = base();
    t.urgency = 8;
    let m = merge_item(None, &o, &t, &text);
    assert_eq!(m.fields[0].field, "urgency");
}
