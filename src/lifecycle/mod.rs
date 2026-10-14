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
#[derive(Debug, Clone, PartialEq)]
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

/// The built-in lifecycle every ticket without one follows: a person moves
/// it between idle, doing, done and wontfix.
pub fn manual() -> File {
    parse(Path::new("manual.toml"), include_str!("manual.toml")).expect("the built-in lifecycle")
}
