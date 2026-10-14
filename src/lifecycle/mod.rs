//! Lifecycles: what a ticket goes through, step by step, as a TOML file in
//! the project (`lifecycles/<name>.toml`). This module reads, checks and
//! versions them; running them is the engine's job.
pub mod format;

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

/// The built-in lifecycle every ticket without one follows: a person moves
/// it between idle, doing, done and wontfix.
pub fn manual() -> File {
    parse(Path::new("manual.toml"), include_str!("manual.toml")).expect("the built-in lifecycle")
}
