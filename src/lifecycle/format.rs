//! The lifecycle file as TOML: what people write, checked key by key (an
//! unknown key is an error, never ignored).
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub start: String,
    /// What the lifecycle reads: a name and where it comes from.
    #[serde(default)]
    pub needs: BTreeMap<String, String>,
    /// The lifecycle's own states; the defaults when absent.
    #[serde(default)]
    pub states: Option<Vec<String>>,
    pub step: Vec<StepDef>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StepDef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// An actionable and its version: `command@1`.
    #[serde(default, rename = "do", skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with: Option<toml::Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<Wait>,
    /// The condition that completes the step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<Retry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<Next>,
    /// A closing step: the ticket is closed here.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub end: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Wait {
    /// `input` or `approval`.
    #[serde(rename = "for")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// For an approval: the step to take when the timeout passes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommend: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Retry {
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff: Option<String>,
}

/// The next step, or branches tried in order (the last without `when`).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Next {
    Step(String),
    Branches(Vec<Branch>),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Branch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    pub go: String,
}
