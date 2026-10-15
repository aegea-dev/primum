//! The local web board: axum routes, the JSON API and the pages.
//!
//! Every listed project is served under `/p/<name>/`. The project list is
//! read once at start; each request reloads the project's files, so an edit
//! made in a text editor shows on the next page load.
use crate::item_file::{file_name, parse_item_name, stem};
use crate::model::{Item, blockers, kind_rank};
use crate::projects::{self, Entry};
use crate::store::{self, Board, Changes, Doc, Project};
use askama::Template;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

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
    /// Done or wontfix.
    pub closed: bool,
    /// The item's own documents and the ones it lists.
    pub docs: Vec<Doc>,
    /// The item's file, relative to the project root.
    pub file: String,
    /// What a sync could not decide for this item.
    pub conflict: Option<crate::conflicts::Conflict>,
    /// The start of the item's file names, `TODO-87`; its page is `item/<stem>`.
    pub stem: String,
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
            closed: it.is_closed(),
            blockers,
            docs: board.docs.get(&it.id).cloned().unwrap_or_default(),
            file: format!("items/{}", file_name(&it.id)),
            stem: stem(&it.id),
            conflict: board.conflicts.get(&it.id).cloned(),
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

struct AppState {
    entries: Vec<Entry>,
}

type Shared = Arc<AppState>;

impl AppState {
    fn project(&self, name: &str) -> Result<&Project, ApiError> {
        self.entries
            .iter()
            .filter_map(|e| e.project.as_ref().ok())
            .find(|p| p.name == name)
            .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no project {name}")))
    }
}

/// An API failure: answered as `{"error": message}`.
#[derive(Debug)]
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, axum::Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> ApiError {
        let message = format!("{e:#}");
        let code = if message.starts_with("no item ") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        ApiError(code, message)
    }
}

fn bad(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message.into())
}

pub fn router(entries: Vec<Entry>) -> Router {
    let state = Arc::new(AppState { entries });
    Router::new()
        .route("/", get(root))
        .route("/p/{project}", get(add_slash))
        .route("/p/{project}/", get(board_page))
        .route("/p/{project}/item/{stem}", get(item_page))
        .route("/p/{project}/api/items", get(api_items).post(api_update))
        .route("/p/{project}/api/items/kind", post(api_kind))
        .route("/p/{project}/api/new", post(api_new))
        .route("/p/{project}/api/delete", post(api_delete))
        .route("/p/{project}/api/resolve", post(api_resolve))
        .route("/p/{project}/api/doc", get(api_doc))
        .route("/p/{project}/api/history", get(api_history))
        .route("/p/{project}/api/history/diff", get(api_history_diff))
        .route("/static/{file}", get(asset))
        .with_state(state)
}

/// Serves the board on 127.0.0.1 until interrupted.
pub fn serve(entries: Vec<Entry>, port: u16, open_browser: bool) -> anyhow::Result<()> {
    if entries.iter().all(|e| e.project.is_err()) {
        anyhow::bail!(
            "no primum project here: run primum serve in a workspace or a board, or `primum init`"
        );
    }
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|e| anyhow::anyhow!("cannot listen on 127.0.0.1:{port}: {e}"))?;
        let url = format!("http://127.0.0.1:{port}/");
        println!("primum board at {url}");
        if open_browser {
            let opener = if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            };
            let _ = std::process::Command::new(opener).arg(&url).spawn();
        }
        let projects: Vec<Project> = entries
            .iter()
            .filter_map(|e| e.project.clone().ok())
            .collect();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tick.tick().await;
                for p in projects.clone() {
                    let _ = tokio::task::spawn_blocking(move || background_sync(&p)).await;
                }
            }
        });
        axum::serve(listener, router(entries)).await?;
        Ok(())
    })
}

