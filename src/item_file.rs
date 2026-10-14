//! One item, one markdown file: a flat YAML front matter, then the body.
//! primum writes exactly the form below and reads it with this parser; any
//! other shape is an error naming the file and line, never a silent default.
//!
//! ```text
//! ---
//! title: "..."                JSON-style quoted, so any text is safe
//! kind: BUG                   one of the lanes
//! status: open
//! impact: 5                   urgency, risk, effort the same
//! needs: [3, 5]               always written, [] when empty
//! docs: [items/3.spec.md]     only when non-empty
//! finished: 2026-10-13        only when set
//! source: "..."               only when non-empty
//! ---
//!
//! body
//! ```
use crate::model::{Item, KINDS, STATUSES, parse_id};
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

/// `12` is `12.md`.
pub fn file_name(id: &str) -> String {
    format!("{}.md", stem(id))
}

/// The start of every file name of an item: its number.
pub fn stem(id: &str) -> String {
    id.to_string()
}

/// The number of an item file name (`12.md`), None for anything else.
pub fn parse_item_name(name: &str) -> Option<u32> {
    let stem = name.strip_suffix(".md")?;
    if stem.starts_with('#') {
        return None;
    }
    parse_id(stem)
}

/// The number and type of a document name (`12.spec.md`,
/// `12.spec.2026-10-06.md`), None for anything else.
pub fn parse_doc_name(name: &str) -> Option<(u32, String)> {
    let stem = name.strip_suffix(".md")?;
    let (id, rest) = stem.split_once('.')?;
    let num = parse_item_name(&format!("{id}.md"))?;
    let doc_type = rest.split('.').next().filter(|t| !t.is_empty())?;
    Some((num, doc_type.to_string()))
}

/// A file name from before numbers (`TODO-7.md`, `TODO-7.spec.md`).
pub fn old_style_name(name: &str) -> bool {
    static OLD: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^(URGENT|BUG|DEBT|KILLER|TODO)-[0-9]+(\.[^/]*)?\.md$")
            .expect("a valid pattern")
    });
    OLD.is_match(name)
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
    out += &format!("kind: {}\n", item.kind);
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
        "kind" => {
            if !KINDS.contains(&raw) {
                return Err(format!("unknown kind `{raw}`"));
            }
            item.kind = raw.to_string();
        }
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

/// Reads an item file. `file` is its name, which gives the number.
pub fn parse(file: &str, text: &str) -> Result<Item, ParseError> {
    let err = |line: usize, message: String| ParseError {
        file: file.to_string(),
        line,
        message,
    };
    let num = parse_item_name(file).ok_or_else(|| err(0, "not an item file name".into()))?;
    let mut item = Item::new("TODO", num, "");
    let mut lines = text.split_inclusive('\n');
    if lines.next().map(str::trim_end) != Some("---") {
        return Err(err(1, "expected `---` to open the front matter".into()));
    }
    let mut n = 1;
    let mut closed = false;
    let mut seen_title = false;
    let mut seen_kind = false;
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
        seen_kind |= key == "kind";
    }
    if !closed {
        return Err(err(n + 1, "front matter is not closed with `---`".into()));
    }
    if !seen_title {
        return Err(err(1, "front matter has no `title`".into()));
    }
    if !seen_kind {
        return Err(err(1, "front matter has no `kind`".into()));
    }
    let rest: String = lines.collect();
    item.body = rest.strip_prefix('\n').unwrap_or(&rest).to_string();
    Ok(item)
}
