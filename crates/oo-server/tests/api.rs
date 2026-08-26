//! canonical Artifact/Transaction API 集成测试。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oo_server::store::LocalFsStore;
use oo_server::{build_router, db, AppState};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tower::ServiceExt;

struct TestApp {
    router: axum::Router,
    dir: std::path::PathBuf,
    pool: SqlitePool,
}

impl TestApp {
    async fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("oo-api-{}", uuid::Uuid::new_v4()));
        let pool = db::connect("sqlite::memory:").await.unwrap();
        let store = Arc::new(LocalFsStore::new(&dir).await.unwrap());
        Self {
            router: build_router(AppState {
                pool: pool.clone(),
                store,
                write_lock: Arc::new(tokio::sync::Mutex::new(())),
                presence: Arc::new(tokio::sync::Mutex::new(
                    oo_server::presence::PresenceStore::default(),
                )),
            }),
            dir,
            pool,
        }
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, Vec<u8>) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, bytes.to_vec())
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

    async fn upload_fixture(&self, name: &str) -> (StatusCode, Value) {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
        let bytes = std::fs::read(format!("{path}{name}")).unwrap();
        self.upload_bytes(name, &bytes).await
    }

    async fn upload_bytes(&self, file_name: &str, bytes: &[u8]) -> (StatusCode, Value) {
        const BOUNDARY: &str = "----ootest";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
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
}

impl Drop for TestApp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

