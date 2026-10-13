use primum::model::*;

fn item(kind: &str, num: u32, i: u8, u: u8, r: u8, e: u8) -> Item {
    let mut it = Item::new(kind, num, "Gökçe");
    (it.impact, it.urgency, it.risk, it.effort) = (i, u, r, e);
    it
}

#[test]
fn score_is_the_boards_formula() {
    assert_eq!(item("URGENT", 3, 8, 7, 8, 4).score(), 57.5);
    assert_eq!(item("TODO", 1, 7, 6, 2, 4).score(), 37.5);
    assert_eq!(item("TODO", 2, 5, 3, 3, 3).score(), 36.7);
    // Python's round() is half-to-even: 130 / 8 = 16.25 gives 16.2.
    assert_eq!(item("TODO", 3, 5, 5, 3, 8).score(), 16.2);
}

#[test]
fn lane_beats_score_then_score_then_ties() {
    let mut v = [
        item("TODO", 10, 10, 10, 10, 1),
        item("BUG", 2, 2, 2, 2, 9),
        item("BUG", 1, 2, 2, 2, 9),
        item("URGENT", 1, 1, 1, 1, 10),
        item("BUG", 3, 9, 9, 9, 3),
    ];
    v.sort_by(priority_cmp);
    let ids: Vec<_> = v.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["URGENT:1", "BUG:3", "BUG:1", "BUG:2", "TODO:10"]);
}

#[test]
fn blockers_are_open_needs_only() {
    let mut a = item("TODO", 1, 5, 3, 3, 5);
    a.needs = vec!["BUG:1".into(), "BUG:2".into(), "BUG:9".into()];
    let mut b1 = item("BUG", 1, 5, 3, 3, 5);
    b1.status = "done".into();
    let b2 = item("BUG", 2, 5, 3, 3, 5);
    assert_eq!(
        blockers(&a, &[a.clone(), b1, b2]),
        vec!["BUG:2".to_string(), "BUG:9".to_string()]
    );
}

#[test]
fn ids() {
    assert_eq!(parse_id("KILLER:12"), Some(("KILLER".into(), 12)));
    assert_eq!(parse_id("IDEA:1"), None);
    assert_eq!(parse_id("TODO:x"), None);
}
