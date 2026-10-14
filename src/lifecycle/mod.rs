//! Lifecycles: the steps a ticket goes through, from created to closed, as
//! a JSON file in the project (`lifecycles/<name>.json`): states, and
//! transitions between them that run chains of actions. This module reads,
//! checks and versions them; running them is the engine's job.
pub mod actionables;
pub mod definitions;
pub mod expr;
pub mod format;
pub mod validate;
pub mod version;

use crate::store::Project;
use definitions::Definition;
use std::fmt;
use std::path::{Path, PathBuf};

pub use format::File;

/// The engine's own state: a transition failed. Never declared.
pub const FAILED: &str = "failed";

/// The states a ticket may have when its lifecycle's file is gone (the
/// engine keeps pinned versions; the ledger cannot check against them).
pub const FALLBACK_STATES: [&str; 7] = [
    "idle",
    "waiting",
    "processing",
    "effects",
    "done",
    "wontfix",
    "failed",
];

/// A problem with a lifecycle or actionable file, naming the file and,
/// when it is about one part, that part (`transition "a -> b"`).
#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleError {
    pub file: String,
    pub at: Option<String>,
    pub message: String,
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.at {
            Some(at) => write!(f, "{}: {at}: {}", self.file, self.message),
            None => write!(f, "{}: {}", self.file, self.message),
        }
    }
}

impl std::error::Error for LifecycleError {}

/// The project's lifecycles folder.
pub fn dir(p: &Project) -> PathBuf {
    p.root.join("lifecycles")
}

/// Reads a lifecycle file's JSON; nothing is checked beyond the keys.
pub fn parse(path: &Path, text: &str) -> Result<File, LifecycleError> {
    serde_json::from_str(text).map_err(|e| LifecycleError {
        file: path.display().to_string(),
        at: None,
        message: e.to_string(),
    })
}

/// A lifecycle that passed its checks.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Lifecycle {
    pub file: File,
    /// Its version (64 hex characters).
    pub version: String,
}

/// A version as people see it: its first 12 characters.
pub fn short(version: &str) -> &str {
    &version[..version.len().min(12)]
}

/// Parses and checks a lifecycle's text against the project's actionables;
/// `path` names it.
pub fn load_text(
    path: &Path,
    text: &str,
    defs: &[Definition],
) -> Result<Lifecycle, LifecycleError> {
    let file = parse(path, text)?;
    validate::validate(path, &file, defs)?;
    Ok(Lifecycle {
        version: version::lifecycle(&file, &actionables::used(&file, defs)),
        file,
    })
}

/// Reads, parses and checks a lifecycle file.
pub fn load_file(path: &Path, defs: &[Definition]) -> Result<Lifecycle, LifecycleError> {
    let text = std::fs::read_to_string(path).map_err(|e| LifecycleError {
        file: path.display().to_string(),
        at: None,
        message: format!("cannot read it: {e}"),
    })?;
    load_text(path, &text, defs)
}

/// A lifecycle's states, in order.
pub fn states(f: &File) -> Vec<String> {
    f.states.clone()
}

/// The states a ticket is closed in: the lifecycle's `end`.
pub fn closed_states(f: &File) -> Vec<String> {
    f.end.clone()
}

/// The state a ticket starts in.
pub fn first_state(f: &File) -> String {
    f.start.clone()
}

/// A lifecycle by name: the project's file, else a built-in one.
pub fn resolve(p: &Project, name: &str) -> Option<Result<Lifecycle, LifecycleError>> {
    let file = dir(p).join(format!("{name}.json"));
    if file.is_file() {
        return Some(definitions::load_all(p).and_then(|defs| load_file(&file, &defs)));
    }
    built_in(name).map(Ok)
}

/// `enrich-ticket@f2c...` is ("enrich-ticket", "f2c...").
pub fn split_pin(pin: &str) -> (&str, &str) {
    pin.split_once('@').unwrap_or((pin, ""))
}

/// What the ledger needs of a ticket's lifecycle: its states, in order,
/// and the ones it is closed in. A lifecycle that runs (has transitions)
/// can also leave a ticket `failed`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Rules {
    pub states: Vec<String>,
    pub closed: Vec<String>,
}

impl Rules {
    pub fn of(f: &File) -> Rules {
        let mut states = states(f);
        if !f.transitions.is_empty() {
            states.push(FAILED.to_string());
        }
        Rules {
            states,
            closed: closed_states(f),
        }
    }

    /// `manual`'s: idle, doing, done, wontfix.
    pub fn manual() -> Rules {
        Rules::of(&manual())
    }
}

const MANUAL: &str = include_str!("manual.json");

/// The built-in lifecycle every ticket without one follows: a person moves
/// it between idle, doing, done and wontfix.
pub fn manual() -> File {
    built_in("manual").expect("the built-in lifecycle").file
}

/// A lifecycle primum ships, by name.
pub fn built_in(name: &str) -> Option<Lifecycle> {
    match name {
        "manual" => Some(
            load_text(Path::new("manual.json"), MANUAL, &[])
                .expect("the built-in lifecycle is valid"),
        ),
        _ => None,
    }
}
