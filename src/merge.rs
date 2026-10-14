//! The three-way merge of one item, field by field, against the common
//! ancestor. Git's line merge cannot be used on item files: two people
//! changing `status` and `impact`, adjacent lines, is a git conflict. Pure:
//! the text merge of bodies is passed in.
use crate::model::Item;

#[derive(Debug, Clone, PartialEq)]
pub struct FieldConflict {
    pub field: String,
    /// This side's value, then the other side's.
    pub values: [String; 2],
}

#[derive(Debug, Clone)]
pub struct Merged {
    /// The merged item; a conflicting field holds this side's value.
    pub item: Item,
    pub fields: Vec<FieldConflict>,
    /// This side's body and the other side's, when they could not merge.
    pub body: Option<[String; 2]>,
}

enum Pick<T> {
    Clean(T),
    Conflict(T, T),
}

/// One side's change wins; the same change on both is clean; different
/// changes on both are a conflict.
fn pick<T: PartialEq + Clone>(base: Option<&T>, ours: &T, theirs: &T) -> Pick<T> {
    if ours == theirs || base == Some(theirs) {
        Pick::Clean(ours.clone())
    } else if base == Some(ours) {
        Pick::Clean(theirs.clone())
    } else {
        Pick::Conflict(ours.clone(), theirs.clone())
    }
}

/// Both sides' additions and removals applied to the base.
fn merge_set(base: Option<&Vec<String>>, ours: &[String], theirs: &[String]) -> Vec<String> {
    let empty = vec![];
    let base = base.unwrap_or(&empty);
    let removed = |x: &String| base.contains(x) && (!ours.contains(x) || !theirs.contains(x));
    let mut out: Vec<String> = vec![];
    for x in ours.iter().chain(theirs) {
        if !removed(x) && !out.contains(x) {
            out.push(x.clone());
        }
    }
    out
}

pub fn merge_item(
    base: Option<&Item>,
    ours: &Item,
    theirs: &Item,
    merge_text: &dyn Fn(&str, &str, &str) -> Option<String>,
) -> Merged {
    let mut item = ours.clone();
    let mut fields = vec![];
    let mut conflict = |name: &str, o: String, t: String| {
        fields.push(FieldConflict {
            field: name.into(),
            values: [o, t],
        });
    };

    match pick(base.map(|b| &b.title), &ours.title, &theirs.title) {
        Pick::Clean(v) => item.title = v,
        Pick::Conflict(o, t) => conflict("title", o, t),
    }
    match pick(base.map(|b| &b.kind), &ours.kind, &theirs.kind) {
        Pick::Clean(v) => item.kind = v,
        Pick::Conflict(o, t) => conflict("kind", o, t),
    }
    // a ticket's lifecycle and its step move together
    let pin = |i: &Item| (i.lifecycle.clone(), i.step.clone());
    match pick(base.map(pin).as_ref(), &pin(ours), &pin(theirs)) {
        Pick::Clean((l, s)) => (item.lifecycle, item.step) = (l, s),
        Pick::Conflict(o, t) => {
            let show = |(l, s): (Option<String>, Option<String>)| match (l, s) {
                (Some(l), Some(s)) => format!("{l} {s}"),
                _ => "manual".to_string(),
            };
            conflict("lifecycle", show(o), show(t))
        }
    }
    match pick(base.map(|b| &b.source), &ours.source, &theirs.source) {
        Pick::Clean(v) => item.source = v,
        Pick::Conflict(o, t) => conflict("source", o, t),
    }
    // finished goes with the status it was stamped with
    match pick(base.map(|b| &b.status), &ours.status, &theirs.status) {
        Pick::Clean(v) => {
            let theirs_won = v == theirs.status && v != ours.status;
            item.finished = if theirs_won {
                theirs.finished.clone()
            } else {
                ours.finished.clone()
            };
            item.status = v;
        }
        Pick::Conflict(o, t) => conflict("status", o, t),
    }
    let scores: [(&str, fn(&Item) -> u8); 4] = [
        ("impact", |i| i.impact),
        ("urgency", |i| i.urgency),
        ("risk", |i| i.risk),
        ("effort", |i| i.effort),
    ];
    for (name, get) in scores {
        match pick(base.map(get).as_ref(), &get(ours), &get(theirs)) {
            Pick::Clean(v) => match name {
                "impact" => item.impact = v,
                "urgency" => item.urgency = v,
                "risk" => item.risk = v,
                _ => item.effort = v,
            },
            Pick::Conflict(o, t) => conflict(name, o.to_string(), t.to_string()),
        }
    }
    item.needs = merge_set(base.map(|b| &b.needs), &ours.needs, &theirs.needs);
    item.docs = merge_set(base.map(|b| &b.docs), &ours.docs, &theirs.docs);

    let mut body = None;
    match pick(base.map(|b| &b.body), &ours.body, &theirs.body) {
        Pick::Clean(v) => item.body = v,
        Pick::Conflict(o, t) => {
            let b = base.map(|b| b.body.as_str()).unwrap_or("");
            match merge_text(b, &o, &t) {
                Some(merged) => item.body = merged,
                None => body = Some([o, t]),
            }
        }
    }
    Merged { item, fields, body }
}