fn transaction_request(id: &str, body: Value) -> Request<Body> {
    let revision = body["baseRevision"].as_u64().expect("baseRevision");
    let transaction_id = body["transactionId"].as_str().expect("transactionId");
    Request::post(format!("/api/artifacts/{id}/transactions"))
        .header("content-type", "application/json")
        .header("if-match", format!("\"{revision}\""))
        .header("x-transaction-id", transaction_id)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn health_and_empty_list_work() {
    let app = TestApp::new().await;
    let (status, body) = app.json(get("/api/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    let (_, list) = app.json(get("/api/artifacts")).await;
    assert!(list["artifacts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn capabilities_expose_only_implemented_document_commands() {
    let app = TestApp::new().await;
    let (status, body) = app.json(get("/api/capabilities")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["protocolVersion"], 1);
    assert_eq!(body["contractVersion"], 1);
    assert_eq!(body["transport"]["revisionHeader"], "If-Match");
    assert_eq!(body["transport"]["idempotencyHeader"], "x-transaction-id");

    let artifacts = body["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 5);
    let document = artifacts
        .iter()
        .find(|artifact| artifact["namespace"] == "document")
        .unwrap();
    assert_eq!(document["status"], "stable");
    let commands = document["commands"].as_array().unwrap();
    assert!(commands.iter().any(|command| {
        command["typeId"] == "document.insertTableRow"
            && command["scope"] == "document.table"
            && command["requiresRevision"] == true
    }));
    assert!(commands.iter().any(|command| {
        command["typeId"] == "document.setTableRowHeight" && command["scope"] == "document.table"
    }));
    assert!(commands.iter().any(|command| {
        command["typeId"] == "document.patchInlineRange"
            && command["scope"] == "document"
            && command["requiresRevision"] == true
            && command["supportsIdempotency"] == true
    }));
    assert!(commands.iter().any(|command| {
        command["typeId"] == "document.patchTableCellInlineRange"
            && command["scope"] == "document.table"
            && command["requiresRevision"] == true
            && command["supportsIdempotency"] == true
    }));
    assert!(commands
        .iter()
        .any(|command| command["typeId"] == "document.resetBlock"));
    for type_id in [
        "document.insertQuote",
        "document.insertTodo",
        "document.insertLink",
        "document.insertDivider",
    ] {
        assert!(
            commands.iter().any(|command| command["typeId"] == type_id),
            "Word command missing from capabilities: {type_id}"
        );
    }
    assert!(!commands
        .iter()
        .any(|command| command["typeId"] == "document.updateBlock"));

    for (namespace, expected_status) in [
        ("spreadsheet", "planned"),
        ("presentation", "stable"),
        ("mindmap", "stable"),
        ("whiteboard", "planned"),
    ] {
        let planned = artifacts
            .iter()
            .find(|artifact| artifact["namespace"] == namespace)
            .unwrap();
        assert_eq!(planned["status"], expected_status);
        if expected_status == "planned" {
            assert!(planned["commands"].as_array().unwrap().is_empty());
        } else {
            assert!(!planned["commands"].as_array().unwrap().is_empty());
        }
    }
}

#[tokio::test]
async fn legacy_document_endpoints_are_not_registered() {
    let app = TestApp::new().await;
    for uri in [
        "/api/docs",
        "/api/docs/example/content",
        "/api/docs/example/operations",
    ] {
        let (status, _) = app.send(get(uri)).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "旧端点不应重新进入公共 API：{uri}"
        );
    }
}

#[tokio::test]
async fn upload_stores_and_reads_a_canonical_snapshot() {
    let app = TestApp::new().await;
    let (status, meta) = app.upload_fixture("sample.docx").await;
    assert_eq!(status, StatusCode::CREATED, "响应：{meta}");
    assert_eq!(meta["title"], "sample");
    let id = meta["id"].as_str().unwrap();
    let (status, initial_history) = app.json(get(&format!("/api/artifacts/{id}/history"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initial_history, json!({"canUndo": false, "canRedo": false}));

    let (status, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot["artifact"]["artifactId"], id);
    assert_eq!(snapshot["artifact"]["revision"], 1);
    assert_eq!(snapshot["artifact"]["payload"]["kind"], "document");
    assert_eq!(
        snapshot["artifact"]["payload"]["data"]["root"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    let created_events = db::list_pending_domain_events(&app.pool, 10).await.unwrap();
    assert_eq!(created_events.len(), 1);
    assert_eq!(created_events[0].event.type_id, "artifact.created");
    assert_eq!(created_events[0].event.payload["artifactId"], id);
}

#[tokio::test]
async fn artifact_put_advances_revision_and_preserves_schema() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let (_, mut snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let block = &snapshot["artifact"]["payload"]["data"]["blocks"][0];
    let block_id = block["id"].as_str().unwrap().to_string();
    snapshot["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"] =
        Value::String("Artifact write!".into());
    snapshot["artifact"]["payload"]["data"]["blocks"][0]["content"]["runs"] =
        Value::Array(Vec::new());
    let (status, saved) = app
        .json(
            Request::put(format!("/api/artifacts/{id}/snapshot"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "artifact-put-1")
                .body(Body::from(snapshot.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{saved}");
    assert_eq!(saved["document"]["version"], 2);
    assert_eq!(saved["events"][0]["typeId"], "document.artifactImported");

    let (_, transaction) = app
        .json(transaction_request(
            id,
            json!({
                "protocolVersion": 1,
                "transactionId": "tx-1",
                "intentId": "intent-1",
                "artifactId": id,
                "actorId": "dev-user",
                "baseRevision": 2,
                "origin": "local",
                "commands": [{
                    "commandId": "op-1",
                    "typeId": "document.replaceBlockText",
                    "payload": {
                        "type": "replaceBlockText",
                        "blockId": block_id,
                        "content": {"text": "Transaction write!", "runs": []}
                    }
                }]
            }),
        ))
        .await;
    assert_eq!(transaction["revision"], 3);
    let (_, reloaded) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(reloaded["artifact"]["revision"], 3);
    assert_eq!(
        reloaded["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"],
        "Transaction write!"
    );
}

#[tokio::test]
async fn snapshot_history_can_be_read_and_restored_as_a_new_revision() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let (_, mut first) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let original_text = first["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    first["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"] =
        Value::String("第二版".into());
    first["artifact"]["payload"]["data"]["blocks"][0]["content"]["runs"] = Value::Array(Vec::new());
    let (status, saved) = app
        .json(
            Request::put(format!("/api/artifacts/{id}/snapshot"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "artifact-put-history-1")
                .body(Body::from(first.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{saved}");
    assert_eq!(saved["document"]["version"], 2);
    assert_eq!(saved["events"][0]["typeId"], "document.artifactImported");

    let (status, history) = app
        .json(get(&format!("/api/artifacts/{id}/revisions")))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_eq!(history[0]["version"], 2);
    assert_eq!(history[0]["current"], true);
    assert_eq!(history[1]["version"], 1);
    assert_eq!(history[1]["current"], false);

    let (status, old) = app
        .json(get(&format!("/api/artifacts/{id}/revisions/1")))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(old["artifact"]["revision"], 1);
    assert_eq!(
        old["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"],
        original_text
    );

    let (status, restored) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/revisions/1/restore"))
                .header("if-match", "\"2\"")
                .header("x-transaction-id", "restore-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{restored}");
    assert_eq!(restored["document"]["version"], 3, "恢复必须产生新版本");
    assert_eq!(restored["events"][0]["typeId"], "document.artifactRestored");
    let events = db::list_pending_domain_events(&app.pool, 10).await.unwrap();
    assert_eq!(events.len(), 3, "创建、导入与恢复均必须写入 durable outbox");
    assert_eq!(events[2].event.type_id, "document.artifactRestored");

    let (status, retry) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/revisions/1/restore"))
                .header("if-match", "\"2\"")
                .header("x-transaction-id", "restore-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "重复恢复事务必须可安全重试：{retry}"
    );
    assert_eq!(retry["document"]["version"], 3);
    assert!(retry["events"].as_array().unwrap().is_empty());
    assert_eq!(db::pending_domain_event_count(&app.pool).await.unwrap(), 3);

    let (_, current) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(current["artifact"]["revision"], 3);
    assert_eq!(
        current["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"],
        original_text
    );
    let (_, history) = app
        .json(get(&format!("/api/artifacts/{id}/revisions")))
        .await;
    assert_eq!(history.as_array().unwrap().len(), 3);
    assert_eq!(history[0]["version"], 3);

    let (status, _) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/revisions/1/restore"))
                .header("if-match", "\"2\"")
                .header("x-transaction-id", "restore-stale")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "过期恢复不能覆盖新版本");
}

#[tokio::test]
async fn transactions_are_idempotent_and_reject_stale_revisions() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let block_id = snapshot["artifact"]["payload"]["data"]["blocks"][0]["id"]
        .as_str()
        .unwrap();
    let request_body = json!({
        "protocolVersion": 1,
        "transactionId": "tx-retry",
        "intentId": "intent-retry",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "op-retry",
            "typeId": "document.replaceBlockText",
            "payload": {"type": "replaceBlockText", "blockId": block_id, "content": {"text": "once", "runs": []}}
        }]
    });
    let request = || transaction_request(id, request_body.clone());
    let (first_status, first) = app.json(request()).await;
    let (retry_status, retry) = app.json(request()).await;
    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(retry_status, StatusCode::OK);
    assert_eq!(first["revision"], retry["revision"]);
    assert_eq!(first["events"].as_array().unwrap().len(), 1);
    assert_eq!(first["events"][0]["typeId"], "document.blockUpdated");
    assert!(retry["events"].as_array().unwrap().is_empty());
    let pending = db::list_pending_domain_events(&app.pool, 10).await.unwrap();
    assert_eq!(pending.len(), 2, "创建与提交事件必须进入 durable outbox");
    assert!(pending
        .iter()
        .any(|event| event.event.event_id == first["events"][0]["eventId"]));

    let (status, body) = app
        .json(
            transaction_request(id, json!({
                    "protocolVersion": 1,
                    "transactionId": "tx-stale",
                    "intentId": "intent-stale",
                    "artifactId": id,
                    "actorId": "dev-user",
                    "baseRevision": 1,
                    "origin": "local",
                    "commands": [{"commandId": "op-stale", "typeId": "document.setPageSetup", "payload": {"type": "setPageSetup", "pageSetup": null}}]
                })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "响应：{body}");
}

#[tokio::test]
async fn word_semantic_insert_commands_are_dispatched_and_persisted() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let body = json!({
        "protocolVersion": 1,
        "transactionId": "word-insert-commands",
        "intentId": "word-insert-commands-intent",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [
            {
                "commandId": "quote-1",
                "typeId": "document.insertQuote",
                "payload": {
                    "type": "insertQuote",
                    "blockId": "quote-1",
                    "content": {"text": "quoted", "runs": []},
                    "index": 0
                }
            },
            {
                "commandId": "todo-1",
                "typeId": "document.insertTodo",
                "payload": {
                    "type": "insertTodo",
                    "blockId": "todo-1",
                    "content": {"text": "todo", "runs": []},
                    "checked": true,
                    "index": 1
                }
            },
            {
                "commandId": "link-1",
                "typeId": "document.insertLink",
                "payload": {
                    "type": "insertLink",
                    "blockId": "link-1",
                    "content": {"text": "link", "runs": []},
                    "url": "https://openoffice.example",
                    "index": 2
                }
            },
            {
                "commandId": "divider-1",
                "typeId": "document.insertDivider",
                "payload": {
                    "type": "insertDivider",
                    "blockId": "divider-1",
                    "index": 3
                }
            }
        ]
    });
    let (status, committed) = app.json(transaction_request(id, body)).await;
    assert_eq!(status, StatusCode::OK, "响应：{committed}");
    assert_eq!(committed["revision"], 2);
    assert_eq!(committed["events"].as_array().unwrap().len(), 4);

    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let blocks = snapshot["artifact"]["payload"]["data"]["blocks"]
        .as_array()
        .unwrap();
    assert!(blocks.iter().any(|block| block["kind"]["type"] == "quote"));
    assert!(blocks.iter().any(|block| block["kind"]["type"] == "todo"));
    assert!(blocks.iter().any(|block| block["kind"]["type"] == "link"));
    assert!(blocks
        .iter()
        .any(|block| block["kind"]["type"] == "divider"));
}

#[tokio::test]
async fn transaction_http_headers_must_match_typed_envelope() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let block_id = snapshot["artifact"]["payload"]["data"]["blocks"][0]["id"]
        .as_str()
        .unwrap();
    let body = json!({
        "protocolVersion": 1,
        "transactionId": "header-contract",
        "intentId": "header-contract-intent",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "header-contract-command",
            "typeId": "document.replaceBlockText",
            "payload": {"type": "replaceBlockText", "blockId": block_id, "content": {"text": "headers", "runs": []}}
        }]
    });
    let (status, missing) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/transactions"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(missing["code"], "bad_request");
    assert!(missing["requestId"].as_str().unwrap().starts_with("err-"));

    let mut mismatched = transaction_request(id, body);
    mismatched
        .headers_mut()
        .insert("if-match", axum::http::HeaderValue::from_static("\"0\""));
    let (status, mismatch) = app.json(mismatched).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(mismatch["code"], "bad_request");
}

#[tokio::test]
async fn history_transactions_are_server_authoritative_and_linear() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let block_id = snapshot["artifact"]["payload"]["data"]["blocks"][0]["id"]
        .as_str()
        .unwrap();
    let original = snapshot["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"]
        .as_str()
        .unwrap();

    let update = json!({
        "protocolVersion": 1,
        "transactionId": "history-local",
        "intentId": "intent-history-local",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "history-op",
            "typeId": "document.replaceBlockText",
            "payload": {"type": "replaceBlockText", "blockId": block_id, "content": {"text": "edited", "runs": []}}
        }]
    });
    let (status, committed) = app.json(transaction_request(id, update.clone())).await;
    assert_eq!(status, StatusCode::OK, "响应：{committed}");
    assert_eq!(committed["revision"], 2);
    assert_eq!(committed["canUndo"], true);
    assert_eq!(committed["canRedo"], false);

    let undo = json!({
        "protocolVersion": 1,
        "transactionId": "history-undo",
        "intentId": "intent-history-undo",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 2,
        "origin": "undo",
        "commands": [{"commandId": "history-undo-op", "typeId": "document.history", "payload": {"action": "undo"}}]
    });
    let (status, undone) = app.json(transaction_request(id, undo.clone())).await;
    assert_eq!(status, StatusCode::OK, "响应：{undone}");
    assert_eq!(undone["revision"], 3);
    assert_eq!(undone["canUndo"], false);
    assert_eq!(undone["canRedo"], true);
    assert_eq!(undone["events"][0]["typeId"], "document.historyApplied");
    let (_, history_after_undo) = app.json(get(&format!("/api/artifacts/{id}/history"))).await;
    assert_eq!(
        history_after_undo,
        json!({"canUndo": false, "canRedo": true})
    );
    let (_, current) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(
        current["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"],
        original
    );

    // Retrying the same history transaction is idempotent and does not toggle it twice.
    let (status, retry) = app.json(transaction_request(id, undo.clone())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retry["revision"], 3);
    assert!(retry["events"].as_array().unwrap().is_empty());

    let redo = json!({
        "protocolVersion": 1,
        "transactionId": "history-redo",
        "intentId": "intent-history-redo",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 3,
        "origin": "redo",
        "commands": [{"commandId": "history-redo-op", "typeId": "document.history", "payload": {"action": "redo"}}]
    });
    let (status, redone) = app.json(transaction_request(id, redo.clone())).await;
    assert_eq!(status, StatusCode::OK, "响应：{redone}");
    assert_eq!(redone["revision"], 4);
    assert_eq!(redone["canUndo"], true);
    assert_eq!(redone["canRedo"], false);
    let (_, current) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(
        current["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"],
        "edited"
    );

    // Undo followed by a new local transaction starts a new branch, so redo is rejected.
    let undo_again = json!({
        "protocolVersion": 1,
        "transactionId": "history-undo-again",
        "intentId": "intent-history-undo-again",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 4,
        "origin": "undo",
        "commands": [{"commandId": "history-undo-again-op", "typeId": "document.history", "payload": {"action": "undo"}}]
    });
    let (status, _) = app.json(transaction_request(id, undo_again.clone())).await;
    assert_eq!(status, StatusCode::OK);
    let branch = json!({
        "protocolVersion": 1,
        "transactionId": "history-branch",
        "intentId": "intent-history-branch",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 5,
        "origin": "local",
        "commands": [{
            "commandId": "history-branch-op",
            "typeId": "document.replaceBlockText",
            "payload": {"type": "replaceBlockText", "blockId": block_id, "content": {"text": "branch", "runs": []}}
        }]
    });
    let (status, _) = app.json(transaction_request(id, branch.clone())).await;
    assert_eq!(status, StatusCode::OK);
    let mut redo_after_branch = redo.as_object().unwrap().clone();
    redo_after_branch.insert(
        "transactionId".into(),
        Value::String("history-redo-after-branch".into()),
    );
    redo_after_branch.insert("baseRevision".into(), Value::from(6));
    let (status, body) = app
        .json(transaction_request(id, Value::Object(redo_after_branch)))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "响应：{body}");
    let pending = db::list_pending_domain_events(&app.pool, 20).await.unwrap();
    assert_eq!(
        pending.len(),
        6,
        "创建、普通提交与每次历史提交各自产生一个事件"
    );
    let event_ids = pending
        .iter()
        .map(|event| event.event.event_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(event_ids.len(), pending.len(), "事件 id 必须稳定且不重复");
}

#[tokio::test]
async fn create_patch_delete_and_original_download_work() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "document", "title": "空白"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap();
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(
        snapshot["artifact"]["payload"]["data"]["root"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let created_events = db::list_pending_domain_events(&app.pool, 10).await.unwrap();
    assert_eq!(created_events.len(), 1);
    assert_eq!(created_events[0].event.type_id, "artifact.created");
    assert_eq!(created_events[0].event.payload["artifactId"], id);
    let (status, patched) = app
        .json(
            Request::patch(format!("/api/artifacts/{id}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"title": "新标题", "starred": true}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["title"], "新标题");
    assert_eq!(patched["starred"], true);
    let (status, _) = app.send(get(&format!("/api/artifacts/{id}/source"))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, exported) = app
        .send(get(&format!("/api/artifacts/{id}/export/docx")))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(exported.starts_with(b"PK"), "导出内容应是 zip/docx 包");
    let (status, _) = app
        .send(
            Request::delete(format!("/api/artifacts/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn broken_upload_is_rejected_without_creating_metadata() {
    let app = TestApp::new().await;
    let (status, body) = app.upload_bytes("bad.docx", b"not a zip").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "响应：{body}");
    let (_, list) = app.json(get("/api/artifacts")).await;
    assert!(list["artifacts"].as_array().unwrap().is_empty());
}

/// C1 drift lock: the served capability catalog must stay in lockstep with the
/// engine command surface. Any addition, removal or rename has to update this
/// test consciously instead of silently changing the public contract.
#[tokio::test]
async fn capability_catalog_locks_the_engine_command_surface() {
    const DOCUMENT_COMMANDS: &[&str] = &[
        "document.insertBlock",
        "document.insertQuote",
        "document.insertTodo",
        "document.insertLink",
        "document.insertDivider",
        "document.setBlockPresentation",
        "document.patchInlineRange",
        "document.deleteBlock",
        "document.resetBlock",
        "document.moveBlock",
        "document.setPageSetup",
        "document.formatTableCells",
        "document.setTableBorders",
        "document.applyTableBorderPreset",
        "document.setTodoChecked",
        "document.convertToLink",
        "document.setLinkTarget",
        "document.setCodeConfig",
        "document.setImageConfig",
        "document.replaceBlockText",
        "document.convertBlock",
        "document.replaceTableCellText",
        "document.patchTableCellInlineRange",
        "document.insertTableRow",
        "document.insertTableColumn",
        "document.deleteTableRow",
        "document.deleteTableColumn",
        "document.setTableColumnWidth",
        "document.setTableRowHeight",
        "document.mergeTableCells",
        "document.splitTableCells",
        "document.history",
    ];
    const MINDMAP_COMMANDS: &[&str] = &[
        "mindmap.addNode",
        "mindmap.updateNode",
        "mindmap.setNodeCollapsed",
        "mindmap.moveNode",
        "mindmap.deleteNode",
        "mindmap.addEdge",
        "mindmap.updateEdge",
        "mindmap.deleteEdge",
    ];
    const PRESENTATION_COMMANDS: &[&str] = &[
        "presentation.registerAsset",
        "presentation.setPageSpec",
        "presentation.createMaster",
        "presentation.updateMaster",
        "presentation.deleteMaster",
        "presentation.createLayout",
        "presentation.updateLayout",
        "presentation.deleteLayout",
        "presentation.createSlide",
        "presentation.deleteSlide",
        "presentation.moveSlide",
        "presentation.duplicateSlide",
        "presentation.insertNode",
        "presentation.deleteNode",
        "presentation.moveNode",
        "presentation.reorderNode",
        "presentation.groupNodes",
        "presentation.ungroupNodes",
        "presentation.setNodeTransform",
        "presentation.setNodeLocked",
        "presentation.alignNodes",
        "presentation.distributeNodes",
        "presentation.setShapeStyle",
        "presentation.setShapeGeometry",
        "presentation.setChartSpec",
        "presentation.setConnectorEndpoints",
        "presentation.setTableCellContent",
        "presentation.setTableCellStyle",
        "presentation.insertTableRows",
        "presentation.insertTableColumns",
        "presentation.deleteTableRow",
        "presentation.deleteTableColumn",
        "presentation.mergeTableCells",
        "presentation.splitTableCell",
        "presentation.setTextContent",
        "presentation.setTextFrame",
        "presentation.setImageConfig",
        "presentation.setMediaConfig",
        "presentation.setSlideNotes",
        "presentation.setSlideBackground",
        "presentation.setSlideLayout",
        "presentation.setTheme",
        "presentation.setSlideTransition",
        "presentation.upsertAnimation",
        "presentation.deleteAnimation",
        "presentation.moveAnimation",
        "presentation.history",
    ];

    let app = TestApp::new().await;
    let (status, catalog) = app.json(get("/api/capabilities")).await;
    assert_eq!(status, StatusCode::OK);
    let commands_of = |kind: &str| -> Vec<String> {
        catalog["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|artifact| artifact["kind"] == kind)
            .map(|artifact| {
                artifact["commands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|command| command["typeId"].as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_else(|| panic!("catalog 缺少 {kind} capability"))
    };

    let mut document = commands_of("document");
    document.sort();
    let mut expected_document: Vec<String> =
        DOCUMENT_COMMANDS.iter().map(|s| s.to_string()).collect();
    expected_document.sort();
    assert_eq!(
        document, expected_document,
        "document catalog 与引擎命令面漂移"
    );

    let mut presentation = commands_of("presentation");
    presentation.sort();
    let mut expected_presentation: Vec<String> = PRESENTATION_COMMANDS
        .iter()
        .map(|s| s.to_string())
        .collect();
    expected_presentation.sort();
    assert_eq!(
        presentation, expected_presentation,
        "presentation catalog 与引擎命令面漂移"
    );

    let mut mindmap = commands_of("mindmap");
    mindmap.sort();
    let mut expected_mindmap: Vec<String> =
        MINDMAP_COMMANDS.iter().map(|s| s.to_string()).collect();
    expected_mindmap.sort();
    assert_eq!(
        mindmap, expected_mindmap,
        "mindmap catalog 与引擎命令面漂移"
    );

    // The remaining kinds are honest about being planned: no invented commands.
    for kind in ["spreadsheet", "whiteboard"] {
        let commands = commands_of(kind);
        assert!(commands.is_empty(), "{kind} 不应虚报命令：{commands:?}");
    }
}

/// C3 minimal complete path for Mindmap: create -> typed semantic edit ->
/// revision advance -> idempotent replay -> stale revision rejection.
#[tokio::test]
async fn mindmap_transactions_follow_the_canonical_contract() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "mindmap", "title": "导图"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();
    assert_eq!(meta["kind"], "mindmap");

    let envelope = |tx: &str, base: u64| {
        transaction_request(
            &id,
            json!({
                "protocolVersion": 1,
                "transactionId": tx,
                "intentId": format!("intent-{tx}"),
                "artifactId": id,
                "actorId": "dev-user",
                "baseRevision": base,
                "origin": "local",
                "commands": [{
                    "commandId": format!("op-{tx}"),
                    "typeId": "mindmap.addNode",
                    "payload": {"type": "addNode", "nodeId": "root-child", "index": 0}
                }]
            }),
        )
    };

    let (status, commit) = app.json(envelope("mm-1", 1)).await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");
    assert_eq!(commit["revision"], 2);
    assert_eq!(
        commit["invalidation"]["changedEntities"][0]["entityType"],
        "mindmap.node"
    );
    let events = commit["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["typeId"], "mindmap.nodeInserted");
    assert_eq!(events[0]["payload"]["actorId"], "dev-user");

    // Idempotent replay returns the committed revision without new events.
    let (retry_status, retry) = app.json(envelope("mm-1", 1)).await;
    assert_eq!(retry_status, StatusCode::OK);
    assert_eq!(retry["revision"], commit["revision"]);
    assert!(retry["events"].as_array().unwrap().is_empty());

    // A stale base revision is rejected with a machine-readable conflict.
    let (stale_status, stale) = app.json(envelope("mm-2", 1)).await;
    assert_eq!(stale_status, StatusCode::CONFLICT);
    assert_eq!(stale["code"], "version_conflict");

    // The persisted snapshot really contains the graph node.
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(
        snapshot["artifact"]["payload"]["data"]["nodes"][0]["id"],
        "root-child"
    );
}

/// C1 Principal seam: every delivered domain event must attribute the
/// authenticated actor without requiring a transaction-table join.
#[tokio::test]
async fn domain_events_carry_the_authenticated_actor() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let block_id = snapshot["artifact"]["payload"]["data"]["blocks"][0]["id"]
        .as_str()
        .unwrap();
    let (status, commit) = app
        .json(transaction_request(
            id,
            json!({
                "protocolVersion": 1,
                "transactionId": "tx-actor",
                "intentId": "intent-actor",
                "artifactId": id,
                "actorId": "dev-user",
                "baseRevision": 1,
                "origin": "local",
                "commands": [{
                    "commandId": "op-actor",
                    "typeId": "document.replaceBlockText",
                    "payload": {"type": "replaceBlockText", "blockId": block_id, "content": {"text": "attributed", "runs": []}}
                }]
            }),
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    for event in commit["events"].as_array().unwrap() {
        assert_eq!(
            event["payload"]["actorId"], "dev-user",
            "提交响应事件缺少 actor 归属：{event}"
        );
    }

    // The durable feed exposes the same attribution to external clients.
    let (status, page) = app
        .json(get(&format!("/api/artifacts/{id}/events?sinceRevision=1")))
        .await;
    assert_eq!(status, StatusCode::OK);
    let events = page["events"].as_array().unwrap();
    assert!(!events.is_empty());
    for event in events {
        assert_eq!(event["payload"]["actorId"], "dev-user", "事件 {event}");
    }
}
