//! Contract tests for the canonical `/api/artifacts` resource.
//!
//! These tests intentionally do not exercise `/api/docs`: the latter is a
//! Document adapter and is not the public polymorphic resource boundary.

use std::io::{Cursor, Write};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use oo_mindmap::parse_mindmap_exchange;
use oo_schema::presentation_v5::{
    AnimationEntry, AnimationPreset, AnimationTrigger, AssetRef, DeckTheme, ImageNode,
    SceneNodeKind, SlideTransition, TransitionKind,
};
use oo_schema::{
    ArtifactEnvelope, ArtifactPayload, CellModel, SheetMetadata, SheetModel, SpreadsheetMetadata,
    SpreadsheetModel, CURRENT_SCHEMA_VERSION,
};
use oo_server::store::{BlobStore, LocalFsStore};
use oo_server::{build_router, db, AppState};
use serde_json::{json, Value};
use tower::ServiceExt;
use zip::write::SimpleFileOptions;

struct TestApp {
    router: axum::Router,
    dir: std::path::PathBuf,
    pool: sqlx::SqlitePool,
    store: Arc<LocalFsStore>,
}

impl TestApp {
    async fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("oo-artifact-api-{}", uuid::Uuid::new_v4()));
        let pool = db::connect("sqlite::memory:").await.unwrap();
        let store = Arc::new(LocalFsStore::new(&dir).await.unwrap());
        Self {
            router: build_router(AppState {
                pool: pool.clone(),
                store: store.clone(),
                write_lock: Arc::new(tokio::sync::Mutex::new(())),
                presence: Arc::new(tokio::sync::Mutex::new(
                    oo_server::presence::PresenceStore::default(),
                )),
                // 这套测试不声明身份，跑在生产默认配置下（`X-OO-User` 不被信任）。
                trust_user_header: false,
            }),
            dir,
            pool,
            store,
        }
    }

    async fn json(&self, request: Request<Body>) -> (StatusCode, Value) {
        let (status, bytes) = self.send(request).await;
        let value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            panic!(
                "响应不是 JSON：{error}；内容：{}",
                String::from_utf8_lossy(&bytes)
            )
        });
        (status, value)
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, Vec<u8>) {
        let (status, _, bytes) = self.send_with_headers(request).await;
        (status, bytes)
    }

    async fn send_with_headers(&self, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, headers, bytes.to_vec())
    }

    async fn upload_bytes(&self, file_name: &str, bytes: &[u8]) -> (StatusCode, Value) {
        self.upload_bytes_with_mode(file_name, bytes, "audit").await
    }

    async fn upload_bytes_with_mode(
        &self,
        file_name: &str,
        bytes: &[u8],
        mode: &str,
    ) -> (StatusCode, Value) {
        const BOUNDARY: &str = "----oo-docx-media";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(
            format!(
                "\r\n--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"mode\"\r\n\r\n{mode}\r\n--{BOUNDARY}--\r\n"
            )
            .as_bytes(),
        );
        self.json(
            Request::post("/api/artifacts/import")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    }

    /// Upload one artifact-scoped binary using the same multipart boundary as
    /// the browser SDK. Presentation transactions may only register assets
    /// returned by this route; tests should never seed an unverified asset row
    /// to bypass that contract.
    async fn upload_artifact_asset(
        &self,
        artifact_id: &str,
        file_name: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> (StatusCode, Value) {
        const BOUNDARY: &str = "----oo-presentation-asset";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: {content_type}\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
        self.json(
            Request::post(format!("/api/artifacts/{artifact_id}/assets"))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    }
}

async fn seed_presentation_fixture(app: &TestApp, id: &str) {
    let deck = serde_json::from_str(include_str!(
        "../../../fixtures/presentation/v5/minimal-deck.json"
    ))
    .expect("presentation fixture must parse");
    let mut artifact = ArtifactEnvelope::new(id, ArtifactPayload::Presentation(deck));
    artifact.revision = 1;
    artifact.validate().expect("fixture envelope must validate");
    let keys = db::get_artifact_blob_keys(&app.pool, id)
        .await
        .unwrap()
        .expect("created artifact has keys");
    app.store
        .put(&keys.snapshot_key, &serde_json::to_vec(&artifact).unwrap())
        .await
        .unwrap();
}

impl Drop for TestApp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// Build an application against durable local storage rather than the normal
/// in-memory test database.  Keeping this outside `TestApp` is intentional:
/// a restart must drop the complete router and its connection pool before the
/// second server instance is constructed.
async fn persistent_router(data_dir: &std::path::Path) -> axum::Router {
    std::fs::create_dir_all(data_dir).unwrap();
    let database_url = format!("sqlite://{}", data_dir.join("open-office.db").display());
    let pool = db::connect(&database_url).await.unwrap();
    let store = Arc::new(LocalFsStore::new(data_dir.join("blobs")).await.unwrap());
    build_router(AppState {
        pool,
        store,
        write_lock: Arc::new(tokio::sync::Mutex::new(())),
        presence: Arc::new(tokio::sync::Mutex::new(
            oo_server::presence::PresenceStore::default(),
        )),
        trust_user_header: false,
    })
}

async fn router_json(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "响应不是 JSON：{error}；内容：{}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, value)
}

fn docx_with_image() -> Vec<u8> {
    let document = br#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:r><w:drawing><a:blip xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" r:embed="rId5"/></w:drawing></w:r></w:p></w:body></w:document>"#;
    let relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let image = b"png-bytes";
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in [
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", relationships.as_slice()),
        ("word/media/image1.png", image.as_slice()),
    ] {
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(content).unwrap();
    }
    archive.finish().unwrap().into_inner()
}

fn xmind_with_image() -> Vec<u8> {
    let content = include_bytes!("../../../fixtures/mindmap/xmind/content.json");
    let image = b"xmind-image-bytes";
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("content.json", content.as_slice()),
        ("resources/topic.png", image.as_slice()),
    ] {
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap().into_inner()
}

fn pptx_with_image() -> Vec<u8> {
    let mut deck: oo_schema::presentation_v5::Deck = serde_json::from_str(include_str!(
        "../../../fixtures/presentation/v5/minimal-deck.json"
    ))
    .unwrap();
    deck.masters.clear();
    deck.layouts.clear();
    deck.theme = DeckTheme {
        id: "strict-pptx-test".into(),
        ..DeckTheme::default()
    };
    deck.slides[0].name.clear();
    deck.slides[0].layout_id = None;
    deck.slides[0].notes = Some("server roundtrip note".into());
    deck.slides[0].transition = Some(SlideTransition {
        kind: TransitionKind::Fade,
        duration_ms: 650,
    });
    deck.assets.push(AssetRef {
        asset_id: "image-asset".into(),
        digest: "fixture-digest".into(),
        mime_type: "image/png".into(),
        width: None,
        height: None,
        original_asset_id: None,
    });
    let image = &mut deck.slides[0].nodes[0];
    image.id = "image-1".into();
    image.layout_placeholder_id = None;
    image.kind = SceneNodeKind::Image(ImageNode {
        asset_id: "image-asset".into(),
        original_asset_id: None,
        crop: Default::default(),
        flip_h: false,
        flip_v: false,
        caption: None,
    });
    let image_id = image.id.clone();
    deck.slides[0].timeline.entries = vec![AnimationEntry {
        id: "image-fade".into(),
        target_node_id: image_id,
        trigger: AnimationTrigger::OnClick,
        preset: AnimationPreset::Fade,
        duration_ms: 500,
        delay_ms: 75,
        order_key: "00000000".into(),
    }];
    let mut assets = oo_pptx::PptxAssetSource::new();
    // The PPTX writer only emits media relationships for recognized image bytes. A minimal
    // PNG signature is enough here: this test verifies artifact asset ownership, not decoding.
    assets.insert(
        "image-asset".into(),
        b"\x89PNG\r\n\x1a\npptx-image-bytes".to_vec(),
    );
    let exported = oo_pptx::write_pptx_with_assets(&deck, &assets).unwrap();
    assert!(exported.loss_report.unsupported.is_empty());
    exported.bytes
}

#[tokio::test]
async fn presentation_transactions_and_read_projections_are_revision_safe() {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "presentation", "title": "Deck API"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_owned();
    seed_presentation_fixture(&app, &id).await;

    let (status, capabilities) = app
        .json(
            Request::get("/api/capabilities")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let presentation = capabilities["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "presentation")
        .unwrap();
    assert_eq!(presentation["features"]["edit"], "stable");
    assert_eq!(presentation["features"]["presence"], "preview");
    assert!(presentation["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["typeId"] == "presentation.setSlideNotes"));
    assert!(presentation["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["typeId"] == "presentation.duplicateSlide"));
    assert!(presentation["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["typeId"] == "presentation.setConnectorEndpoints"));
    assert!(presentation["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["typeId"] == "presentation.setChartSpec"));
    for command_type in [
        "presentation.createMaster",
        "presentation.updateMaster",
        "presentation.deleteMaster",
        "presentation.createLayout",
        "presentation.updateLayout",
        "presentation.deleteLayout",
    ] {
        assert!(presentation["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| command["typeId"] == command_type));
    }
    assert!(presentation["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command["typeId"] == "presentation.history"));

    let (status, headers, bytes) = app
        .send_with_headers(
            Request::get(format!("/api/artifacts/{id}/projection/presentation"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("etag").unwrap(), "\"1\"");
    assert_eq!(headers.get("x-artifact-revision").unwrap(), "1");
    let deck = serde_json::from_slice::<Value>(&bytes).unwrap();
    assert_eq!(deck["projection"], "presentation");
    assert_eq!(deck["data"]["slideCount"], 1);
    assert_eq!(deck["data"]["layouts"][0]["id"], "layout-title");
    assert_eq!(deck["data"]["layouts"][0]["masterId"], "master-default");

    let (status, _, _) = app
        .send_with_headers(
            Request::get(format!("/api/artifacts/{id}/projection/presentation"))
                .header("if-none-match", "\"1\"")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);

    let (status, outline) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/presentation/outline?limit=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outline["projection"], "presentationOutline");
    assert_eq!(outline["data"]["items"][0]["slideId"], "slide-1");

    let (status, slide) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/presentation/slides/slide-1?include=nodes,notes,timeline"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(slide["projection"], "presentationSlide");
    assert_eq!(slide["data"]["nodes"][0]["id"], "title-1");
    assert!(slide["data"].get("transition").is_some());

    let (status, node) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/presentation/slides/slide-1/nodes/title-1"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(node["projection"], "presentationNode");
    assert_eq!(node["data"]["node"]["id"], "title-1");

    let transaction = json!({
        "protocolVersion": 1,
        "transactionId": "presentation-notes-1",
        "intentId": "presentation-notes-intent-1",
        "artifactId": id.clone(),
        "actorId": "local-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "presentation-notes-command-1",
            "typeId": "presentation.setSlideNotes",
            "payload": {"type": "setSlideNotes", "slideId": "slide-1", "notes": "Agent-ready notes"}
        }]
    });
    let (status, committed) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "presentation-notes-1")
                .body(Body::from(transaction.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "response: {committed}");
    assert_eq!(committed["revision"], 2);
    assert_eq!(committed["canUndo"], true);
    assert_eq!(committed["canRedo"], false);
    assert_eq!(
        committed["mutations"][0]["typeId"],
        "presentation.slideNotesSet"
    );

    // Lost-response retries use the transaction id before checking a stale
    // If-Match and never append a second event or advance revision.
    let (status, replay) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "presentation-notes-1")
                .body(Body::from(transaction.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["revision"], 2);
    assert!(replay["mutations"].as_array().unwrap().is_empty());

    let (status, slide_after_write) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/presentation/slides/slide-1?include=notes"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(slide_after_write["revision"], 2);
    assert_eq!(slide_after_write["data"]["notes"], "Agent-ready notes");

    let undo = json!({
        "protocolVersion": 1,
        "transactionId": "presentation-notes-undo-1",
        "intentId": "presentation-notes-undo-intent-1",
        "artifactId": id.clone(),
        "actorId": "local-user",
        "baseRevision": 2,
        "origin": "undo",
        "commands": [{
            "commandId": "presentation-notes-undo-command-1",
            "typeId": "presentation.history",
            "payload": {"action": "undo"}
        }]
    });
    let (status, undone) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"2\"")
                .header("x-transaction-id", "presentation-notes-undo-1")
                .body(Body::from(undo.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "response: {undone}");
    assert_eq!(undone["revision"], 3);
    assert_eq!(undone["canUndo"], false);
    assert_eq!(undone["canRedo"], true);
    assert_eq!(
        undone["mutations"][0]["typeId"],
        "presentation.slideNotesSet"
    );
    assert!(undone["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["typeId"] == "presentation.historyApplied"));

    let (status, slide_after_undo) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/presentation/slides/slide-1?include=notes"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(slide_after_undo["revision"], 3);
    assert!(slide_after_undo["data"]["notes"].is_null());

    // The same undo transaction must replay as an idempotent response rather
    // than consuming another history entry.
    let (status, undo_retry) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"2\"")
                .header("x-transaction-id", "presentation-notes-undo-1")
                .body(Body::from(undo.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(undo_retry["revision"], 3);
    assert!(undo_retry["events"].as_array().unwrap().is_empty());

    let redo = json!({
        "protocolVersion": 1,
        "transactionId": "presentation-notes-redo-1",
        "intentId": "presentation-notes-redo-intent-1",
        "artifactId": id.clone(),
        "actorId": "local-user",
        "baseRevision": 3,
        "origin": "redo",
        "commands": [{
            "commandId": "presentation-notes-redo-command-1",
            "typeId": "presentation.history",
            "payload": {"action": "redo"}
        }]
    });
    let (status, redone) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"3\"")
                .header("x-transaction-id", "presentation-notes-redo-1")
                .body(Body::from(redo.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "response: {redone}");
    assert_eq!(redone["revision"], 4);
    assert_eq!(redone["canUndo"], true);
    assert_eq!(redone["canRedo"], false);
    assert!(redone["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["typeId"] == "presentation.historyApplied"));

    let (status, slide_after_redo) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/presentation/slides/slide-1?include=notes"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(slide_after_redo["revision"], 4);
    assert_eq!(slide_after_redo["data"]["notes"], "Agent-ready notes");

    let (status, history_events) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/events?sinceRevision=2"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(history_events["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["revision"] == 3 && event["typeId"] == "presentation.historyApplied"));
    assert!(history_events["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["revision"] == 4 && event["typeId"] == "presentation.historyApplied"));

    let (status, error) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/presentation/slides/slide-1?include=bogus"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "bad_request");
}

#[tokio::test]
async fn presentation_master_and_layout_commands_are_capability_gated_and_typed() {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "presentation", "title": "Master commands"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_owned();
    seed_presentation_fixture(&app, &id).await;
    let master = json!({
        "id": "master-default", "name": "Updated default", "background": {"type": "none"},
        "placeholders": [{
            "id": "master-title", "kind": "title",
            "transform": {"x": 720000, "y": 480000, "width": 7200000, "height": 900000, "rotation": 0},
            "defaultText": null
        }]
    });
    let transaction = json!({
        "protocolVersion": 1,
        "transactionId": "presentation-master-update-1",
        "intentId": "presentation-master-update-intent-1",
        "artifactId": id.clone(),
        "actorId": "local-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "presentation-master-update-command-1",
            "typeId": "presentation.updateMaster",
            "payload": {"type": "updateMaster", "master": master}
        }]
    });
    let (status, committed) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "presentation-master-update-1")
                .body(Body::from(transaction.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "response: {committed}");
    assert_eq!(committed["revision"], 2);
    assert_eq!(
        committed["mutations"][0]["typeId"],
        "presentation.masterUpdated"
    );
}

#[tokio::test]
async fn presentation_duplicate_slide_rewrites_id_references_server_side() {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "presentation", "title": "Duplicate"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_owned();
    seed_presentation_fixture(&app, &id).await;

    let transaction = json!({
        "protocolVersion": 1,
        "transactionId": "presentation-duplicate-1",
        "intentId": "presentation-duplicate-intent-1",
        "artifactId": id.clone(),
        "actorId": "local-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "presentation-duplicate-command-1",
            "typeId": "presentation.duplicateSlide",
            "payload": {
                "type": "duplicateSlide",
                "sourceSlideId": "slide-1",
                "slideId": "slide-2",
                "orderKey": "00000001",
                "name": "Introduction 副本",
                "nodeIdMap": [
                    {"sourceId": "title-1", "targetId": "title-2"},
                    {"sourceId": "shape-1", "targetId": "shape-2"}
                ],
                "animationIdMap": [],
                "index": 1
            }
        }]
    });
    let (status, committed) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "presentation-duplicate-1")
                .body(Body::from(transaction.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "response: {committed}");
    assert_eq!(committed["revision"], 2);
    assert_eq!(
        committed["mutations"][0]["typeId"],
        "presentation.slideDuplicated"
    );
    assert!(committed["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |event| event["typeId"] == "presentation.thumbnailInvalidated"
                && event["payload"]["slideId"] == "slide-2"
        ));

    let (status, slide) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/presentation/slides/slide-2?include=nodes,timeline"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "response: {slide}");
    assert_eq!(slide["data"]["name"], "Introduction 副本");
    assert_eq!(slide["data"]["nodes"][0]["id"], "title-2");
    assert_eq!(slide["data"]["nodes"][1]["id"], "shape-2");
}

#[tokio::test]
async fn presentation_image_asset_registration_requires_verified_binary_metadata_and_commits_atomically(
) {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "presentation", "title": "Verified image asset"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_owned();
    seed_presentation_fixture(&app, &id).await;

    let (status, uploaded) = app
        .upload_artifact_asset(&id, "diagram.png", "image/png", b"verified-image-bytes")
        .await;
    assert_eq!(status, StatusCode::CREATED, "response: {uploaded}");
    let asset_id = uploaded["assetId"].as_str().unwrap().to_owned();
    let digest = uploaded["checksum"].as_str().unwrap().to_owned();
    assert_eq!(
        db::get_artifact_asset(&app.pool, &id, &asset_id)
            .await
            .unwrap()
            .unwrap()
            .ref_count,
        0
    );

    let node = |asset_id: &str| {
        json!({
            "id": "verified-image-1",
            "parentId": null,
            "orderKey": "00000002",
            "name": "Verified image",
            "altText": null,
            "layoutPlaceholderId": null,
            "transform": {"x": 1000000.0, "y": 1000000.0, "width": 4000000.0, "height": 3000000.0, "rotation": 0.0},
            "visible": true,
            "locked": false,
            "opacity": 1.0,
            "kind": {
                "type": "image",
                "data": {
                    "assetId": asset_id,
                    "originalAssetId": null,
                    "crop": {"top": 0.0, "right": 0.0, "bottom": 0.0, "left": 0.0},
                    "flipH": false,
                    "flipV": false,
                    "caption": null
                }
            }
        })
    };
    let transaction = |transaction_id: &str,
                       requested_asset_id: &str,
                       asset_digest: &str,
                       asset_mime_type: &str| {
        json!({
            "protocolVersion": 1,
            "transactionId": transaction_id,
            "intentId": format!("{transaction_id}-intent"),
            "artifactId": id,
            "actorId": "local-user",
            "baseRevision": 1,
            "origin": "local",
            "commands": [
                {
                    "commandId": format!("{transaction_id}-asset"),
                    "typeId": "presentation.registerAsset",
                    "payload": {
                        "type": "registerAsset",
                        "asset": {
                            "assetId": requested_asset_id,
                            "digest": asset_digest,
                            "mimeType": asset_mime_type,
                            "width": null,
                            "height": null,
                            "originalAssetId": null
                        }
                    }
                },
                {
                    "commandId": format!("{transaction_id}-node"),
                    "typeId": "presentation.insertNode",
                    "payload": {"type": "insertNode", "slideId": "slide-1", "node": node(requested_asset_id), "index": 2}
                }
            ]
        })
    };

    // Neither a forged digest nor a forged MIME may advance the revision,
    // persist a Deck asset declaration, or claim the stored binary.
    for (transaction_id, requested_asset_id, forged_digest, forged_mime) in [
        (
            "presentation-image-missing-asset",
            "missing-image-asset",
            "missing-digest",
            "image/png",
        ),
        (
            "presentation-image-wrong-digest",
            asset_id.as_str(),
            "forged-digest",
            "image/png",
        ),
        (
            "presentation-image-wrong-mime",
            asset_id.as_str(),
            digest.as_str(),
            "image/jpeg",
        ),
    ] {
        let (status, error) = app
            .json(
                Request::post(format!("/api/artifacts/{id}/transactions"))
                    .header("content-type", "application/json")
                    .header("if-match", "\"1\"")
                    .header("x-transaction-id", transaction_id)
                    .body(Body::from(
                        transaction(
                            transaction_id,
                            requested_asset_id,
                            forged_digest,
                            forged_mime,
                        )
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "response: {error}");
        assert_eq!(error["code"], "bad_request");
        assert_eq!(
            db::get_artifact(&app.pool, &id)
                .await
                .unwrap()
                .unwrap()
                .version,
            1
        );
        assert_eq!(
            db::get_artifact_asset(&app.pool, &id, &asset_id)
                .await
                .unwrap()
                .unwrap()
                .ref_count,
            0
        );
    }

    let (status, committed) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "presentation-image-success")
                .body(Body::from(
                    transaction(
                        "presentation-image-success",
                        &asset_id,
                        &digest,
                        "image/png",
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "response: {committed}");
    assert_eq!(committed["revision"], 2);
    assert_eq!(
        db::get_artifact_asset(&app.pool, &id, &asset_id)
            .await
            .unwrap()
            .unwrap()
            .ref_count,
        1
    );

    let (status, snapshot) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/snapshot"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let deck = &snapshot["artifact"]["payload"]["data"];
    assert_eq!(deck["assets"][0]["assetId"], asset_id);
    assert_eq!(deck["assets"][0]["digest"], digest);
    assert_eq!(
        deck["slides"][0]["nodes"][2]["kind"]["data"]["assetId"],
        asset_id
    );

    let history_transaction = |action: &str, base_revision: u64| {
        let transaction_id = format!("presentation-image-{action}");
        json!({
            "protocolVersion": 1,
            "transactionId": transaction_id,
            "intentId": format!("presentation-image-{action}-intent"),
            "artifactId": id,
            "actorId": "local-user",
            "baseRevision": base_revision,
            "origin": action,
            "commands": [{
                "commandId": format!("presentation-image-{action}-command"),
                "typeId": "presentation.history",
                "payload": {"action": action}
            }]
        })
    };
    for (action, base_revision, expected_ref_count) in
        [("undo", 2_u64, 0_i64), ("redo", 3_u64, 1_i64)]
    {
        let transaction_id = format!("presentation-image-{action}");
        let (status, history_result) = app
            .json(
                Request::post(format!("/api/artifacts/{id}/transactions"))
                    .header("content-type", "application/json")
                    .header("if-match", format!("\"{base_revision}\""))
                    .header("x-transaction-id", &transaction_id)
                    .body(Body::from(
                        history_transaction(action, base_revision).to_string(),
                    ))
                    .unwrap(),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "response: {history_result}");
        assert_eq!(
            db::get_artifact_asset(&app.pool, &id, &asset_id)
                .await
                .unwrap()
                .unwrap()
                .ref_count,
            expected_ref_count
        );
    }

    let (status, protected) = app
        .json(
            Request::delete(format!("/api/artifacts/{id}/assets/{asset_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "response: {protected}");
    assert!(protected["error"]
        .as_str()
        .unwrap()
        .contains("仍被 snapshot 引用"));
}

#[tokio::test]
async fn presentation_created_before_restart_retains_v5_deck_and_read_routes() {
    let data_dir =
        std::env::temp_dir().join(format!("oo-presentation-restart-{}", uuid::Uuid::new_v4()));

    let first_server = persistent_router(&data_dir).await;
    let (status, created) = router_json(
        &first_server,
        Request::post("/api/artifacts")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"kind": "presentation", "title": "Restart-safe deck"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "response: {created}");
    let id = created["id"]
        .as_str()
        .expect("presentation artifact id")
        .to_owned();

    // A process restart must recreate both the SQLite pool and blob store. Do
    // not reuse an AppState here: this is the failure mode that previously
    // produced the legacy unsupported-engine fallback in the editor.
    drop(first_server);
    let restarted_server = persistent_router(&data_dir).await;

    let (status, snapshot) = router_json(
        &restarted_server,
        Request::get(format!("/api/artifacts/{id}/snapshot"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "response: {snapshot}");
    assert_eq!(
        snapshot["artifact"]["schemaVersion"],
        CURRENT_SCHEMA_VERSION
    );
    assert_eq!(snapshot["artifact"]["kind"], "presentation");
    assert_eq!(snapshot["artifact"]["payload"]["kind"], "presentation");
    assert_eq!(
        snapshot["artifact"]["payload"]["data"]["theme"]["id"],
        "default"
    );

    let (status, projection) = router_json(
        &restarted_server,
        Request::get(format!("/api/artifacts/{id}/projection/presentation"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "response: {projection}");
    assert_eq!(projection["projection"], "presentation");
    assert_eq!(projection["data"]["slideCount"], 0);

    let (status, outline) = router_json(
        &restarted_server,
        Request::get(format!("/api/artifacts/{id}/presentation/outline"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "response: {outline}");
    assert_eq!(outline["projection"], "presentationOutline");
    assert_eq!(outline["data"]["items"], json!([]));

    // History state is artifact-level metadata. Presentation must not be
    // rejected by the old Document-only history guard after a cold restart.
    let (status, history) = router_json(
        &restarted_server,
        Request::get(format!("/api/artifacts/{id}/history"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "response: {history}");
    assert_eq!(history, json!({"canUndo": false, "canRedo": false}));

    drop(restarted_server);
    std::fs::remove_dir_all(data_dir).ok();
}

#[tokio::test]
async fn artifact_events_feed_is_revision_ordered_and_replayable() {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "document", "title": "Events"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();

    let (_, snapshot) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/snapshot"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    let block_id = snapshot["artifact"]["payload"]["data"]["blocks"][0]["id"]
        .as_str()
        .unwrap();
    let transaction = json!({
        "protocolVersion": 1,
        "transactionId": "events-transaction",
        "intentId": "events-intent",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "events-command",
            "typeId": "document.replaceBlockText",
            "payload": {
                "type": "replaceBlockText",
                "blockId": block_id,
                "content": {"text": "event revision", "runs": []}
            }
        }]
    });
    let (status, _) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "events-transaction")
                .body(Body::from(transaction.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, page) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/events?sinceRevision=0&limit=1"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{page}");
    assert_eq!(page["artifactId"], id);
    assert_eq!(page["revision"], 2);
    assert_eq!(page["events"][0]["typeId"], "artifact.created");
    assert!(page["events"][0].get("status").is_none());
    assert!(page["events"][0].get("claimedBy").is_none());
    let next_cursor = page["nextCursor"].as_str().expect("next event cursor");

    let (status, next_page) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/events?cursor={next_cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(next_page["events"][0]["typeId"], "document.blockUpdated");

    let (status, empty) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/events?sinceRevision=2"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(empty["events"].as_array().unwrap().is_empty());

    let (status, invalid) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/events?cursor=bad"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid["code"], "bad_request");
}

#[tokio::test]
async fn canonical_collection_and_document_creation_use_artifact_shape() {
    let app = TestApp::new().await;

    let (status, body) = app
        .json(Request::get("/api/artifacts").body(Body::empty()).unwrap())
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["artifacts"], json!([]));

    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "document", "title": "Canonical"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "响应：{created}");
    assert_eq!(created["kind"], "document");
    assert_eq!(created["title"], "Canonical");

    let id = created["id"].as_str().expect("created artifact id");
    let (status, meta) = app
        .json(
            Request::get(format!("/api/artifacts/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(meta["kind"], "document");

    let (status, snapshot) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/snapshot"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot["artifact"]["artifactId"], id);
    assert_eq!(snapshot["artifact"]["payload"]["kind"], "document");

    let (status, source) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/source"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(source.is_empty(), "blank document has no source bytes");

    let (status, exported) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/export/docx"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "导出响应：{}",
        String::from_utf8_lossy(&exported)
    );
    assert!(
        !exported.is_empty(),
        "exported blank document is a valid docx"
    );

    let (status, _) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/export/pdf"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn asset_routes_verify_bytes_and_delete_unreferenced() {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kind": "document"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();

    const BOUNDARY: &str = "----oo-asset-contract";
    let bytes = b"asset-bytes";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\nContent-Type: text/plain\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    let (status, asset) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/assets"))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={BOUNDARY}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "响应：{asset}");
    assert_eq!(asset["fileName"], "hello.txt");
    assert_eq!(asset["contentType"], "text/plain");
    assert_eq!(asset["size"], bytes.len());
    assert_eq!(asset["refCount"], 0);
    let asset_id = asset["assetId"].as_str().unwrap();

    let (status, listed) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/assets"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["assets"].as_array().unwrap().len(), 1);

    let (status, downloaded) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/assets/{asset_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(downloaded, bytes);

    let (status, _) = app
        .send(
            Request::delete(format!("/api/artifacts/{id}/assets/{asset_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/assets/{asset_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, corruptible) = app
        .upload_artifact_asset(id, "corrupt.bin", "application/octet-stream", b"verified")
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let corruptible_id = corruptible["assetId"].as_str().unwrap();
    tokio::fs::write(
        app.dir.join(format!("{id}/assets/{corruptible_id}")),
        b"tampered",
    )
    .await
    .unwrap();
    let (status, _) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/assets/{corruptible_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn docx_import_registers_media_as_referenced_assets() {
    let app = TestApp::new().await;
    let (status, imported) = app
        .upload_bytes("with-image.docx", &docx_with_image())
        .await;
    assert_eq!(status, StatusCode::CREATED, "响应：{imported}");
    let id = imported["id"].as_str().unwrap();
    let (status, listed) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/assets"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{listed}");
    let assets = listed["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0]["fileName"], "image1.png");
    assert_eq!(assets[0]["contentType"], "image/png");
    assert_eq!(assets[0]["refCount"], 1);
    let asset_id = assets[0]["assetId"].as_str().unwrap();
    let (status, bytes) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/assets/{asset_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"png-bytes");
    let (status, exported) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/export/docx"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let roundtrip = oo_docx::parse_docx_with_assets(&exported, "exported-image")
        .expect("图片资产应随 DOCX 导出并可重新导入");
    assert_eq!(roundtrip.assets.len(), 1);
    assert_eq!(roundtrip.assets[0].bytes, b"png-bytes");
    let (status, _) = app
        .send(
            Request::delete(format!("/api/artifacts/{id}/assets/{asset_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn pptx_import_persists_media_assets_and_export_resolves_them() {
    let app = TestApp::new().await;
    let source_bytes = pptx_with_image();
    let source = oo_pptx::parse_pptx_with_report(&source_bytes).unwrap();
    assert!(source.loss_report.unsupported.is_empty());
    let (status, imported) = app.upload_bytes("with-image.pptx", &source_bytes).await;
    assert_eq!(status, StatusCode::CREATED, "响应：{imported}");
    let id = imported["id"].as_str().unwrap();
    let (status, listed) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/assets"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{listed}");
    let assets = listed["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0]["contentType"], "image/png");
    assert_eq!(assets[0]["refCount"], 1);
    let asset_id = assets[0]["assetId"].as_str().unwrap();
    let (status, bytes) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/assets/{asset_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"\x89PNG\r\n\x1a\npptx-image-bytes");

    let (status, exported) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/export/pptx"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "导出响应：{}",
        String::from_utf8_lossy(&exported)
    );
    let roundtrip = oo_pptx::parse_pptx_with_report(&exported).unwrap();
    assert!(roundtrip.loss_report.unsupported.is_empty());
    assert!(oo_pptx::semantic_diff(&source.deck, &roundtrip.deck).is_equivalent());
    assert_eq!(
        roundtrip.deck.slides[0].notes.as_deref(),
        Some("server roundtrip note")
    );
    assert_eq!(
        roundtrip.deck.slides[0]
            .transition
            .as_ref()
            .unwrap()
            .duration_ms,
        650
    );
    assert_eq!(roundtrip.deck.slides[0].timeline.entries.len(), 1);
    assert_eq!(roundtrip.assets.len(), 1);
    assert_eq!(
        roundtrip.assets[0].bytes,
        b"\x89PNG\r\n\x1a\npptx-image-bytes"
    );
}

#[tokio::test]
async fn document_projections_are_bounded_and_keep_canonical_block_refs() {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kind": "document"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();

    let transaction = json!({
        "protocolVersion": 1,
        "transactionId": "toc-heading-transaction",
        "intentId": "toc-heading-intent",
        "artifactId": id,
        "actorId": "browser-client",
        "baseRevision": 1,
        "origin": "local",
        "commands": [
            {
                "commandId": "toc-heading-kind",
                "typeId": "document.convertBlock",
                "payload": {"type": "convertBlock", "blockId": "block-1", "kind": {"type": "heading", "level": 2}}
            },
            {
                "commandId": "toc-heading-text",
                "typeId": "document.replaceBlockText",
                "payload": {"type": "replaceBlockText", "blockId": "block-1", "content": {"text": "路线图 😀", "runs": []}}
            }
        ]
    });
    let (status, commit) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "toc-heading-transaction")
                .body(Body::from(transaction.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");

    let (status, toc) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/toc?limit=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{toc}");
    assert_eq!(toc["projection"], "tableOfContents");
    assert_eq!(toc["revision"], 2);
    assert_eq!(toc["data"]["items"][0]["blockId"], "block-1");
    assert_eq!(toc["data"]["items"][0]["level"], 2);
    assert_eq!(toc["data"]["items"][0]["text"], "路线图 😀");

    let (status, outline) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/outline?include=headingPath"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{outline}");
    assert_eq!(outline["projection"], "outline");
    assert_eq!(outline["data"]["items"][0]["blockId"], "block-1");
    assert_eq!(outline["data"]["items"][0]["kind"], "heading");
    assert!(outline["data"]["items"][0]["parentId"].is_null());

    let (status, block_refs) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/blocks?include=headingPath&limit=1"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{block_refs}");
    assert_eq!(block_refs["projection"], "block");
    assert_eq!(block_refs["data"]["items"][0]["blockId"], "block-1");
    assert!(block_refs["data"]["items"][0]["headingPath"].is_array());

    let (status, invalid_limit) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/blocks?limit=0"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_limit["code"], "bad_request");

    let (status, block) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/blocks/block-1?include=content,refs"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{block}");
    assert_eq!(block["projection"], "block");
    assert_eq!(block["data"]["blockId"], "block-1");
    assert!(block["data"]["content"].is_object());
    assert_eq!(block["data"]["refs"][0]["artifactId"], id);
    assert_eq!(block["data"]["refs"][0]["blockId"], "block-1");

    let (status, invalid_include) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/outline?include=payload"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_include["code"], "bad_request");

    let (status, invalid_budget) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/outline?maxBytes=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_budget["code"], "bad_request");

    let (status, exceeded) = app
        .json(
            Request::get(format!(
                "/api/artifacts/{id}/blocks/block-1?include=content,refs&maxBytes=256"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(exceeded["code"], "projection_budget_exceeded");
    assert_eq!(exceeded["maxBytes"], 256);
}

#[tokio::test]
async fn canonical_create_persists_each_artifact_kind_without_downgrade() {
    let app = TestApp::new().await;
    for (kind, title) in [
        ("spreadsheet", "Sheet"),
        ("presentation", "Deck"),
        ("mindmap", "Map"),
        ("whiteboard", "Board"),
    ] {
        let (status, body) = app
            .json(
                Request::post("/api/artifacts")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"kind": kind, "title": title}).to_string(),
                    ))
                    .unwrap(),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "响应：{body}");
        assert_eq!(body["kind"], kind);
        let id = body["id"].as_str().expect("artifact id");
        let (status, snapshot) = app
            .json(
                Request::get(format!("/api/artifacts/{id}/snapshot"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(snapshot["artifact"]["kind"], kind);
        assert_eq!(snapshot["artifact"]["payload"]["kind"], kind);
    }
}

#[tokio::test]
async fn canonical_import_and_export_dispatch_by_artifact_kind() {
    let app = TestApp::new().await;
    let model = SpreadsheetModel {
        metadata: SpreadsheetMetadata::default(),
        sheets: vec![SheetModel {
            id: "sheet-1".into(),
            name: "Sheet 1".into(),
            cells: vec![CellModel {
                row: 1,
                column: 1,
                value: Some(Value::String("hello".into())),
                formula: None,
                attrs: Default::default(),
                style: None,
            }],
            metadata: SheetMetadata::default(),
        }],
    };
    let bytes = oo_xlsx::write_xlsx(&model).unwrap();
    let boundary = "----ooartifact";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"table.xlsx\"\r\nContent-Type: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let (status, imported) = app
        .json(
            Request::post("/api/artifacts/import")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "响应：{imported}");
    assert_eq!(imported["kind"], "spreadsheet");
    let id = imported["id"].as_str().expect("spreadsheet id");

    let (status, snapshot) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/snapshot"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot["artifact"]["payload"]["kind"], "spreadsheet");

    let (status, exported) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/export/xlsx"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(exported.starts_with(b"PK"));
}

#[tokio::test]
async fn presentation_presence_is_ephemeral_and_never_creates_a_transaction() {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"id": "presence-deck", "kind": "presentation", "title": "Presence"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();
    seed_presentation_fixture(&app, id).await;
    let payload = json!({
        "slideId": "slide-1",
        "selectedNodeIds": ["node-1"],
        "cursor": { "x": 42.5, "y": 21.0 }
    });
    let (status, body) = app
        .send(
            Request::put(format!("/api/artifacts/{id}/presence/browser-1"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "{}",
        String::from_utf8_lossy(&body)
    );

    let (status, page) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/presence"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["participants"][0]["slideId"], "slide-1");
    assert_eq!(page["participants"][0]["cursor"]["x"], 42.5);
    assert!(db::get_artifact_history(&app.pool, id, false)
        .await
        .unwrap()
        .is_none());
    assert!(db::get_artifact_history(&app.pool, id, true)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn mindmap_markdown_json_and_verified_assets_round_trip() {
    let app = TestApp::new().await;
    let markdown = b"# Product roadmap\n\n> Shared note\n\n## Milestone one\n\n## Milestone two\n";
    let (status, imported) = app.upload_bytes("roadmap.md", markdown).await;
    assert_eq!(status, StatusCode::CREATED, "response: {imported}");
    assert_eq!(imported["kind"], "mindmap");
    let id = imported["id"].as_str().unwrap();

    let (status, exported_markdown) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/export/md"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let exported_markdown = String::from_utf8(exported_markdown).unwrap();
    assert!(exported_markdown.contains("# Product roadmap"));
    assert!(exported_markdown.contains("> Shared note"));

    let (status, json_headers, exported_json) = app
        .send_with_headers(
            Request::get(format!("/api/artifacts/{id}/export/json"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json_headers["content-type"],
        "application/vnd.open-office.mindmap+json"
    );
    assert!(json_headers["content-disposition"]
        .to_str()
        .unwrap()
        .contains(".mindmap.json"));
    let package = parse_mindmap_exchange(&exported_json).unwrap();
    assert_eq!(package.format, "open-office-mindmap");
    assert!(package.assets.is_empty());
    let (status, json_copy) = app
        .upload_bytes("roadmap.mindmap.json", &exported_json)
        .await;
    assert_eq!(status, StatusCode::CREATED, "response: {json_copy}");
    assert_eq!(json_copy["kind"], "mindmap");

    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "mindmap", "title": "Images"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let image_map_id = created["id"].as_str().unwrap();
    let (status, asset) = app
        .upload_artifact_asset(image_map_id, "topic.png", "image/png", b"verified-image")
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let asset_id = asset["assetId"].as_str().unwrap();
    let transaction_id = "mindmap-image-1";
    let transaction = json!({
        "protocolVersion": 1,
        "transactionId": transaction_id,
        "intentId": "mindmap-image-intent",
        "artifactId": image_map_id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [
            {"commandId": "add-root", "typeId": "mindmap.addNode", "payload": {
                "type": "addNode", "nodeId": "root", "parentId": null,
                "content": {"text": "Image topic", "runs": []}, "attrs": {}, "index": 0
            }},
            {"commandId": "set-image", "typeId": "mindmap.setNodeSupplement", "payload": {
                "type": "setNodeSupplement", "nodeId": "root", "supplement": {
                    "note": null, "hyperlink": null, "markers": [],
                    "image": {"assetId": asset_id, "alt": "topic", "width": null, "height": null}
                }
            }},
            {"commandId": "add-boundary", "typeId": "mindmap.addBoundary", "payload": {
                "type": "addBoundary", "boundary": {
                    "id": "boundary", "rootNodeId": "root", "label": {"text": "Scope", "runs": []}
                }
            }},
            {"commandId": "add-formula", "typeId": "mindmap.addFormula", "payload": {
                "type": "addFormula", "formula": {
                    "id": "formula", "nodeId": "root", "source": "x^2", "display": "block"
                }
            }}
        ]
    });
    let (status, committed) = app
        .json(
            Request::post(format!("/api/artifacts/{image_map_id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", transaction_id)
                .body(Body::from(transaction.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "response: {committed}");
    assert_eq!(
        db::get_artifact_asset(&app.pool, image_map_id, asset_id)
            .await
            .unwrap()
            .unwrap()
            .ref_count,
        1
    );

    let (status, markdown_headers, _) = app
        .send_with_headers(
            Request::get(format!("/api/artifacts/{image_map_id}/export/md"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        markdown_headers["x-mindmap-losses"],
        "boundary:1,formula:1,nodeImage:1"
    );

    let (status, svg_headers, exported_svg) = app
        .send_with_headers(
            Request::get(format!("/api/artifacts/{image_map_id}/export/svg"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(svg_headers["content-type"], "image/svg+xml; charset=utf-8");
    assert!(svg_headers["content-disposition"]
        .to_str()
        .unwrap()
        .contains(".svg"));
    let exported_svg = String::from_utf8(exported_svg).unwrap();
    assert!(exported_svg.starts_with("<svg xmlns="));
    assert!(exported_svg.contains("data-kind=\"node-image\""));
    assert!(exported_svg.contains("data-kind=\"boundary\""));
    assert!(exported_svg.contains("data-kind=\"formula\""));
    assert!(exported_svg.contains("data:image/png;base64,"));
    assert!(!exported_svg.contains("selection"));
    assert!(!exported_svg.contains("presence"));

    let (status, pdf_headers, exported_pdf) = app
        .send_with_headers(
            Request::get(format!(
                "/api/artifacts/{id}/export/pdf?paper=a4&orientation=landscape&mode=fit"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pdf_headers["content-type"], "application/pdf");
    assert_eq!(pdf_headers["x-mindmap-losses"], "fontSubstitution:1");
    assert!(pdf_headers["content-disposition"]
        .to_str()
        .unwrap()
        .contains(".pdf"));
    assert!(exported_pdf.starts_with(b"%PDF-"));

    let (status, exported_with_asset) = app
        .send(
            Request::get(format!("/api/artifacts/{image_map_id}/export/json"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let package = parse_mindmap_exchange(&exported_with_asset).unwrap();
    assert_eq!(package.assets.len(), 1);
    assert_eq!(package.assets[0].asset_id, asset_id);
    let (status, imported_copy) = app
        .upload_bytes("portable-asset.mindmap.json", &exported_with_asset)
        .await;
    assert_eq!(status, StatusCode::CREATED, "response: {imported_copy}");
    let copy_id = imported_copy["id"].as_str().unwrap();
    let (_, copy_snapshot) = app
        .json(
            Request::get(format!("/api/artifacts/{copy_id}/snapshot"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    let remapped_asset_id = copy_snapshot["artifact"]["payload"]["data"]["nodes"][0]["supplement"]
        ["image"]["assetId"]
        .as_str()
        .unwrap();
    assert_ne!(remapped_asset_id, asset_id);
    assert_eq!(
        copy_snapshot["artifact"]["payload"]["data"]["boundaries"][0]["id"],
        "boundary"
    );
    assert_eq!(
        copy_snapshot["artifact"]["payload"]["data"]["formulas"][0]["source"],
        "x^2"
    );
    let copy_artifact: ArtifactEnvelope =
        serde_json::from_value(copy_snapshot["artifact"].clone()).unwrap();
    let ArtifactPayload::Mindmap(mut copied_model) = copy_artifact.payload else {
        panic!("copy must remain a Mindmap");
    };
    let mut exported_model = package.model.clone();
    exported_model.nodes[0]
        .supplement
        .image
        .as_mut()
        .unwrap()
        .asset_id = "normalized-asset".into();
    copied_model.nodes[0]
        .supplement
        .image
        .as_mut()
        .unwrap()
        .asset_id = "normalized-asset".into();
    assert_eq!(copied_model, exported_model);
    let (status, copied_asset) = app
        .send(
            Request::get(format!(
                "/api/artifacts/{copy_id}/assets/{remapped_asset_id}"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(copied_asset, b"verified-image");
    assert_eq!(
        db::get_artifact_asset(&app.pool, copy_id, remapped_asset_id)
            .await
            .unwrap()
            .unwrap()
            .ref_count,
        1
    );

    let mut tampered: Value = serde_json::from_slice(&exported_with_asset).unwrap();
    tampered["assets"][0]["checksum"] = Value::String("0".repeat(64));
    let (status, rejected) = app
        .upload_bytes(
            "tampered.mindmap.json",
            &serde_json::to_vec(&tampered).unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "response: {rejected}");
    assert!(rejected["error"]
        .as_str()
        .unwrap()
        .contains("checksum 不匹配"));
}

#[tokio::test]
async fn freemind_import_is_streamed_validated_and_preserves_original_source_type() {
    let app = TestApp::new().await;
    let fixture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/mindmap/freemind/basic.mm"
    ))
    .unwrap();
    let (status, imported) = app.upload_bytes("basic.mm", &fixture).await;
    assert_eq!(status, StatusCode::CREATED, "response: {imported}");
    assert_eq!(imported["kind"], "mindmap");
    assert!(imported["warnings"][0]
        .as_str()
        .unwrap()
        .contains("unsupportedFeature:1"));
    let id = imported["id"].as_str().unwrap();
    let (_, snapshot) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/snapshot"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    let data = &snapshot["artifact"]["payload"]["data"];
    assert_eq!(data["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(data["nodes"][0]["content"]["text"], "产品路线图");
    assert_eq!(
        data["nodes"][0]["supplement"]["note"]["text"],
        "共享说明\n第二行"
    );
    assert_eq!(data["edges"][0]["sourceId"], "node-2");
    assert_eq!(data["edges"][0]["targetId"], "node-3");

    let (status, headers, source) = app
        .send_with_headers(
            Request::get(format!("/api/artifacts/{id}/source"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers["content-type"],
        "application/x-freemind; charset=utf-8"
    );
    assert!(headers["content-disposition"]
        .to_str()
        .unwrap()
        .contains(".mm"));
    assert!(String::from_utf8(source).unwrap().contains("<map version="));

    let (status, rejected) = app
        .upload_bytes(
            "malicious.mm",
            br#"<!DOCTYPE map [<!ENTITY x "boom">]><map><node TEXT="&x;"/></map>"#,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "response: {rejected}");
    assert!(rejected["error"].as_str().unwrap().contains("禁止 DTD"));

    let (status, strict_rejected) = app
        .upload_bytes_with_mode("basic.mm", &fixture, "strict")
        .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert!(strict_rejected["error"]
        .as_str()
        .unwrap()
        .contains("strict 导入拒绝有损内容"));
}

#[tokio::test]
async fn xmind_import_validates_package_remaps_assets_and_preserves_source_type() {
    let app = TestApp::new().await;
    let fixture = xmind_with_image();
    let (status, imported) = app.upload_bytes("roadmap.xmind", &fixture).await;
    assert_eq!(status, StatusCode::CREATED, "response: {imported}");
    assert_eq!(imported["kind"], "mindmap");
    assert_eq!(imported["warnings"], json!([]));
    let id = imported["id"].as_str().unwrap();

    let (_, snapshot) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/snapshot"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    let data = &snapshot["artifact"]["payload"]["data"];
    assert_eq!(data["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(data["nodes"][0]["content"]["text"], "产品路线图");
    assert_eq!(data["nodes"][0]["supplement"]["note"]["text"], "共享说明");
    assert_eq!(data["nodes"][0]["supplement"]["markers"][0], "priority-1");
    assert_eq!(data["edges"][0]["sourceId"], "node-2");
    assert_eq!(data["edges"][0]["targetId"], "node-3");
    assert_eq!(data["edges"][0]["label"]["text"], "依赖");
    let asset_id = data["nodes"][0]["supplement"]["image"]["assetId"]
        .as_str()
        .unwrap();
    assert_ne!(asset_id, "asset-1");

    let (status, listed) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/assets"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "response: {listed}");
    assert_eq!(listed["assets"][0]["assetId"], asset_id);
    assert_eq!(listed["assets"][0]["fileName"], "topic.png");
    assert_eq!(listed["assets"][0]["contentType"], "image/png");
    assert_eq!(listed["assets"][0]["refCount"], 1);
    let (status, bytes) = app
        .send(
            Request::get(format!("/api/artifacts/{id}/assets/{asset_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"xmind-image-bytes");

    let (status, headers, source) = app
        .send_with_headers(
            Request::get(format!("/api/artifacts/{id}/source"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/vnd.xmind.workbook");
    assert!(headers["content-disposition"]
        .to_str()
        .unwrap()
        .contains(".xmind"));
    assert_eq!(source, fixture);

    let empty_package = {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"{}").unwrap();
        archive.finish().unwrap().into_inner()
    };
    let (status, rejected) = app
        .upload_bytes("missing-content.xmind", &empty_package)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "response: {rejected}");
    assert!(rejected["error"]
        .as_str()
        .unwrap()
        .contains("缺少 content.json"));
}

#[tokio::test]
async fn mindmap_presence_is_ephemeral_graph_state() {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "mindmap", "title": "Presence map"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();
    let (status, _) = app
        .send(
            Request::put(format!("/api/artifacts/{id}/presence/map-browser"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"selectedNodeIds": ["root"], "cursor": {"x": 10, "y": 20}}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, page) = app
        .json(
            Request::get(format!("/api/artifacts/{id}/presence"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["participants"][0]["selectedNodeIds"][0], "root");
    assert_eq!(page["participants"][0]["cursor"]["y"], 20.0);
    assert!(db::get_artifact_history(&app.pool, id, false)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn mindmap_event_stream_pushes_durable_revision_and_ephemeral_presence() {
    let app = TestApp::new().await;
    let (status, created) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "mindmap", "title": "Stream map"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();
    let response = app
        .router
        .clone()
        .oneshot(
            Request::get(format!("/api/artifacts/{id}/event-stream?sinceRevision=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");

    let transaction = json!({
        "protocolVersion": 1,
        "transactionId": "stream-add-root",
        "intentId": "stream-add-root-intent",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "add-root",
            "typeId": "mindmap.addNode",
            "payload": {"type": "addNode", "nodeId": "root", "parentId": null, "content": {"text": "Root", "runs": []}, "attrs": {}, "index": 0}
        }]
    });
    let (status, _) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "stream-add-root")
                .body(Body::from(transaction.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let mut body = response.into_body();
    let streamed = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut output = String::new();
        while let Some(frame) = body.frame().await {
            let frame = frame.unwrap();
            if let Some(data) = frame.data_ref() {
                output.push_str(&String::from_utf8_lossy(data));
            }
            if output.contains("event: revision") && output.contains("event: presence") {
                return output;
            }
        }
        output
    })
    .await
    .expect("event stream did not deliver within budget");
    assert!(streamed.contains("\"revision\":2"), "stream: {streamed}");
    assert!(streamed.contains("mindmap.node"), "stream: {streamed}");
    assert!(
        streamed.contains("\"structureChanged\":true"),
        "stream: {streamed}"
    );
}
