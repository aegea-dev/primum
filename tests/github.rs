use primum::github::*;
use primum::model::Item;

fn item() -> Item {
    let mut it = Item::new("BUG", 4, "Gökçe'nin galerisi boş");
    (it.impact, it.urgency, it.risk, it.effort) = (8, 7, 3, 2);
    it.status = "doing".into();
    it.needs = vec!["TODO:1".into(), "DEBT:2".into()];
    it.body = "Işıl gördü.\n".into();
    it
}

fn issue(number: u64, w: &IssueWrite) -> Issue {
    Issue {
        number,
        title: w.title.clone(),
        body: w.body.clone(),
        open: w.open,
        reason: w.reason.clone(),
        labels: w.labels.clone(),
        updated: "2026-10-14T09:00:00Z".into(),
        closed: None,
        author: "isil".into(),
        is_pr: false,
        pr_merged: false,
    }
}

#[test]
fn an_item_as_an_issue() {
    let w = issue_write(&item(), &|id| (id == "TODO:1").then_some(45));
    assert_eq!(w.title, "Gökçe'nin galerisi boş");
    assert!(w.open && w.reason.is_none());
    assert_eq!(w.labels, ["kind:bug", "doing"]);
    assert_eq!(
        w.body,
        "Işıl gördü.\n\nNeeds: #45, DEBT:2\n\n<!-- primum\nid: BUG:4\nimpact: 8\nurgency: 7\nrisk: 3\neffort: 2\nneeds: [TODO:1, DEBT:2]\n-->\n"
    );
    let mut closed = item();
    closed.status = "wontfix".into();
    let w = issue_write(&closed, &|_| None);
    assert_eq!((w.open, w.reason.as_deref()), (false, Some("not_planned")));
    assert_eq!(w.labels, ["kind:bug"]);
}

#[test]
fn an_issue_as_an_item() {
    let w = issue_write(&item(), &|_| None);
    let back = item_from_issue(&issue(9, &w), &Item::new("TODO", 1, ""));
    let mut want = item();
    want.github = Some(9);
    assert_eq!(back, want);
    let mut closed = issue(9, &w);
    closed.open = false;
    closed.reason = Some("completed".into());
    closed.closed = Some("2026-10-15T10:00:00Z".into());
    closed.labels = vec!["kind:killer".into()];
    let b = item_from_issue(&closed, &Item::new("TODO", 1, ""));
    assert_eq!(
        (b.status.as_str(), b.finished.as_deref(), b.kind.as_str()),
        ("done", Some("2026-10-15"), "KILLER")
    );
}

#[test]
fn a_plain_issue_and_a_broken_block() {
    let plain = IssueWrite {
        title: "Özgür'ün isteği".into(),
        body: "Sadece metin.".into(),
        open: true,
        reason: None,
        labels: vec![],
    };
    let it = item_from_issue(&issue(3, &plain), &Item::new("TODO", 7, ""));
    assert_eq!(
        (it.kind.as_str(), it.impact, it.body.as_str()),
        ("TODO", 5, "Sadece metin.\n")
    );
    let (text, meta) = split_block("metin\n\n<!-- primum\nid: TODO:x\n-->\n");
    assert_eq!((text.as_str(), meta), ("metin\n", None));
}

#[test]
fn memory_is_a_small_github() {
    let m = Memory::new();
    let i = m.create(&issue_write(&item(), &|_| None)).unwrap();
    assert_eq!(i.number, 1);
    m.edit(1, |i| i.title = "yeni".into());
    let pr = m.open_pr("Galeri düzeltmesi", "Fixes BUG:4", true);
    let all = m.issues_since(None).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|x| x.number == pr && x.is_pr && x.pr_merged));
    let later = m.issues_since(Some(&i.updated)).unwrap();
    assert!(later.iter().all(|x| x.updated > i.updated));
    m.remove(1);
    assert!(m.update(1, &issue_write(&item(), &|_| None)).is_err());
}

#[test]
fn ids_in_the_title_and_as_a_label() {
    let w = issue_write_as(&item(), &|_| None, IdStyle::Both);
    assert_eq!(w.title, "[BUG:4] Gökçe'nin galerisi boş");
    assert_eq!(w.labels, ["kind:bug", "BUG:4", "doing"]);
    let back = item_from_issue(&issue(9, &w), &Item::new("TODO", 1, ""));
    assert_eq!(
        back.title, "Gökçe'nin galerisi boş",
        "the prefix is not part of the title"
    );
    let only_title = issue_write_as(&item(), &|_| None, IdStyle::Title);
    assert_eq!(only_title.labels, ["kind:bug", "doing"]);
    let mut odd = issue(9, &only_title);
    odd.title = "[not an id] stays".into();
    let kept = item_from_issue(&odd, &Item::new("TODO", 1, ""));
    assert_eq!(kept.title, "[not an id] stays");
    assert_eq!(IdStyle::parse("label"), Some(IdStyle::Label));
    assert_eq!(IdStyle::parse("rozet"), None);
}
