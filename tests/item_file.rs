use primum::item_file::*;
use primum::model::Item;

fn sample() -> Item {
    let mut it = Item::new("TODO", 87, "Özgür'ün listesi: \"tırnak\" ve iki nokta");
    (it.impact, it.urgency, it.risk, it.effort) = (7, 6, 2, 4);
    it.status = "doing".into();
    it.needs = vec!["TODO:64".into(), "BUG:3".into()];
    it.docs = vec!["items/TODO-80.spec.md".into()];
    it.source = "owner, 2026-10-13".into();
    it.body = "## Neden\n\nIşıl yazdı.\n\n---\n\n## 2026-10-13\n\nnot\n".into();
    it
}

#[test]
fn write_then_parse_is_the_same_item() {
    let it = sample();
    assert_eq!(parse("TODO-87.md", &write(&it)).unwrap(), it);
}

#[test]
fn the_written_form() {
    let text = write(&sample());
    assert!(text.starts_with("---\ntitle: \"Özgür'ün listesi: \\\"tırnak\\\" ve iki nokta\"\nstatus: doing\nimpact: 7\nurgency: 6\nrisk: 2\neffort: 4\nneeds: [TODO:64, BUG:3]\n"));
    assert!(text.contains("\n---\n\n## Neden\n"));
    let mut bare = Item::new("BUG", 1, "x");
    bare.finished = Some("2026-10-13".into());
    let t = write(&bare);
    assert!(t.contains("needs: []\nfinished: 2026-10-13\n---\n"));
    assert!(!t.contains("docs:") && !t.contains("source:"));
}

#[test]
fn hand_written_plain_title_is_accepted() {
    let it = parse(
        "BUG-2.md",
        "---\ntitle: Gallery 404s\nstatus: open\nimpact: 5\nurgency: 3\nrisk: 3\neffort: 5\nneeds: []\n---\n",
    )
    .unwrap();
    assert_eq!(
        (it.id.as_str(), it.title.as_str(), it.body.as_str()),
        ("BUG:2", "Gallery 404s", "")
    );
}

#[test]
fn errors_name_the_line() {
    let e = parse("TODO-1.md", "---\ntitle: \"x\"\nstatus: bitti\n---\n").unwrap_err();
    assert_eq!(e.to_string(), "TODO-1.md:3: unknown status `bitti`");
    let e = parse(
        "TODO-1.md",
        "---\ntitle: \"x\"\nstatus: open\nimpact: 11\n---\n",
    )
    .unwrap_err();
    assert_eq!(e.to_string(), "TODO-1.md:4: impact must be 1-10, not 11");
    let e = parse("TODO-1.md", "---\ntitle: \"x\"\nrenk: mavi\n---\n").unwrap_err();
    assert_eq!(e.to_string(), "TODO-1.md:3: unknown field `renk`");
    let e = parse("TODO-1.md", "title: x\n").unwrap_err();
    assert_eq!(
        e.to_string(),
        "TODO-1.md:1: expected `---` to open the front matter"
    );
    let e = parse("TODO-1.md", "---\ntitle: \"x\"\n").unwrap_err();
    assert_eq!(
        e.to_string(),
        "TODO-1.md:3: front matter is not closed with `---`"
    );
    let e = parse("TODO-1.md", "---\nstatus: open\n---\n").unwrap_err();
    assert_eq!(e.to_string(), "TODO-1.md:1: front matter has no `title`");
    let e = parse("TODO-1.md", "---\ntitle: x\nneeds: TODO:2\n---\n").unwrap_err();
    assert_eq!(e.to_string(), "TODO-1.md:3: expected a list like [A, B]");
}

#[test]
fn names() {
    assert_eq!(file_name("KILLER:2"), "KILLER-2.md");
    assert_eq!(parse_item_name("KILLER-2.md"), Some(("KILLER".into(), 2)));
    assert_eq!(parse_item_name("KILLER-2.spec.md"), None);
    assert_eq!(
        parse_doc_name("TODO-64.spec.2026-10-06.md"),
        Some(("TODO".into(), 64, "spec".into()))
    );
    assert_eq!(parse_doc_name("TODO-64.md"), None);
    assert_eq!(parse_doc_name("notes.md"), None);
}