/// A sync the page did not ask for: only when the project syncs by itself;
/// problems go to the server's stderr.
fn background_sync(p: &Project) {
    if !crate::sync::is_auto(p) {
        return;
    }
    match crate::sync::sync(p) {
        Ok(r) => {
            if let Some(why) = r.offline {
                eprintln!("primum: {}: offline ({why})", p.name);
            }
            for id in r.conflicts {
                eprintln!("primum: {}: {id} has a conflict", p.name);
            }
        }
        Err(e) => eprintln!("primum: {}: sync: {e:#}", p.name),
    }
}

/// Syncs `p` in the background after a change; the answer does not wait.
fn sync_soon(p: &Project) {
    let p = p.clone();
    tokio::task::spawn_blocking(move || background_sync(&p));
}

// ---------------------------------------------------------------- pages

fn url_part(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out += &format!("%{b:02X}");
        }
    }
    out
}

fn project_url(name: &str) -> String {
    format!("/p/{}/", url_part(name))
}

async fn root(State(s): State<Shared>) -> Response {
    match s.entries.iter().find_map(|e| e.project.as_ref().ok()) {
        Some(p) => Redirect::to(&project_url(&p.name)).into_response(),
        None => (StatusCode::NOT_FOUND, "no projects: run `primum init`").into_response(),
    }
}

async fn add_slash(Path(project): Path<String>) -> Redirect {
    Redirect::permanent(&project_url(&project))
}

/// One tab of the nav strip.
pub struct NavLink {
    pub name: String,
    pub href: String,
    pub on: bool,
    /// Why the project cannot be shown, for an unavailable one.
    pub reason: Option<String>,
}

/// What every page shows around its content.
pub struct PageCommon {
    pub project: String,
    pub path: String,
    pub open: usize,
    pub doing: usize,
    pub nav: Vec<NavLink>,
    /// `PROJECT` and `API` for the page's scripts, as a JSON object.
    pub globals: String,
}

/// JSON for a `<script>`: `</` is escaped so no string can end the element.
fn script_json(v: &Value) -> String {
    v.to_string().replace("</", "<\\/")
}

fn common(s: &AppState, p: &Project, board: &Board) -> PageCommon {
    let nav = s
        .entries
        .iter()
        .map(|e| match &e.project {
            Ok(q) => NavLink {
                name: q.name.clone(),
                href: project_url(&q.name),
                on: q.name == p.name,
                reason: None,
            },
            Err(why) => NavLink {
                name: e
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| e.path.display().to_string()),
                href: String::new(),
                on: false,
                reason: Some(why.clone()),
            },
        })
        .collect();
    let api = format!("/p/{}/api", url_part(&p.name));
    PageCommon {
        project: p.name.clone(),
        path: projects::contract(&p.root),
        open: board.items.iter().filter(|i| !i.is_closed()).count(),
        doing: board.items.iter().filter(|i| i.status == "doing").count(),
        nav,
        globals: script_json(&json!({
            "PROJECT": p.name,
            "API": api,
        })),
    }
}

#[derive(Template)]
#[template(path = "board.html")]
struct BoardPage {
    page: PageCommon,
}

#[derive(Template)]
#[template(path = "item.html")]
struct ItemPage {
    page: PageCommon,
    id: String,
    title: String,
    /// `ITEM_ID` for the page's script.
    item_json: String,
}

