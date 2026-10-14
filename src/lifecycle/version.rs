//! A lifecycle's version: a SHA-256 over its canonical form (JSON with
//! sorted keys, every description removed) and that of every project
//! actionable it names. Formatting and descriptions never count.
use super::definitions::Definition;
use super::format::File;
use sha2::{Digest, Sha256};

fn hash(v: &serde_json::Value) -> String {
    // serde_json's maps are sorted (no preserve_order), so this is canonical
    let text = serde_json::to_string(v).expect("JSON serializes");
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The lifecycle's version, given the project actionables it names.
pub fn lifecycle(f: &File, used: &[&Definition]) -> String {
    let mut f = f.clone();
    f.description.clear();
    for t in &mut f.transitions {
        t.description.clear();
        for a in &mut t.actions {
            a.description.clear();
        }
    }
    let defs: Vec<Definition> = used
        .iter()
        .map(|d| Definition {
            description: String::new(),
            ..(*d).clone()
        })
        .collect();
    hash(&serde_json::json!({ "lifecycle": f, "actionables": defs }))
}
