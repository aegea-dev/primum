//! The checks a lifecycle file passes before anything can follow it. The
//! first problem found is reported, naming the file and the step.
use super::format::{File, Next, StepDef};
use super::{LifecycleError, actionables, expr};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

/// The states a lifecycle has when it declares none.
pub const DEFAULT_STATES: [&str; 6] = [
    "idle",
    "waiting",
    "processing",
    "effects",
    "done",
    "wontfix",
];

/// Every step a step can go to: next, branches, fallbacks, a recommendation.
pub fn targets(s: &StepDef) -> Vec<&str> {
    let mut out = vec![];
    match &s.next {
        Some(Next::Step(id)) => out.push(id.as_str()),
        Some(Next::Branches(bs)) => out.extend(bs.iter().map(|b| b.go.as_str())),
        None => {}
    }
    out.extend(s.on_error.as_deref());
    out.extend(s.on_timeout.as_deref());
    out.extend(s.wait.as_ref().and_then(|w| w.recommend.as_deref()));
    out
}

/// Every string inside a `with` value, for its templates.
fn strings<'a>(v: &'a toml::Value, out: &mut Vec<&'a str>) {
    match v {
        toml::Value::String(s) => out.push(s),
        toml::Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        toml::Value::Table(t) => t.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

fn check_step(s: &StepDef, states: &[String], ids: &BTreeSet<&str>) -> Result<(), String> {
    match (&s.action, &s.wait) {
        (Some(_), Some(_)) => {
            return Err("a step either does something (do) or waits (wait), not both".into());
        }
        (None, None) if !s.end => {
            return Err("a step does something (do) or waits (wait); only a closing step (end = true) may do neither".into());
        }
        _ => {}
    }
    if s.next.is_none() && !s.end {
        return Err("a step needs next, or end = true".into());
    }
    if let Some(st) = &s.state
        && !states.contains(st)
    {
        return Err(format!(
            "state \"{st}\" is not one of {}",
            states.join(", ")
        ));
    }
    match &s.action {
        Some(a) => actionables::check(a, s.with.as_ref())?,
        None if s.with.is_some() => return Err("with goes with do".into()),
        None => {}
    }
    if let Some(w) = &s.wait
        && w.kind != "input"
        && w.kind != "approval"
    {
        return Err(format!(
            "wait for \"input\" or \"approval\", not \"{}\"",
            w.kind
        ));
    }
    let mut exprs: Vec<&str> = vec![];
    exprs.extend(s.target.as_deref());
    exprs.extend(s.wait.as_ref().and_then(|w| w.when.as_deref()));
    if let Some(Next::Branches(bs)) = &s.next {
        exprs.extend(bs.iter().filter_map(|b| b.when.as_deref()));
    }
    for e in exprs {
        expr::compile(e)?;
    }
    if let Some(with) = &s.with {
        let mut texts = vec![];
        with.values().for_each(|v| strings(v, &mut texts));
        for t in texts {
            for e in expr::templates(t)? {
                expr::compile(&e)?;
            }
        }
    }
    let durations = [
        s.timeout.as_deref(),
        s.wait.as_ref().and_then(|w| w.timeout.as_deref()),
        s.retry.as_ref().and_then(|r| r.backoff.as_deref()),
    ];
    for d in durations.into_iter().flatten() {
        expr::duration(d)?;
    }
    for t in targets(s) {
        if !ids.contains(t) {
            let what = match &s.next {
                Some(Next::Step(n)) if n == t => "next",
                Some(Next::Branches(bs)) if bs.iter().any(|b| b.go == t) => "go",
                _ if s.on_error.as_deref() == Some(t) => "on_error",
                _ if s.on_timeout.as_deref() == Some(t) => "on_timeout",
                _ => "recommend",
            };
            return Err(format!("{what} \"{t}\" is not a step"));
        }
    }
    Ok(())
}

/// Checks a parsed lifecycle; `path` names it in errors and gives the name
/// it must have.
pub fn validate(path: &Path, f: &File) -> Result<(), LifecycleError> {
    let file = path.display().to_string();
    let whole = |message: String| LifecycleError {
        file: file.clone(),
        step: None,
        message,
    };
    let at = |id: &str, message: String| LifecycleError {
        file: file.clone(),
        step: Some(id.to_string()),
        message,
    };
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if f.name != stem {
        return Err(whole(format!(
            "name \"{}\" must match the file name \"{stem}\"",
            f.name
        )));
    }
    let mut ids = BTreeSet::new();
    for s in &f.step {
        let ok = !s.id.is_empty()
            && s.id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !ok {
            return Err(at(&s.id, "an id is lowercase letters, digits and -".into()));
        }
        if !ids.insert(s.id.as_str()) {
            return Err(at(&s.id, "the id is used twice".into()));
        }
    }
    if !ids.contains(f.start.as_str()) {
        return Err(whole(format!("start \"{}\" is not a step", f.start)));
    }
    let states: Vec<String> = match &f.states {
        Some(s) => s.clone(),
        None => DEFAULT_STATES.iter().map(|s| s.to_string()).collect(),
    };
    for s in &f.step {
        check_step(s, &states, &ids).map_err(|m| at(&s.id, m))?;
    }
    // every step reachable from start
    let by_id: BTreeMap<&str, &StepDef> = f.step.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut seen = BTreeSet::from([f.start.as_str()]);
    let mut queue = VecDeque::from([f.start.as_str()]);
    while let Some(id) = queue.pop_front() {
        for t in targets(by_id[id]) {
            if seen.insert(t) {
                queue.push_back(t);
            }
        }
    }
    if let Some(s) = f.step.iter().find(|s| !seen.contains(s.id.as_str())) {
        return Err(at(&s.id, "not reachable from start".into()));
    }
    // a closing step reachable from every step
    for s in &f.step {
        let mut seen = BTreeSet::from([s.id.as_str()]);
        let mut queue = VecDeque::from([s.id.as_str()]);
        let mut found = false;
        while let Some(id) = queue.pop_front() {
            if by_id[id].end {
                found = true;
                break;
            }
            for t in targets(by_id[id]) {
                if seen.insert(t) {
                    queue.push_back(t);
                }
            }
        }
        if !found {
            return Err(at(&s.id, "no closing step is reachable from here".into()));
        }
    }
    Ok(())
}