fn html(t: impl Template) -> Response {
    match t.render() {
        Ok(s) => Html(s).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn board_page(
    State(s): State<Shared>,
    Path(project): Path<String>,
) -> Result<Response, ApiError> {
    let p = s.project(&project)?;
    let board = store::load(p)?;
    Ok(html(BoardPage {
        page: common(&s, p, &board),
    }))
}

async fn item_page(
    State(s): State<Shared>,
    Path((project, item)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let p = s.project(&project)?;
    let board = store::load(p)?;
    let id = parse_item_name(&format!("{item}.md"))
        .map(crate::model::id_of)
        .and_then(|id| board.get(&id).map(|i| i.id.clone()))
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no item {item}")))?;
    let it = board.get(&id).expect("found above");
    Ok(html(ItemPage {
        page: common(&s, p, &board),
        title: it.title.clone(),
        item_json: script_json(&json!(id)),
        id,
    }))
}

// ---------------------------------------------------------------- API

async fn api_items(
    State(s): State<Shared>,
    Path(project): Path<String>,
) -> Result<axum::Json<Vec<ItemView>>, ApiError> {
    let board = store::load(s.project(&project)?)?;
    Ok(axum::Json(views(&board)))
}

async fn api_doc(
    State(s): State<Shared>,
    Path(project): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let p = s.project(&project)?;
    let path = q.get("path").map(String::as_str).unwrap_or("");
    Ok(match store::read_doc(p, path) {
        Some(text) => ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response(),
        None => (StatusCode::NOT_FOUND, "not a document").into_response(),
    })
}

async fn api_history(
    State(s): State<Shared>,
    Path(project): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<axum::Json<Vec<crate::history::Entry>>, ApiError> {
    let p = s.project(&project)?;
    let Some(h) = crate::history::History::open(p)? else {
        return Ok(axum::Json(vec![]));
    };
    let path = match q.get("id") {
        Some(id) => {
            let ids = crate::cli::parse_needs(id).map_err(|e| bad(e.to_string()))?;
            let [id] = ids.as_slice() else {
                return Err(bad("name one item id"));
            };
            Some(format!("items/{}", file_name(id)))
        }
        None => None,
    };
    Ok(axum::Json(h.log(path.as_deref(), 200)?))
}

async fn api_history_diff(
    State(s): State<Shared>,
    Path(project): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let p = s.project(&project)?;
    let h = crate::history::History::open(p)?.ok_or_else(|| bad("history is off"))?;
    let arg = |k: &str| q.get(k).map(String::as_str).unwrap_or("");
    let diff = h.diff(arg("commit"), arg("path"))?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], diff).into_response())
}

fn text_field(v: &Value, key: &str) -> Result<Option<String>, ApiError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(bad(format!("{key} must be text"))),
    }
}

/// A score as board.py's coerce_priority read it: a number or numeric text,
/// clamped to 1-10 later by the store.
fn score_field(v: &Value, key: &str) -> Result<Option<i64>, ApiError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .map(Some)
            .ok_or_else(|| bad(format!("{key} must be a number"))),
        Some(Value::String(s)) => s
            .trim()
            .parse()
            .map(Some)
            .map_err(|_| bad(format!("{key} must be a number"))),
        Some(_) => Err(bad(format!("{key} must be a number"))),
    }
}

/// `needs` as the page sends it (`"TODO:1, BUG:2"`) or as a list.
fn needs_field(v: &Value) -> Result<Option<Vec<String>>, ApiError> {
    let text = match v.get("needs") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| x.as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(","),
        Some(_) => return Err(bad("needs must be text or a list")),
    };
    crate::cli::parse_needs(&text)
        .map(Some)
        .map_err(|e| bad(e.to_string()))
}

fn changes(v: &Value) -> Result<Changes, ApiError> {
    Ok(Changes {
        title: text_field(v, "title")?,
        kind: text_field(v, "kind")?,
        status: text_field(v, "status")?,
        impact: score_field(v, "impact")?,
        urgency: score_field(v, "urgency")?,
        risk: score_field(v, "risk")?,
        effort: score_field(v, "effort")?,
        needs: needs_field(v)?,
        body: text_field(v, "body")?,
    })
}

fn id_field(v: &Value) -> Result<String, ApiError> {
    text_field(v, "id")?.ok_or_else(|| bad("id required"))
}

