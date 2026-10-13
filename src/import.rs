//! A one-time import of a videre-board board (`board.py`'s `data/items.json`
//! and its specs and plans) into an empty project.
use crate::store::Project;
use std::path::Path;

/// What an import wrote and what it left behind.
#[derive(Debug, Default)]
pub struct Report {
    pub items: usize,
    /// (source, copy) of every document copied.
    pub docs: Vec<(String, String)>,
    /// (item, copy) of every document an item lists in its `docs:`.
    pub shared: Vec<(String, String)>,
    /// Source documents linked to no item, not copied.
    pub left: Vec<String>,
}

pub fn run(_board_dir: &Path, _project: &Project) -> anyhow::Result<Report> {
    anyhow::bail!("import is not built yet")
}
