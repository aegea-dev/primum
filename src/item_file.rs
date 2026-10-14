//! One item, one markdown file: a flat YAML front matter, then the body.
//! primum writes exactly the form below and reads it with this parser; any
//! other shape is an error naming the file and line, never a silent default.
//!
//! ```text
//! ---
//! title: "..."                JSON-style quoted, so any text is safe
//! status: open
//! impact: 5                   urgency, risk, effort the same
//! needs: [BUG:1, TODO:2]      always written, [] when empty
//! docs: [items/X-1.spec.md]   only when non-empty
//! finished: 2026-10-13        only when set
//! source: "..."               only when non-empty
//! ---
//!
//! body
//! ```
use crate::model::{Item, STATUSES, parse_id};
use std::fmt;

#[derive(Debug, PartialEq)]
pub struct ParseError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// `TODO:87` is `TODO-87.md`.
pub fn file_name(id: &str) -> String {
    format!("{}.md", stem(id))
}

/// `TODO:87` is `TODO-87`, the start of every file name of that item.
pub fn stem(id: &str) -> String {
    id.replacen(':', "-", 1)
}

fn split_stem(stem: &str) -> Option<(String, u32)> {
    let (kind, num) = stem.split_once('-')?;
    parse_id(&format!("{kind}:{num}"))
}

/// The id parts of an item file name (`TODO-87.md`), None for anything else.
pub fn parse_item_name(name: &str) -> Option<(String, u32)> {
    let stem = name.strip_suffix(".md")?;
    if stem.contains('.') {
        return None;
    }
    split_stem(stem)
}

/// The id parts and type of a document name (`TODO-87.spec.md`,
/// `TODO-87.spec.2026-10-06.md`), None for anything else.
pub fn parse_doc_name(name: &str) -> Option<(String, u32, String)> {
    let stem = name.strip_suffix(".md")?;
    let (id, rest) = stem.split_once('.')?;
    let (kind, num) = split_stem(id)?;
    let doc_type = rest.split('.').next().filter(|t| !t.is_empty())?;
    Some((kind, num, doc_type.to_string()))
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).expect("a string serializes")
}

fn list(v: &[String]) -> String {
    format!("[{}]", v.join(", "))
}

pub fn write(item: &Item) -> String {
    let mut out = String::from("---\n");
    out += &format!("title: {}\n", quote(&item.title));
    out += &format!("status: {}\n", item.status);
    for (k, v) in [
        ("impact", item.impact),
        ("urgency", item.urgency),
        ("risk", item.risk),
        ("effort", item.effort),
    ] {
        out += &format!("{k}: {v}\n");
    }
    out += &format!("needs: {}\n", list(&item.needs));
    if !item.docs.is_empty() {
        out += &format!("docs: {}\n", list(&item.docs));
    }
    if let Some(f) = &item.finished {
        out += &format!("finished: {f}\n");
    }
    if !item.source.is_empty() {
        out += &format!("source: {}\n", quote(&item.source));
    }
    out += "---\n";
    if !item.body.is_empty() {
        out += "\n";
        out += &item.body;
        if !item.body.ends_with('\n') {
            out += "\n";
        }
    }
    out
}

fn text_value(raw: &str) -> Result<String, String> {
    if raw.starts_with('"') {
        serde_json::from_str::<String>(raw).map_err(|e| format!("bad quoted text: {e}"))
    } else {
        Ok(raw.to_string())
    }
}

fn list_value(raw: &str) -> Result<Vec<String>, String> {
    let inner = raw
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .ok_or("expected a list like [A, B]")?;
    inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(text_value)
        .collect()
}

fn score_value(key: &str, raw: &str) -> Result<u8, String> {
    let n: i64 = raw
        .parse()
        .map_err(|_| format!("{key} must be a number, not `{raw}`"))?;
    if !(1..=10).contains(&n) {
        return Err(format!("{key} must be 1-10, not {n}"));
    }
    Ok(n as u8)
}

fn set_field(item: &mut Item, key: &str, raw: &str) -> Result<(), String> {
    match key {
        "title" => item.title = text_value(raw)?,
        "status" => {
            if !STATUSES.contains(&raw) {
                return Err(format!("unknown status `{raw}`"));
            }
            item.status = raw.to_string();
        }
        "impact" => item.impact = score_value(key, raw)?,
        "urgency" => item.urgency = score_value(key, raw)?,
        "risk" => item.risk = score_value(key, raw)?,
        "effort" => item.effort = score_value(key, raw)?,
        "needs" => item.needs = list_value(raw)?,
        "docs" => item.docs = list_value(raw)?,
        "finished" => item.finished = (!raw.is_empty()).then(|| raw.to_string()),
        "source" => item.source = text_value(raw)?,
        other => return Err(format!("unknown field `{other}`")),
    }
    Ok(())
}

/// Reads an item file. `file` is its name, which gives the kind and number.
pub fn parse(file: &str, text: &str) -> Result<Item, ParseError> {
    let err = |line: usize, message: String| ParseError {
        file: file.to_string(),
        line,
        message,
    };
    let (kind, num) =
        parse_item_name(file).ok_or_else(|| err(0, "not an item file name".into()))?;
    let mut item = Item::new(&kind, num, "");
    let mut lines = text.split_inclusive('\n');
    if lines.next().map(str::trim_end) != Some("---") {
        return Err(err(1, "expected `---` to open the front matter".into()));
    }
    let mut n = 1;
    let mut closed = false;
    let mut seen_title = false;
    for line in lines.by_ref() {
        n += 1;
        let line = line.trim_end();
        if line == "---" {
            closed = true;
            break;
        }
        if line.is_empty() {
            continue;
        }
        let (key, raw) = line
            .split_once(':')
            .ok_or_else(|| err(n, format!("expected `key: value`, got `{line}`")))?;
        let key = key.trim();
        set_field(&mut item, key, raw.trim()).map_err(|m| err(n, m))?;
        seen_title |= key == "title";
    }
    if !closed {
        return Err(err(n + 1, "front matter is not closed with `---`".into()));
    }
    if !seen_title {
        return Err(err(1, "front matter has no `title`".into()));
    }
    let rest: String = lines.collect();
    item.body = rest.strip_prefix('\n').unwrap_or(&rest).to_string();
    Ok(item)
}
