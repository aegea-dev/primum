//! Conflicts a sync could not decide: `conflicts/<STEM>.json`, one file per
//! item, tracked so every machine sees them and anyone can resolve them.
//! The item file holds the first value of each; the record holds both.
use crate::item_file::stem;
use crate::store::Project;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Side {
    pub author: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldValues {
    pub field: String,
    pub values: [String; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyValues {
    pub values: [String; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocConflict {
    /// The document as this side has it, in place.
    pub path: String,
    /// The other side's text, saved beside it.
    pub other: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub id: String,
    pub sides: [Side; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldValues>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyValues>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<DocConflict>,
}

impl Conflict {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.body.is_none() && self.documents.is_empty()
    }

    /// Adds `other`'s entries; one for a field or document already here
    /// replaces it.
    pub fn add(&mut self, other: Conflict) {
        self.sides = other.sides;
        for f in other.fields {
            self.fields.retain(|x| x.field != f.field);
            self.fields.push(f);
        }
        if other.body.is_some() {
            self.body = other.body;
        }
        for d in other.documents {
            self.documents.retain(|x| x.path != d.path);
            self.documents.push(d);
        }
    }

    /// What conflicts, as BOARD.md and the CLI list it.
    pub fn what(&self) -> Vec<String> {
        let mut out: Vec<String> = self.fields.iter().map(|f| f.field.clone()).collect();
        if self.body.is_some() {
            out.push("body".into());
        }
        out.extend(self.documents.iter().map(|d| d.path.clone()));
        out
    }
}

fn dir(p: &Project) -> PathBuf {
    p.root.join("conflicts")
}

fn path(p: &Project, id: &str) -> PathBuf {
    dir(p).join(format!("{}.json", stem(id)))
}

pub fn read_all(p: &Project) -> Result<BTreeMap<String, Conflict>> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(dir(p)) else {
        return Ok(out);
    };
    for e in entries.flatten() {
        let file = e.path();
        if file.extension().is_some_and(|x| x == "json") {
            let text = fs::read_to_string(&file)?;
            let c: Conflict = serde_json::from_str(&text)
                .with_context(|| format!("{} is not a conflict record", file.display()))?;
            out.insert(c.id.clone(), c);
        }
    }
    Ok(out)
}

/// Writes the record, or removes the file when nothing is left in it.
pub fn write(p: &Project, c: &Conflict) -> Result<()> {
    let file = path(p, &c.id);
    if c.is_empty() {
        if file.exists() {
            fs::remove_file(file)?;
        }
        return Ok(());
    }
    fs::create_dir_all(dir(p))?;
    fs::write(file, serde_json::to_string_pretty(c)? + "\n")?;
    Ok(())
}

pub fn rename(p: &Project, old_id: &str, new_id: &str) -> Result<()> {
    let from = path(p, old_id);
    if let Ok(text) = fs::read_to_string(&from) {
        let mut c: Conflict = serde_json::from_str(&text)?;
        c.id = new_id.to_string();
        fs::remove_file(&from)?;
        write(p, &c)?;
    }
    Ok(())
}

pub fn remove(p: &Project, id: &str) -> Result<()> {
    let file = path(p, id);
    if file.exists() {
        fs::remove_file(file)?;
    }
    Ok(())
}
