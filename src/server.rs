//! The local web board: axum routes, the JSON API and the pages.
use crate::model::{Item, blockers, kind_rank};
use crate::store::{Board, Doc};
use serde::Serialize;

/// An item as the API and `--json` give it: its fields, plus what the page
/// shows beside them.
#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    pub id: String,
    pub kind: String,
    pub num: u32,
    pub title: String,
    pub status: String,
    pub impact: u8,
    pub urgency: u8,
    pub risk: u8,
    pub effort: u8,
    pub needs: Vec<String>,
    pub finished: Option<String>,
    pub source: String,
    pub body: String,
    pub score: f64,
    pub kind_rank: usize,
    pub blockers: Vec<String>,
    pub blocked: bool,
    /// The item's own documents and the ones it lists.
    pub docs: Vec<Doc>,
    /// The item's file, relative to the project root.
    pub file: String,
}

impl ItemView {
    pub fn new(it: &Item, board: &Board) -> ItemView {
        let blockers = blockers(it, &board.items);
        ItemView {
            id: it.id.clone(),
            kind: it.kind.clone(),
            num: it.num,
            title: it.title.clone(),
            status: it.status.clone(),
            impact: it.impact,
            urgency: it.urgency,
            risk: it.risk,
            effort: it.effort,
            needs: it.needs.clone(),
            finished: it.finished.clone(),
            source: it.source.clone(),
            body: it.body.clone(),
            score: it.score(),
            kind_rank: kind_rank(&it.kind),
            blocked: !blockers.is_empty(),
            blockers,
            docs: board.docs.get(&it.id).cloned().unwrap_or_default(),
            file: format!("items/{}", crate::item_file::file_name(&it.id)),
        }
    }
}

/// Every item of the board, in priority order.
pub fn views(board: &Board) -> Vec<ItemView> {
    board
        .items
        .iter()
        .map(|i| ItemView::new(i, board))
        .collect()
}

pub fn serve(_port: u16, _open_browser: bool) -> anyhow::Result<()> {
    anyhow::bail!("the board is not built yet")
}
