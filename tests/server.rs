mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use primum::projects::Entry;
use primum::server::router;
use primum::store::{self, Changes, Project};
use serde_json::{Value, json};
use tower::ServiceExt;

fn setup(dir: &std::path::Path) -> (Project, axum::Router) {
    let mut p = common::project(&dir.join("işler"));
    p.name = "işler".into();
    let c = |body: &str| Changes {
        body: Some(body.into()),
        ..Default::default()
    };
    store::insert(&p, "TODO", "Özgür'ün işi", c("## Neden\n")).unwrap();
    store::insert(&p, "BUG", "Gökçe'nin hatası", Changes::default()).unwrap();
    store::set_doc(&p, "TODO:1", "spec", "# Spec\n\nIşıl yazdı.\n").unwrap();
    let missing = Entry {
        path: dir.join("yok"),
        project: Err("no primum.toml in yok".into()),
    };
    let entries = vec![
        missing,
        Entry {
            path: p.root.clone(),
            project: Ok(p.clone()),
        },
    ];
    (p, router(entries))
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let location = res
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    (status, location.unwrap_or(text))
}

async fn post(app: &axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::post(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

const API: &str = "/p/i%C5%9Fler/api";

#[tokio::test]
async fn items_in_priority_order_with_page_fields() {
    let d = tempfile::tempdir().unwrap();
    let (_, app) = setup(d.path());
    let (code, text) = get(&app, &format!("{API}/items")).await;
    assert_eq!(code, StatusCode::OK);
    let items: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(items[0]["id"], "BUG:1");
    assert_eq!(items[1]["id"], "TODO:1");
    for field in [
        "score",
        "kind_rank",
        "blockers",
        "blocked",
        "docs",
        "file",
        "stem",
        "body",
    ] {
        assert!(items[1].get(field).is_some(), "{field}");
    }
    assert_eq!(items[1]["docs"][0]["path"], "items/TODO-1.spec.md");
    assert_eq!(items[1]["file"], "items/TODO-1.md");
}

#[tokio::test]
async fn doc_serves_documents_only() {
    let d = tempfile::tempdir().unwrap();
    let (_, app) = setup(d.path());
    let (code, text) = get(&app, &format!("{API}/doc?path=items/TODO-1.spec.md")).await;
    assert_eq!(
        (code, text.as_str()),
        (StatusCode::OK, "# Spec\n\nIşıl yazdı.\n")
    );
    for bad in [
        "items/TODO-1.md",
        "../primum.toml",
        "items%2F..%2FBOARD.md",
        "items/TODO-9.spec.md",
        "",
    ] {
        let (code, _) = get(&app, &format!("{API}/doc?path={bad}")).await;
        assert_eq!(code, StatusCode::NOT_FOUND, "{bad}");
    }
}

#[tokio::test]
async fn update_writes_the_file_and_reopening_clears_finished() {
    let d = tempfile::tempdir().unwrap();
    let (p, app) = setup(d.path());
    let (code, _) = post(
        &app,
        &format!("{API}/items"),
        json!({"id": "BUG:1", "status": "done", "effort": "3"}),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    let file = common::read(&p.root, "items/BUG-1.md");
    assert!(
        file.contains("status: done") && file.contains("finished: ") && file.contains("effort: 3")
    );
    post(
        &app,
        &format!("{API}/items"),
        json!({"id": "BUG:1", "status": "open"}),
    )
    .await;
    assert!(!common::read(&p.root, "items/BUG-1.md").contains("finished:"));
    let (code, v) = post(
        &app,
        &format!("{API}/items"),
        json!({"id": "BUG:1", "status": "bitti"}),
    )
    .await;
    assert_eq!(
        (code, v["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("unknown status bitti"))
    );
    let (code, _) = post(
        &app,
        &format!("{API}/items"),
        json!({"id": "BUG:9", "risk": 2}),
    )
    .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _) = post(&app, &format!("{API}/items"), json!({"id": "BUG:1"})).await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "nothing to update");
    let (code, _) = post(
        &app,
        &format!("{API}/items"),
        json!({"id": "BUG:1", "needs": "TODO:1, BUG:2"}),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert!(common::read(&p.root, "items/BUG-1.md").contains("needs: [TODO:1, BUG:2]"));
}

#[tokio::test]
async fn kind_new_and_delete() {
    let d = tempfile::tempdir().unwrap();
    let (p, app) = setup(d.path());
    let (code, v) = post(
        &app,
        &format!("{API}/items/kind"),
        json!({"id": "TODO:1", "kind": "URGENT"}),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        (v["old"].as_str(), v["new"].as_str()),
        (Some("TODO:1"), Some("URGENT:1"))
    );
    assert!(p.root.join("items/URGENT-1.spec.md").is_file());

    let (code, v) = post(
        &app,
        &format!("{API}/new"),
        json!({"kind": "BUG", "title": "Işıl'ın bulduğu", "impact": 9}),
    )
    .await;
    assert_eq!((code, v["id"].as_str()), (StatusCode::OK, Some("BUG:2")));
    assert!(common::read(&p.root, "items/BUG-2.md").contains("impact: 9"));
    let (code, _) = post(
        &app,
        &format!("{API}/new"),
        json!({"kind": "IDEA", "title": "x"}),
    )
    .await;
    assert_eq!(code, StatusCode::BAD_REQUEST);

    let (code, v) = post(&app, &format!("{API}/delete"), json!({"id": "URGENT:1"})).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        v["removed"],
        json!(["items/URGENT-1.md", "items/URGENT-1.spec.md"])
    );
    assert!(!p.root.join("items/URGENT-1.md").exists());
}

#[tokio::test]
async fn pages_projects_and_assets() {
    let d = tempfile::tempdir().unwrap();
    let (_, app) = setup(d.path());
    assert_eq!(
        get(&app, "/").await,
        (StatusCode::SEE_OTHER, "/p/i%C5%9Fler/".to_string())
    );
    let (code, page) = get(&app, "/p/i%C5%9Fler/").await;
    assert_eq!(code, StatusCode::OK);
    assert!(page.contains("2 open, 0 in progress"), "{page}");
    assert!(page.contains(r#""API":"/p/i%C5%9Fler/api""#));
    assert!(page.contains(r#"class="secnav-off" title="no primum.toml in yok""#));
    let (code, page) = get(&app, "/p/i%C5%9Fler/item/TODO-1").await;
    assert_eq!(code, StatusCode::OK);
    assert!(
        page.contains("Özgür&#39;ün işi") || page.contains("Özgür&#x27;ün işi"),
        "{page}"
    );
    assert!(page.contains(r#"window.ITEM_ID="TODO:1""#));
    assert_eq!(
        get(&app, "/p/i%C5%9Fler/item/TODO-9").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/p/i%C5%9Fler/item/x").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(get(&app, "/p/yok/").await.0, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/p/yok/api/items").await.0, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/static/board.js").await.0, StatusCode::OK);
    assert_eq!(
        get(&app, "/static/../Cargo.toml").await.0,
        StatusCode::NOT_FOUND
    );
}
