//! The actionables an action can name: primum's built-ins, and the
//! project's own (`<name>.<target>@<version>`, see `definitions`), and what
//! each needs in `with`. Running them is the manager's and workers' job.
use super::definitions::Definition;
use super::format::File;
use serde_json::{Map, Value};

/// The built-in actionables at their versions.
pub const BUILT_IN: [&str; 5] = [
    "ticket.update@1",
    "json.pick@1",
    "event.emit@1",
    "http.request@1",
    "command@1",
];

/// Checks that `name` is a built-in, or one of `defs` at its version, and
/// that `with` has what it needs.
pub fn check(name: &str, with: Option<&Map<String, Value>>, defs: &[Definition]) -> Result<(), String> {
    let empty = Map::new();
    let with = with.unwrap_or(&empty);
    let text = |k: &str| with.get(k).is_some_and(Value::is_string);
    let only = |keys: &[&str]| match with.keys().find(|k| !keys.contains(&k.as_str())) {
        Some(k) => Err(format!("{name} takes no with.{k}")),
        None => Ok(()),
    };
    match name {
        "ticket.update@1" => {
            only(&["title", "impact", "urgency", "risk", "effort", "needs", "body", "note"])?;
            if with.is_empty() {
                return Err("ticket.update@1 needs at least one of with.title, impact, urgency, risk, effort, needs, body, note".into());
            }
        }
        "json.pick@1" => {
            only(&["path"])?;
            if !text("path") {
                return Err("json.pick@1 needs with.path, like \"json.data\"".into());
            }
        }
        "event.emit@1" => {
            only(&["type", "target", "data"])?;
            if !text("type") || !text("target") {
                return Err("event.emit@1 needs with.type and with.target".into());
            }
        }
        "http.request@1" => {
            only(&["url", "method", "headers", "json", "body"])?;
            if !text("url") {
                return Err("http.request@1 needs with.url".into());
            }
            if with.contains_key("json") && with.contains_key("body") {
                return Err("http.request@1 takes with.json or with.body, not both".into());
            }
        }
        "command@1" => {
            only(&["run"])?;
            let ok = with
                .get("run")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string));
            if !ok {
                return Err("command@1 needs with.run, a list of strings".into());
            }
        }
        _ => return project(name, defs).map(|_| ()),
    }
    Ok(())
}

/// A project actionable named `<name>.<target>@<version>`.
fn project<'a>(name: &str, defs: &'a [Definition]) -> Result<&'a Definition, String> {
    let unknown = || format!("unknown actionable {name}");
    let (what, version) = name.rsplit_once('@').ok_or_else(unknown)?;
    let (def, target) = what.split_once('.').ok_or_else(unknown)?;
    let d = defs.iter().find(|d| d.name == def).ok_or_else(unknown)?;
    if !d.targets.contains_key(target) {
        return Err(format!("actionable {def} has no target {target}"));
    }
    if version != d.version.to_string() {
        return Err(format!("actionable {def} is at version {}, not {version}", d.version));
    }
    Ok(d)
}

/// The project actionables a lifecycle names, each once, by name.
pub fn used<'a>(f: &File, defs: &'a [Definition]) -> Vec<&'a Definition> {
    let mut out: Vec<&Definition> = f
        .transitions
        .iter()
        .flat_map(|t| &t.actions)
        .filter(|a| !BUILT_IN.contains(&a.action.as_str()))
        .filter_map(|a| project(&a.action, defs).ok())
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}
