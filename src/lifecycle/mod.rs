//! Lifecycles: what a ticket goes through, step by step, as a TOML file in
//! the project (`lifecycles/<name>.toml`). This module reads, checks and
//! versions them; running them is the engine's job.
pub mod actionables;
pub mod expr;
pub mod format;
pub mod validate;
pub mod version;

use crate::store::Project;
use std::fmt;
use std::path::{Path, PathBuf};

pub use format::File;

/// A problem with a lifecycle file, naming the file and, when it is about
/// one step, the step.
#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleError {
    pub file: String,
    pub step: Option<String>,
    pub message: String,
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.step {
            Some(s) => write!(f, "{}: step \"{s}\": {}", self.file, self.message),
            None => write!(f, "{}: {}", self.file, self.message),
        }
    }
}

impl std::error::Error for LifecycleError {}

/// The project's lifecycles folder.
pub fn dir(p: &Project) -> PathBuf {
    p.root.join("lifecycles")
}

/// Reads a lifecycle file's TOML; nothing is checked beyond the keys.
pub fn parse(path: &Path, text: &str) -> Result<File, LifecycleError> {
    toml::from_str(text).map_err(|e| LifecycleError {
        file: path.display().to_string(),
        step: None,
        message: e.message().to_string(),
    })
}

/// A lifecycle that passed its checks.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Lifecycle {
    pub file: File,
    /// The whole lifecycle's version (64 hex characters).
    pub version: String,
    /// Each step's id and version, in file order.
    pub steps: Vec<(String, String)>,
}

/// A version as people see it: its first 12 characters.
pub fn short(version: &str) -> &str {
    &version[..version.len().min(12)]
}

/// Parses and checks a lifecycle's text; `path` names it.
pub fn load_text(path: &Path, text: &str) -> Result<Lifecycle, LifecycleError> {
    let file = parse(path, text)?;
    validate::validate(path, &file)?;
    let steps = file
        .step
        .iter()
        .map(|s| (s.id.clone(), version::step(s)))
        .collect();
    Ok(Lifecycle {
        version: version::lifecycle(&file),
        steps,
        file,
    })
}

/// Reads, parses and checks a lifecycle file.
pub fn load_file(path: &Path) -> Result<Lifecycle, LifecycleError> {
    let text = std::fs::read_to_string(path).map_err(|e| LifecycleError {
        file: path.display().to_string(),
        step: None,
        message: format!("cannot read it: {e}"),
    })?;
    load_text(path, &text)
}

/// A lifecycle's states, in order: its declared `states`, else the states
/// its steps name (first use), else the defaults.
pub fn states(f: &File) -> Vec<String> {
    if let Some(s) = &f.states {
        return s.clone();
    }
    let mut out: Vec<String> = vec![];
    for s in f.step.iter().filter_map(|s| s.state.as_ref()) {
        if !out.contains(s) {
            out.push(s.clone());
        }
    }
    if out.is_empty() {
        out = validate::DEFAULT_STATES
            .iter()
            .map(|s| s.to_string())
            .collect();
    }
    out
}

/// The states a ticket is closed in: those of the lifecycle's closing
/// steps (a closing step without `state` keeps the previous step's).
pub fn closed_states(f: &File) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut current: Option<&String> = None;
    for s in &f.step {
        if s.state.is_some() {
            current = s.state.as_ref();
        }
        if s.end
            && let Some(c) = current
            && !out.contains(c)
        {
            out.push(c.clone());
        }
    }
    out
}

/// The state a ticket starts in.
pub fn first_state(f: &File) -> String {
    let start = f.step.iter().find(|s| s.id == f.start);
    start
        .and_then(|s| s.state.clone())
        .unwrap_or_else(|| states(f).into_iter().next().unwrap_or_default())
}

/// A lifecycle by name: the project's file, else a built-in one.
pub fn resolve(p: &Project, name: &str) -> Option<Result<Lifecycle, LifecycleError>> {
    let file = dir(p).join(format!("{name}.toml"));
    if file.is_file() {
        return Some(load_file(&file));
    }
    built_in(name).map(Ok)
}

/// `stakeholder-update@f2c...` is ("stakeholder-update", "f2c...").
pub fn split_pin(pin: &str) -> (&str, &str) {
    pin.split_once('@').unwrap_or((pin, ""))
}

/// What the ledger needs of a ticket's lifecycle: its states, in order,
/// and the ones it is closed in.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Rules {
    pub states: Vec<String>,
    pub closed: Vec<String>,
}

impl Rules {
    pub fn of(f: &File) -> Rules {
        Rules {
            states: states(f),
            closed: closed_states(f),
        }
    }

    /// `manual`'s: idle, doing, done, wontfix.
    pub fn manual() -> Rules {
        Rules::of(&manual())
    }
}

const MANUAL: &str = include_str!("manual.toml");

/// The built-in lifecycle every ticket without one follows: a person moves
/// it between idle, doing, done and wontfix.
pub fn manual() -> File {
    built_in("manual").expect("the built-in lifecycle").file
}

/// A lifecycle primum ships, by name.
pub fn built_in(name: &str) -> Option<Lifecycle> {
    match name {
        "manual" => Some(
            load_text(Path::new("manual.toml"), MANUAL).expect("the built-in lifecycle is valid"),
        ),
        _ => None,
    }
}
