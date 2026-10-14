//! A project's own actionables: `actionables/<name>/actionable.json`, a
//! function's runtime and entry points, after NGINX Unit's application
//! objects. Running them is the workers' job; this reads and checks them.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

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

/// Every actionable in the project's `actionables/` (Task 2 reads them).
pub fn load_all(_p: &crate::store::Project) -> Result<Vec<Definition>, super::LifecycleError> {
    Ok(vec![])
}
