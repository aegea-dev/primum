use primum::model::*;

fn item(kind: &str, num: u32, i: u8, u: u8, r: u8, e: u8) -> Item {
    let mut it = Item::new(kind, num, "Café");
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
    assert_eq!(ids, ["1", "3", "1", "2", "10"]);
    let kinds: Vec<_> = v.iter().map(|i| i.kind.as_str()).collect();
    assert_eq!(kinds, ["URGENT", "BUG", "BUG", "BUG", "TODO"]);
}

#[test]
fn blockers_are_open_needs_only() {
    let mut a = item("TODO", 1, 5, 3, 3, 5);
    a.needs = vec!["2".into(), "3".into(), "9".into()];
    let mut b2 = item("BUG", 2, 5, 3, 3, 5);
    b2.status = "done".into();
    let b3 = item("BUG", 3, 5, 3, 3, 5);
    assert_eq!(
        blockers(&a, &[a.clone(), b2, b3], |i| i.status == "done"),
        vec!["3".to_string(), "9".to_string()]
    );
}

#[test]
fn ids_are_numbers() {
    assert_eq!(id_of(12), "12");
    assert_eq!(parse_id("12"), Some(12));
    assert_eq!(parse_id(" #12 "), Some(12));
    for bad in ["0", "TODO:3", "12a", "#", "", "-1"] {
        assert_eq!(parse_id(bad), None, "{bad}");
    }
    assert_eq!(label("12"), "#12");
    let it = Item::new("BUG", 7, "Café menüsü");
    assert_eq!((it.id.as_str(), it.kind.as_str(), it.num), ("7", "BUG", 7));
}
