//! Versions: a step's is a SHA-256 over its canonical form (its fields as
//! JSON with sorted keys); a lifecycle's is a SHA-256 over its header and
//! its steps' versions in order. Comments and formatting never count.
use super::format::{File, StepDef};
use sha2::{Digest, Sha256};

fn hash(v: &serde_json::Value) -> String {
    // serde_json's maps are sorted (no preserve_order), so this is canonical
    let text = serde_json::to_string(v).expect("JSON serializes");
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A step's version.
pub fn step(s: &StepDef) -> String {
    hash(&serde_json::to_value(s).expect("a step serializes"))
}

/// A lifecycle's version: its header and every step's version, in order.
pub fn lifecycle(f: &File) -> String {
    let steps: Vec<String> = f.step.iter().map(step).collect();
    hash(&serde_json::json!({
        "name": f.name,
        "start": f.start,
        "needs": f.needs,
        "states": f.states,
        "steps": steps,
    }))
}
