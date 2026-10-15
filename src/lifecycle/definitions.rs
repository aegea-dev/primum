//! A project's own actionables: `actionables/<name>/actionable.json`, a
//! function's runtime and entry points, after NGINX Unit's application
//! objects. Running them is the workers' job; this reads and checks them.
use super::validate::{check_templates, is_word};
use super::{LifecycleError, expr};
use crate::store::Project;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// `python@<version>`, `bun@<version>` or `executable`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<Limits>,
    /// Python: the virtual environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    /// Python: the requirements file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<String>,
    /// Named entry points.
    pub targets: BTreeMap<String, Target>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// The most a job may take: nothing extends it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// The job's lease: it renews it by touching `PRIMUM_HEARTBEAT`, or is
    /// killed and run again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<String>,
}

/// Seconds a job may take when its actionable sets no `limits.timeout`.
pub const DEFAULT_TIMEOUT: u64 = 60;

/// An entry point: a Python module and callable, a Bun file and export, or
/// a program and its arguments.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<String>,
}

/// Names a project actionable cannot take: the built-ins' prefixes.
pub const RESERVED: [&str; 5] = ["ticket", "json", "event", "http", "command"];

/// The project's actionables folder.
pub fn dir(p: &Project) -> PathBuf {
    p.root.join("actionables")
}

/// Reads and checks `actionables/<name>/actionable.json`.
pub fn load_file(path: &Path) -> Result<Definition, LifecycleError> {
    let err = |message: String| LifecycleError {
        file: path.display().to_string(),
        at: None,
        message,
    };
    let text = std::fs::read_to_string(path).map_err(|e| err(format!("cannot read it: {e}")))?;
    let d: Definition = serde_json::from_str(&text).map_err(|e| err(e.to_string()))?;
    validate(path, &d)?;
    Ok(d)
}

/// Every actionable in the project, by name: each folder in `actionables/`
/// that has an `actionable.json`.
pub fn load_all(p: &Project) -> Result<Vec<Definition>, LifecycleError> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir(p))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path().join("actionable.json")))
        .filter(|f| f.is_file())
        .collect();
    files.sort();
    files.iter().map(|f| load_file(f)).collect()
}

fn type_name(kind: &str) -> Option<&str> {
    if kind == "executable" {
        return Some("executable");
    }
    let (name, version) = kind.split_once('@')?;
    let ok = !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.');
    (ok && (name == "python" || name == "bun")).then_some(name)
}

fn check_target(kind: &str, t: &Target) -> Result<(), String> {
    let has = |o: &Option<String>| o.is_some();
    let (ok, what) = match kind {
        "python" => (
            has(&t.module)
                && has(&t.callable)
                && !has(&t.file)
                && !has(&t.export)
                && !has(&t.executable)
                && t.arguments.is_empty(),
            "a python@ target is a module and a callable",
        ),
        "bun" => (
            has(&t.file)
                && has(&t.export)
                && !has(&t.module)
                && !has(&t.callable)
                && !has(&t.executable)
                && t.arguments.is_empty(),
            "a bun@ target is a file and an export",
        ),
        _ => (
            has(&t.executable)
                && !has(&t.module)
                && !has(&t.callable)
                && !has(&t.file)
                && !has(&t.export),
            "an executable target is an executable and its arguments",
        ),
    };
    if ok { Ok(()) } else { Err(what.into()) }
}

/// Checks a definition; `path` names it, and its folder gives the name.
pub fn validate(path: &Path, d: &Definition) -> Result<(), LifecycleError> {
    let file = path.display().to_string();
    let whole = |message: String| LifecycleError {
        file: file.clone(),
        at: None,
        message,
    };
    let folder = path
        .parent()
        .and_then(|f| f.file_name())
        .and_then(|f| f.to_str())
        .unwrap_or("");
    if d.name != folder {
        return Err(whole(format!(
            "name \"{}\" must match the folder name \"{folder}\"",
            d.name
        )));
    }
    if d.name.contains('.') || d.name.contains('@') {
        return Err(whole(format!("name \"{}\" has no . or @", d.name)));
    }
    if RESERVED.contains(&d.name.as_str()) {
        return Err(whole(format!(
            "name \"{}\" is taken by a built-in actionable",
            d.name
        )));
    }
    if d.version == 0 {
        return Err(whole("version starts at 1".into()));
    }
    let kind = type_name(&d.kind).ok_or_else(|| {
        whole(format!(
            "type is python@<version>, bun@<version> or executable, not \"{}\"",
            d.kind
        ))
    })?;
    if kind != "python" && (d.home.is_some() || d.requirements.is_some()) {
        return Err(whole("home and requirements are for python@ types".into()));
    }
    if d.targets.is_empty() {
        return Err(whole("targets is empty".into()));
    }
    for (name, t) in &d.targets {
        let at = |message: String| LifecycleError {
            file: file.clone(),
            at: Some(format!("target \"{name}\"")),
            message,
        };
        if !is_word(name) {
            return Err(at("a target name is lowercase letters, digits and -".into()));
        }
        check_target(kind, t).map_err(at)?;
    }
    if let Some(l) = &d.limits {
        let timeout = match l.timeout.as_deref() {
            Some(t) => expr::duration(t).map_err(&whole)?,
            None => std::time::Duration::from_secs(DEFAULT_TIMEOUT),
        };
        if let Some(h) = l.heartbeat.as_deref()
            && expr::duration(h).map_err(&whole)? > timeout
        {
            return Err(whole("heartbeat is longer than timeout".into()));
        }
    }
    for v in d.environment.values() {
        check_templates(v).map_err(&whole)?;
    }
    Ok(())
}