async fn api_update(
    State(s): State<Shared>,
    Path(project): Path<String>,
    axum::Json(v): axum::Json<Value>,
) -> Result<axum::Json<Value>, ApiError> {
    let p = s.project(&project)?;
    let id = id_field(&v)?;
    let c = changes(&v)?;
    let nothing = c.title.is_none()
        && c.status.is_none()
        && c.impact.is_none()
        && c.urgency.is_none()
        && c.risk.is_none()
        && c.effort.is_none()
        && c.needs.is_none()
        && c.body.is_none();
    if nothing {
        return Err(bad("nothing to update"));
    }
    store::update(p, &id, c)?;
    sync_soon(p);
    Ok(axum::Json(json!({ "ok": true })))
}

async fn api_kind(
    State(s): State<Shared>,
    Path(project): Path<String>,
    axum::Json(v): axum::Json<Value>,
) -> Result<axum::Json<Value>, ApiError> {
    let p = s.project(&project)?;
    let id = id_field(&v)?;
    let kind = text_field(&v, "kind")?.ok_or_else(|| bad("kind required"))?;
    store::set_kind(p, &id, &kind)?;
    sync_soon(p);
    Ok(axum::Json(
        json!({ "old": id, "new": id, "stem": stem(&id) }),
    ))
}

async fn api_new(
    State(s): State<Shared>,
    Path(project): Path<String>,
    axum::Json(v): axum::Json<Value>,
) -> Result<axum::Json<Value>, ApiError> {
    let p = s.project(&project)?;
    let kind = text_field(&v, "kind")?.unwrap_or_else(|| "TODO".into());
    let title = text_field(&v, "title")?.unwrap_or_default();
    let mut c = changes(&v)?;
    c.title = None;
    let id = store::insert(p, &kind, &title, c)?;
    sync_soon(p);
    Ok(axum::Json(json!({ "id": id, "stem": stem(&id) })))
}

async fn api_delete(
    State(s): State<Shared>,
    Path(project): Path<String>,
    axum::Json(v): axum::Json<Value>,
) -> Result<axum::Json<Value>, ApiError> {
    let p = s.project(&project)?;
    let removed = store::delete(p, &id_field(&v)?)?;
    sync_soon(p);
    Ok(axum::Json(json!({ "removed": removed })))
}

async fn api_resolve(
    State(s): State<Shared>,
    Path(project): Path<String>,
    axum::Json(v): axum::Json<Value>,
) -> Result<axum::Json<Value>, ApiError> {
    let p = s.project(&project)?;
    let id = id_field(&v)?;
    let what = text_field(&v, "field")?.ok_or_else(|| bad("field required"))?;
    let keep = match v.get("keep") {
        Some(Value::Number(n)) if n.as_u64() == Some(1) => store::Keep::First,
        Some(Value::Number(n)) if n.as_u64() == Some(2) => store::Keep::Second,
        Some(Value::String(s)) if s == "both" => store::Keep::Both,
        _ => return Err(bad("keep must be 1, 2 or \"both\"")),
    };
    store::resolve(p, &id, &what, keep)?;
    sync_soon(p);
    Ok(axum::Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------- static

/// (name, content type, text) of every file under static/, embedded.
static ASSETS: &[(&str, &str, &str)] = &[
    (
        "chrome.css",
        "text/css; charset=utf-8",
        include_str!("../static/chrome.css"),
    ),
    (
        "primum.css",
        "text/css; charset=utf-8",
        include_str!("../static/primum.css"),
    ),
    (
        "md.js",
        "text/javascript; charset=utf-8",
        include_str!("../static/md.js"),
    ),
    (
        "board.js",
        "text/javascript; charset=utf-8",
        include_str!("../static/board.js"),
    ),
    (
        "item.js",
        "text/javascript; charset=utf-8",
        include_str!("../static/item.js"),
    ),
];

async fn asset(Path(file): Path<String>) -> Response {
    match ASSETS.iter().find(|(name, _, _)| *name == file) {
        Some((_, ctype, text)) => (
            [
                (header::CONTENT_TYPE, *ctype),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            *text,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "no such file").into_response(),
    }
}
