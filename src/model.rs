//! Items, lanes and the board's priority: the same formula and order as the
//! page, so the terminal and the browser always agree.
use serde::Serialize;
use std::cmp::Ordering;

pub const KINDS: [&str; 5] = ["URGENT", "BUG", "DEBT", "KILLER", "TODO"];
pub const STATUSES: [&str; 4] = ["open", "doing", "done", "wontfix"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub id: String,
    pub kind: String,
    pub num: u32,
    pub title: String,
    pub status: String,
    pub impact: u8,
    pub urgency: u8,
    pub risk: u8,
    pub effort: u8,
    pub needs: Vec<String>,
    /// Documents of other items this item also shows, as project-relative paths.
    pub docs: Vec<String>,
    pub finished: Option<String>,
    pub source: String,
    /// The GitHub issue this item is kept in step with.
    pub github: Option<u64>,
    pub body: String,
}

pub fn id_of(kind: &str, num: u32) -> String {
    format!("{kind}:{num}")
}

pub fn parse_id(id: &str) -> Option<(String, u32)> {
    let (kind, num) = id.split_once(':')?;
    if !KINDS.contains(&kind) || num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((kind.to_string(), num.parse().ok()?))
}

impl Item {
    pub fn new(kind: &str, num: u32, title: &str) -> Item {
        Item {
            id: id_of(kind, num),
            kind: kind.to_string(),
            num,
            title: title.to_string(),
            status: "open".into(),
            impact: 5,
            urgency: 3,
            risk: 3,
            effort: 5,
            needs: vec![],
            docs: vec![],
            finished: None,
            source: String::new(),
            github: None,
            body: String::new(),
        }
    }

    /// `(impact + urgency + risk) * 10 / effort`, to one decimal. Rounds half
    /// to even, as Python's `round` does, so 16.25 is 16.2 here as on the
    /// old board.
    pub fn score(&self) -> f64 {
        let raw = (self.impact as f64 + self.urgency as f64 + self.risk as f64) * 10.0
            / self.effort.max(1) as f64;
        (raw * 10.0).round_ties_even() / 10.0
    }

    pub fn is_closed(&self) -> bool {
        self.status == "done" || self.status == "wontfix"
    }
}

pub fn kind_rank(kind: &str) -> usize {
    KINDS.iter().position(|k| *k == kind).unwrap_or(KINDS.len())
}

pub fn clamp_score(n: i64) -> u8 {
    n.clamp(1, 10) as u8
}

/// Lane, then score (highest first), then impact, urgency, risk (highest
/// first), effort (lowest first), then number: `board.py`'s `priority_order`.
pub fn priority_cmp(a: &Item, b: &Item) -> Ordering {
    kind_rank(&a.kind)
        .cmp(&kind_rank(&b.kind))
        .then(b.score().total_cmp(&a.score()))
        .then(b.impact.cmp(&a.impact))
        .then(b.urgency.cmp(&a.urgency))
        .then(b.risk.cmp(&a.risk))
        .then(a.effort.cmp(&b.effort))
        .then(a.num.cmp(&b.num))
}

/// The ids in `needs` that still block: everything but closed items. An id
/// naming no item still blocks, as on the old board.
pub fn blockers(item: &Item, all: &[Item]) -> Vec<String> {
    item.needs
        .iter()
        .filter(|n| !all.iter().any(|i| &i.id == *n && i.is_closed()))
        .cloned()
        .collect()
}
