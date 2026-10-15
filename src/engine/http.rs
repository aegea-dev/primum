//! The daemon's HTTP server: the board, as `primum serve` serves it, plus
//! `GET /health` and `POST /p/<project>/api/events` for events from outside
//! (a script, a webhook relay). It listens on 127.0.0.1 only.
use super::daemon::{Health, stamp};
use super::journal::{self, Envelope, Journal};
use crate::projects::Entry;
use crate::store::{self, Project};
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};

/// What the server shares with the daemon's loop.
pub struct Engine {
    pub health: Mutex<Health>,
    /// Wakes the loop for an event that just arrived.
    pub wake: Sender<()>,
}

#[derive(Clone)]
struct Shared {
    engine: Arc<Engine>,
    projects: Arc<Vec<Project>>,
}

/// The board's routes and the engine's.
pub fn router(entries: Vec<Entry>, engine: Arc<Engine>) -> Router {
    let projects: Vec<Project> = entries
        .iter()
        .filter_map(|e| e.project.clone().ok())
        .collect();
    let api = Router::new()
        .route("/health", get(health))
        .route("/p/{project}/api/events", post(event))
        .with_state(Shared {
            engine,
            projects: Arc::new(projects),
        });
    crate::server::router(entries).merge(api)
}

type Reply = (StatusCode, Json<Value>);

fn fail(code: StatusCode, message: impl Into<String>) -> Reply {
    (code, Json(json!({ "error": message.into() })))
}

async fn health(State(s): State<Shared>) -> Json<Value> {
    let h = s
        .engine
        .health
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let names: Vec<&str> = s.projects.iter().map(|p| p.name.as_str()).collect();
    Json(json!({
        "ok": true,
        "projects": names,
        "started": h.started,
        "passes": h.passes,
        "last_pass": h.last_pass,
        "last_error": h.last_error,
    }))
}

/// An event as it is posted.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Posted {
    #[serde(rename = "type")]
    kind: String,
    /// The ticket it is for; none for the project.
    #[serde(default)]
    ticket: Option<String>,
    #[serde(default)]
    data: Option<Value>,
}

/// Types the engine sends itself: from outside they would forge a worker's
/// result, a timer or a ledger change.
fn engine_own(kind: &str) -> bool {
    kind == "lifecycle.attached"
        || ["action.", "timer.", "ticket."]
            .iter()
            .any(|p| kind.starts_with(p))
}

async fn event(
    State(s): State<Shared>,
    Path(project): Path<String>,
    body: Result<Json<Posted>, JsonRejection>,
) -> Result<Reply, Reply> {
    let p = s
        .projects
        .iter()
        .find(|p| p.name == project)
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, format!("no project {project}")))?;
    let Json(posted) = body.map_err(|r| fail(StatusCode::BAD_REQUEST, r.body_text()))?;
    let kind = posted.kind;
    if !kind.split('.').all(crate::lifecycle::validate::is_word) {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            format!(
                "an event type is lowercase words joined by dots, like input.received, not {kind}"
            ),
        ));
    }
    if engine_own(&kind) {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            format!("{kind} is the engine's own: it cannot be sent from outside"),
        ));
    }
    let data = match posted.data {
        None => Value::Object(Map::new()),
        Some(v @ Value::Object(_)) => v,
        Some(_) => {
            return Err(fail(StatusCode::BAD_REQUEST, "data must be a JSON object"));
        }
    };
    let ticket = match posted.ticket {
        None => None,
        Some(t) => Some(
            crate::model::parse_id(&t)
                .map(crate::model::id_of)
                .ok_or_else(|| {
                    fail(
                        StatusCode::BAD_REQUEST,
                        format!("{t} is not an item id, like 12 or #12"),
                    )
                })?,
        ),
    };
    // reading the board and writing the journal (which may wait on a busy
    // SQLite) block: they run off the server's async threads
    let p = p.clone();
    let id = tokio::task::spawn_blocking(move || save(&p, ticket, &kind, data))
        .await
        .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    // a loop that has stopped finds the event in the journal when it starts
    let _ = s.engine.wake.send(());
    Ok((StatusCode::ACCEPTED, Json(json!({ "id": id }))))
}

/// Checks an event's ticket and saves the event in the project's journal;
/// returns its id.
fn save(p: &Project, ticket: Option<String>, kind: &str, data: Value) -> Result<String, Reply> {
    let internal = |e: anyhow::Error| fail(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"));
    let target = match ticket {
        None => "project".to_string(),
        Some(id) => {
            let board = store::load(p).map_err(internal)?;
            let it = board
                .get(&id)
                .ok_or_else(|| fail(StatusCode::NOT_FOUND, format!("no item {id}")))?;
            if it.lifecycle.is_none() {
                return Err(fail(
                    StatusCode::CONFLICT,
                    format!("#{id} is not on a lifecycle"),
                ));
            }
            id
        }
    };
    let e = Envelope::new(
        "http",
        kind,
        &journal::subject(&p.name, &target),
        &stamp(chrono::Utc::now()),
        None,
        data,
    );
    Journal::open(p)
        .and_then(|j| j.publish(&e))
        .map_err(internal)?;
    Ok(e.id)
}
