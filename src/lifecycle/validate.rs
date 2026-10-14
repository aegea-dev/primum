//! The checks a lifecycle file passes before a ticket can follow it. The
//! first problem found is reported, naming the file and the transition.
use super::definitions::Definition;
use super::format::{File, Transition};
use super::{FAILED, LifecycleError, actionables, expr};
use serde_json::Value;
use std::path::Path;

/// A lowercase word: letters, digits and `-`.
pub fn is_word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Every string inside a JSON value, for its templates.
pub fn strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/// Compiles every `${...}` in a text.
pub fn check_templates(text: &str) -> Result<(), String> {
    for e in expr::templates(text)? {
        expr::compile(&e)?;
    }
    Ok(())
}

fn check_transition(t: &Transition, states: &[String], defs: &[Definition]) -> Result<(), String> {
    let named = [
        ("from", Some(&t.from)),
        ("to", Some(&t.to)),
        ("while", t.showing.as_ref()),
        ("on_timeout", t.on_timeout.as_ref()),
        ("on_fail", t.on_fail.as_ref()),
    ];
    for (what, s) in named {
        if let Some(s) = s
            && !states.contains(s)
        {
            return Err(format!("{what} \"{s}\" is not a state"));
        }
    }
    if t.on.is_none() && t.actions.is_empty() {
        return Err("a transition needs on, do, or both".into());
    }
    if let Some(on) = &t.on {
        let ok = on.split('.').all(is_word);
        if !ok {
            return Err(format!("on \"{on}\" is an event type, like ticket.created"));
        }
    }
    match (&t.timeout, &t.on_timeout) {
        (Some(d), Some(_)) => {
            expr::duration(d)?;
        }
        (Some(_), None) => return Err("timeout needs on_timeout".into()),
        (None, Some(_)) => return Err("on_timeout needs timeout".into()),
        (None, None) => {}
    }
    if let Some(w) = &t.when {
        expr::compile(w)?;
    }
    for a in &t.actions {
        actionables::check(&a.action, a.with.as_ref(), defs)?;
        if let Some(e) = &a.expect {
            expr::compile(e)?;
        }
        if let Some(with) = &a.with {
            let mut texts = vec![];
            with.values().for_each(|v| strings(v, &mut texts));
            for text in texts {
                check_templates(text)?;
            }
        }
    }
    Ok(())
}

/// Checks a parsed lifecycle against the project's actionables; `path`
/// names it in errors and gives the name it must have.
pub fn validate(path: &Path, f: &File, defs: &[Definition]) -> Result<(), LifecycleError> {
    let file = path.display().to_string();
    let whole = |message: String| LifecycleError {
        file: file.clone(),
        at: None,
        message,
    };
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if f.name != stem {
        return Err(whole(format!("name \"{}\" must match the file name \"{stem}\"", f.name)));
    }
    if f.states.is_empty() {
        return Err(whole("states is empty".into()));
    }
    for (i, s) in f.states.iter().enumerate() {
        if !is_word(s) {
            return Err(whole(format!("state \"{s}\" is lowercase letters, digits and -")));
        }
        if f.states[..i].contains(s) {
            return Err(whole(format!("state \"{s}\" is listed twice")));
        }
        if s == FAILED {
            return Err(whole("\"failed\" is the engine's own state; do not list it".into()));
        }
    }
    if !f.states.contains(&f.start) {
        return Err(whole(format!("start \"{}\" is not a state", f.start)));
    }
    if let Some(e) = f.end.iter().find(|e| !f.states.contains(e)) {
        return Err(whole(format!("end \"{e}\" is not a state")));
    }
    for t in &f.transitions {
        check_transition(t, &f.states, defs).map_err(|message| LifecycleError {
            file: file.clone(),
            at: Some(format!("transition \"{} -> {}\"", t.from, t.to)),
            message,
        })?;
    }
    Ok(())
}

/// What `primum lifecycle check` points out without failing: a state no
/// transition enters or leaves (apart from start and end), which only a
/// person can move a ticket to. A lifecycle with no transitions at all is
/// moved by a person throughout, like `manual`.
pub fn warnings(f: &File) -> Vec<String> {
    if f.transitions.is_empty() {
        return vec![];
    }
    f.states
        .iter()
        .filter(|s| **s != f.start && !f.end.contains(s))
        .filter(|s| !f.transitions.iter().any(|t| &t.from == *s || &t.to == *s))
        .map(|s| format!("state \"{s}\": no transition enters or leaves it; only a person can move a ticket there"))
        .collect()
}
