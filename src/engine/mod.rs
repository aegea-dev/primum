//! The lifecycle engine. So far, the lifecycle manager: a pure decision for
//! one ticket and one event (`manager::decide`). The daemon and workers
//! come next.
pub mod builtins;
pub mod daemon;
pub mod eval;
pub mod http;
pub mod journal;
pub mod manager;
pub mod secrets;
pub mod watchdog;
pub mod worker;

use crate::lifecycle::File;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// An event, as the manager sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub data: Value,
}

/// An event the manager asks the daemon to send.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutEvent {
    #[serde(rename = "type")]
    pub kind: String,
    /// `worker`, `project`, or a ticket id.
    pub target: String,
    #[serde(default)]
    pub data: Value,
}

/// A timer: when it is due, the daemon sends `timer.fired` with `data` to
/// the ticket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timer {
    pub at: String,
    pub data: Value,
}

/// A transition whose chain of actions is running.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chain {
    /// Names this run: action results and timers carry it.
    pub run: String,
    /// The transition, by its place in the lifecycle.
    pub transition: usize,
    /// The action a worker is running.
    pub index: usize,
    /// The data of the event that started it.
    pub input: Value,
    /// The last finished action's output.
    pub prev: Value,
    /// Ticket changes held back until the chain succeeds.
    #[serde(default)]
    pub changes: Map<String, Value>,
    /// Events held back until the chain succeeds.
    #[serde(default)]
    pub emits: Vec<OutEvent>,
}

/// Where a failed ticket was, to resume from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Failed {
    pub from: String,
    pub transition: usize,
    pub run: String,
    pub input: Value,
}

/// A ticket's place in its lifecycle, kept by the daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Runtime {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<Chain>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<Failed>,
}

/// Everything the manager is given for one decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Input {
    /// The time, RFC 3339: the manager never reads a clock.
    pub now: String,
    /// The ticket's lifecycle, at its pinned version.
    pub lifecycle: File,
    /// The ticket's fields, `id` and `status` included.
    pub ticket: Map<String, Value>,
    #[serde(default)]
    pub args: Map<String, Value>,
    /// The project's context, stored and computed.
    #[serde(default)]
    pub project: Map<String, Value>,
    pub runtime: Runtime,
    pub event: Event,
}

/// What the manager decided.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    /// The ticket's place after the event.
    pub runtime: Runtime,
    /// What the ticket's status shows.
    pub status: String,
    /// Ticket changes to apply now.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub changes: Map<String, Value>,
    /// Events to send.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<OutEvent>,
    /// The ticket's timers from now on; none means unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timers: Option<Vec<Timer>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<String>,
}

/// One action for a worker: what `primum worker` reads on stdin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub action: String,
    #[serde(default)]
    pub with: Map<String, Value>,
    /// The idempotency key: the same when the action runs again.
    pub key: String,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub prev: Value,
    #[serde(default)]
    pub ticket: Map<String, Value>,
    #[serde(default)]
    pub project: Map<String, Value>,
    #[serde(default)]
    pub args: Map<String, Value>,
    /// The project's folder: where `command@1` runs.
    pub root: std::path::PathBuf,
    /// For a project actionable: the program that runs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<Program>,
    /// Seconds it may take (the daemon kills it after that).
    pub timeout: u64,
}

/// A project actionable as a program: given the job on stdin, it prints
/// its output as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    /// Where it runs.
    pub folder: std::path::PathBuf,
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub environment: std::collections::BTreeMap<String, String>,
    /// For `python@` and `bun@`: what to prepare and call; the program is
    /// then the interpreter itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpreter: Option<Interpreter>,
}

/// The interpreter a project actionable's function runs in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Interpreter {
    Python {
        version: String,
        /// The virtual environment, relative to the folder.
        home: String,
        requirements: Option<String>,
        module: String,
        callable: String,
    },
    Bun {
        file: String,
        export: String,
    },
}
