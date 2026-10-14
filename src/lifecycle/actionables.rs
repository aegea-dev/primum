//! The actionables a step can name (`do = "<name>@<version>"`), and what
//! each needs in `with`. Running them is the workers' job.

/// Every known actionable at its versions.
pub const KNOWN: [&str; 3] = ["command@1", "http@1", "ledger@1"];

/// Checks that `name` is known and `with` has what it needs.
pub fn check(name: &str, with: Option<&toml::Table>) -> Result<(), String> {
    let empty = toml::Table::new();
    let with = with.unwrap_or(&empty);
    let text = |key: &str| with.get(key).and_then(|v| v.as_str()).is_some();
    match name {
        "command@1" => {
            let ok = with
                .get("run")
                .and_then(|v| v.as_array())
                .is_some_and(|a| !a.is_empty() && a.iter().all(|x| x.is_str()));
            if !ok {
                return Err("command@1 needs with.run, a list of strings".into());
            }
        }
        "http@1" => {
            if !text("method") || !text("url") {
                return Err("http@1 needs with.method and with.url".into());
            }
        }
        "ledger@1" => {
            if !["state", "note", "field", "add"]
                .iter()
                .any(|k| with.contains_key(*k))
            {
                return Err("ledger@1 needs at least one of with.state, note, field, add".into());
            }
        }
        _ => return Err(format!("unknown actionable {name}")),
    }
    Ok(())
}
