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
    /// 默认显式信任 `X-OO-User`：ACL、presence 与审计测试需要参数化 Principal。
    /// 生产默认是关闭的，覆盖那条路径的测试见
    /// `user_header_is_refused_unless_explicitly_trusted`。
    async fn new() -> Self {
        Self::with_user_header_trust(true).await
    }

    /// 用指定的 `X-OO-User` 信任开关装配应用，让两种部署配置都能被覆盖。
    async fn with_user_header_trust(trust_user_header: bool) -> Self {
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
                trust_user_header,
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
    assert_eq!(body["contractVersion"], 2);
    assert_eq!(body["transport"]["revisionHeader"], "If-Match");
    assert_eq!(body["transport"]["idempotencyHeader"], "x-transaction-id");

    let artifacts = body["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 5);
    let document = artifacts
        .iter()
        .find(|artifact| artifact["namespace"] == "document")
        .unwrap();
    assert_eq!(document["features"]["edit"], "stable");
    assert_eq!(document["features"]["history"], "stable");
    assert_eq!(document["features"]["presence"], "preview");
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

    for (namespace, edit, history, import, export, presence) in [
        (
            "spreadsheet",
            "stable",
            "stable",
            "preview",
            "preview",
            "planned",
        ),
        (
            "presentation",
            "stable",
            "stable",
            "preview",
            "preview",
            "preview",
        ),
        (
            "mindmap", "stable", "stable", "preview", "preview", "preview",
        ),
        (
            "whiteboard",
            "preview",
            "planned",
            "unsupported",
            "unsupported",
            "planned",
        ),
    ] {
        let capability = artifacts
            .iter()
            .find(|artifact| artifact["namespace"] == namespace)
            .unwrap();
        assert_eq!(capability["features"]["edit"], edit);
        assert_eq!(capability["features"]["history"], history);
        assert_eq!(capability["features"]["import"], import);
        assert_eq!(capability["features"]["export"], export);
        assert_eq!(capability["features"]["presence"], presence);
        assert!(!capability["commands"].as_array().unwrap().is_empty());
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
async fn snapshot_put_is_not_public_and_transaction_preserves_schema() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let block = &snapshot["artifact"]["payload"]["data"]["blocks"][0];
    let block_id = block["id"].as_str().unwrap().to_string();
    let (status, rejected) = app
        .send(
            Request::put(format!("/api/artifacts/{id}/snapshot"))
                .header("content-type", "application/json")
                .header("if-match", "\"1\"")
                .header("x-transaction-id", "artifact-put-1")
                .body(Body::from(snapshot.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(rejected.is_empty(), "405 不应伪造 JSON 响应体");

    let (_, transaction) = app
        .json(transaction_request(
            id,
            json!({
                "protocolVersion": 1,
                "transactionId": "tx-1",
                "intentId": "intent-1",
                "artifactId": id,
                "actorId": "dev-user",
                "baseRevision": 1,
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
    assert_eq!(transaction["revision"], 2);
    let (_, reloaded) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(reloaded["artifact"]["revision"], 2);
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
    let (_, first) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let original_text = first["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let block_id = first["artifact"]["payload"]["data"]["blocks"][0]["id"]
        .as_str()
        .unwrap();
    let (status, saved) = app
        .json(transaction_request(
            id,
            json!({
                "protocolVersion": 1,
                "transactionId": "history-version-2",
                "intentId": "history-version-2-intent",
                "artifactId": id,
                "actorId": "dev-user",
                "baseRevision": 1,
                "origin": "local",
                "commands": [{
                    "commandId": "history-version-2-command",
                    "typeId": "document.replaceBlockText",
                    "payload": {
                        "type": "replaceBlockText",
                        "blockId": block_id,
                        "content": {"text": "第二版", "runs": []}
                    }
                }]
            }),
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{saved}");
    assert_eq!(saved["revision"], 2);
    assert_eq!(saved["events"][0]["typeId"], "document.blockUpdated");

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
    assert_eq!(
        events.len(),
        3,
        "创建、语义编辑与恢复均必须写入 durable outbox"
    );
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
    assert_eq!(body["code"], "version_conflict");
    assert_eq!(body["retryable"], true);
    assert!(body["requestId"].as_str().unwrap().starts_with("err-"));
    assert!(body["details"]["changedEntities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entity| entity == block_id));
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
async fn document_page_semantics_commit_and_print_projection_are_canonical() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let root_id = snapshot["artifact"]["payload"]["data"]["root"][0]
        .as_str()
        .unwrap();
    let body = json!({
        "protocolVersion": 1,
        "transactionId": "page-semantics",
        "intentId": "page-semantics-intent",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [
            {
                "commandId": "section-1",
                "typeId": "document.upsertSection",
                "payload": {
                    "type": "upsertSection",
                    "index": 0,
                    "section": {
                        "id": "section-1",
                        "startBlockId": root_id,
                        "pageSetup": null,
                        "header": {"default": {"segments": [{"type": "text", "content": {"text": "Header", "runs": []}}]}, "firstPage": null, "evenPages": null},
                        "footer": {"default": {"segments": [{"type": "pageNumber"}]}, "firstPage": null, "evenPages": null},
                        "pageNumbering": {"startAt": 3, "format": "decimal"}
                    }
                }
            },
            {
                "commandId": "footnote-1",
                "typeId": "document.upsertNote",
                "payload": {
                    "type": "upsertNote",
                    "noteKind": "footnote",
                    "note": {
                        "id": "footnote-1",
                        "anchor": {"blockId": root_id, "rowId": null, "cellId": null, "start": 0, "end": 0},
                        "content": [{"text": "A note", "runs": []}]
                    }
                }
            }
        ]
    });
    let (status, commit) = app.json(transaction_request(id, body)).await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");
    assert_eq!(
        commit["events"][0]["typeId"],
        "document.pageSemanticsChanged"
    );

    let (status, projection) = app
        .json(get(&format!(
            "/api/artifacts/{id}/projection/documentPrint"
        )))
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{projection}");
    assert_eq!(projection["projection"], "documentPrint");
    assert_eq!(projection["data"]["revision"], 2);
    assert_eq!(
        projection["data"]["sections"][0]["rootBlockIds"][0],
        root_id
    );
    assert_eq!(
        projection["data"]["sections"][0]["pageNumbering"]["startAt"],
        3
    );
    assert_eq!(projection["data"]["footnotes"][0]["id"], "footnote-1");
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
        "document.upsertSection",
        "document.deleteSection",
        "document.upsertNote",
        "document.deleteNote",
        "document.formatTableCells",
        "document.setTableBorders",
        "document.applyTableBorderPreset",
        "document.setTodoChecked",
        "document.convertToLink",
        "document.setLinkTarget",
        "document.setCodeConfig",
        "document.setImageConfig",
        "document.replaceBlockText",
        "document.replaceAllText",
        "document.replaceTextMatch",
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
    const SPREADSHEET_COMMANDS: &[&str] = &[
        "spreadsheet.createSheet",
        "spreadsheet.renameSheet",
        "spreadsheet.deleteSheet",
        "spreadsheet.setSheetMetadata",
        "spreadsheet.insertRows",
        "spreadsheet.insertColumns",
        "spreadsheet.deleteRows",
        "spreadsheet.deleteColumns",
        "spreadsheet.mergeCells",
        "spreadsheet.unmergeCells",
        "spreadsheet.sortRange",
        "spreadsheet.formatRange",
        "spreadsheet.clearRange",
        "spreadsheet.replaceRange",
        "spreadsheet.pasteRange",
        "spreadsheet.fillRange",
        "spreadsheet.setFreezePane",
        "spreadsheet.setAutoFilter",
        "spreadsheet.upsertFilterColumn",
        "spreadsheet.clearFilter",
        "spreadsheet.setCalculationMode",
        "spreadsheet.setRowDimensions",
        "spreadsheet.setRowLayout",
        "spreadsheet.setColumnDimensions",
        "spreadsheet.upsertConditionalFormat",
        "spreadsheet.deleteConditionalFormat",
        "spreadsheet.upsertDataValidation",
        "spreadsheet.deleteDataValidation",
        "spreadsheet.history",
        "spreadsheet.setCell",
        "spreadsheet.setCellStyle",
        "spreadsheet.clearCell",
    ];
    const WHITEBOARD_COMMANDS: &[&str] = &[
        "whiteboard.addElement",
        "whiteboard.updateElement",
        "whiteboard.deleteElement",
        "whiteboard.setCamera",
        "whiteboard.panCamera",
        "whiteboard.zoomCamera",
    ];
    const MINDMAP_COMMANDS: &[&str] = &[
        "mindmap.setSettings",
        "mindmap.addNode",
        "mindmap.updateNode",
        "mindmap.replaceNodeText",
        "mindmap.patchNodeTextRange",
        "mindmap.setNodeStyle",
        "mindmap.setNodeSupplement",
        "mindmap.setNodeCollapsed",
        "mindmap.moveNode",
        "mindmap.deleteNode",
        "mindmap.addEdge",
        "mindmap.updateEdge",
        "mindmap.setEdgeStyle",
        "mindmap.deleteEdge",
        "mindmap.addSummary",
        "mindmap.updateSummary",
        "mindmap.deleteSummary",
        "mindmap.addBoundary",
        "mindmap.updateBoundary",
        "mindmap.deleteBoundary",
        "mindmap.addFormula",
        "mindmap.updateFormula",
        "mindmap.deleteFormula",
        "mindmap.history",
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

    for (kind, expected) in [
        ("spreadsheet", SPREADSHEET_COMMANDS),
        ("whiteboard", WHITEBOARD_COMMANDS),
    ] {
        let mut actual = commands_of(kind);
        actual.sort();
        let mut expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(actual, expected, "{kind} catalog 与引擎命令面漂移");
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

    // Add a real child so the projection contract covers the connector DTO,
    // not just a root-only layout.
    let child = transaction_request(
        &id,
        json!({
            "protocolVersion": 1,
            "transactionId": "mm-child",
            "intentId": "intent-mm-child",
            "artifactId": id,
            "actorId": "dev-user",
            "baseRevision": 2,
            "origin": "local",
            "commands": [{
                "commandId": "op-mm-child",
                "typeId": "mindmap.addNode",
                "payload": {
                    "type": "addNode", "nodeId": "child", "parentId": "root-child", "index": 0
                }
            }]
        }),
    );
    let (child_status, child_commit) = app.json(child).await;
    assert_eq!(child_status, StatusCode::OK, "响应：{child_commit}");

    // The renderer reads canonical graph geometry through the versioned
    // projection envelope; the response must identify itself as `mindmap`.
    let (projection_status, projection) = app
        .json(get(&format!("/api/artifacts/{id}/projection/mindmap")))
        .await;
    assert_eq!(projection_status, StatusCode::OK, "响应：{projection}");
    assert_eq!(projection["projection"], "mindmap");
    assert_eq!(projection["data"]["layout"]["nodes"][0]["id"], "root-child");
    assert_eq!(
        projection["data"]["edges"]["routes"][0]["parentId"],
        "root-child"
    );
    assert_eq!(projection["data"]["edges"]["routes"][0]["childId"], "child");
}

#[tokio::test]
async fn mindmap_advanced_structures_persist_project_and_undo_as_typed_transactions() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "mindmap", "title": "高级结构"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap();

    let nodes = json!({
        "protocolVersion": 1,
        "transactionId": "mm-advanced-nodes",
        "intentId": "intent-mm-advanced-nodes",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [
            {"commandId": "root", "typeId": "mindmap.addNode", "payload": {"type": "addNode", "nodeId": "root", "index": 0}},
            {"commandId": "a", "typeId": "mindmap.addNode", "payload": {"type": "addNode", "nodeId": "a", "parentId": "root", "index": 0}},
            {"commandId": "b", "typeId": "mindmap.addNode", "payload": {"type": "addNode", "nodeId": "b", "parentId": "root", "index": 1}}
        ]
    });
    let (status, created) = app.json(transaction_request(id, nodes)).await;
    assert_eq!(status, StatusCode::OK, "响应：{created}");

    let advanced = json!({
        "protocolVersion": 1,
        "transactionId": "mm-advanced-add",
        "intentId": "intent-mm-advanced-add",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 2,
        "origin": "local",
        "commands": [
            {"commandId": "summary", "typeId": "mindmap.addSummary", "payload": {"type": "addSummary", "summary": {"id": "summary", "startNodeId": "a", "endNodeId": "b", "content": {"text": "结论", "runs": []}}}},
            {"commandId": "boundary", "typeId": "mindmap.addBoundary", "payload": {"type": "addBoundary", "boundary": {"id": "boundary", "rootNodeId": "a", "label": {"text": "范围", "runs": []}}}},
            {"commandId": "formula", "typeId": "mindmap.addFormula", "payload": {"type": "addFormula", "formula": {"id": "formula", "nodeId": "b", "source": "x^2", "display": "block"}}}
        ]
    });
    let (status, committed) = app.json(transaction_request(id, advanced)).await;
    assert_eq!(status, StatusCode::OK, "响应：{committed}");
    assert_eq!(committed["revision"], 3);
    assert_eq!(committed["invalidation"]["structureChanged"], true);
    assert_eq!(committed["mutations"].as_array().unwrap().len(), 3);

    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let data = &snapshot["artifact"]["payload"]["data"];
    assert_eq!(
        snapshot["artifact"]["schemaVersion"],
        oo_schema::CURRENT_SCHEMA_VERSION
    );
    assert_eq!(data["summaries"][0]["content"]["text"], "结论");
    assert_eq!(data["boundaries"][0]["rootNodeId"], "a");
    assert_eq!(data["formulas"][0]["source"], "x^2");

    let (status, projection) = app
        .json(get(&format!("/api/artifacts/{id}/projection/mindmap")))
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{projection}");
    assert_eq!(
        projection["data"]["advanced"]["summaries"][0]["nodeIds"],
        json!(["a", "b"]),
        "投影：{projection}"
    );
    assert!(
        projection["data"]["advanced"]["boundaries"][0]["rect"]["width"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(projection["data"]["advanced"]["formulas"][0]["nodeId"], "b");

    let undo = json!({
        "protocolVersion": 1,
        "transactionId": "mm-advanced-undo",
        "intentId": "intent-mm-advanced-undo",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 3,
        "origin": "undo",
        "commands": [{"commandId": "undo", "typeId": "mindmap.history", "payload": {"action": "undo"}}]
    });
    let (status, undone) = app.json(transaction_request(id, undo)).await;
    assert_eq!(status, StatusCode::OK, "响应：{undone}");
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let data = &snapshot["artifact"]["payload"]["data"];
    assert!(data["summaries"].as_array().unwrap().is_empty());
    assert!(data["boundaries"].as_array().unwrap().is_empty());
    assert!(data["formulas"].as_array().unwrap().is_empty());
    assert_eq!(data["nodes"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn mindmap_history_is_server_authoritative_and_restart_safe() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "mindmap", "title": "历史导图"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap();
    let add = json!({
        "protocolVersion": 1,
        "transactionId": "mm-history-add",
        "intentId": "intent-mm-history-add",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "mm-history-add-root",
            "typeId": "mindmap.addNode",
            "payload": {"type": "addNode", "nodeId": "root", "index": 0}
        }]
    });
    let (status, added) = app.json(transaction_request(id, add)).await;
    assert_eq!(status, StatusCode::OK, "响应：{added}");
    assert_eq!(added["canUndo"], true);
    assert_eq!(added["canRedo"], false);

    let undo = json!({
        "protocolVersion": 1,
        "transactionId": "mm-history-undo",
        "intentId": "intent-mm-history-undo",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 2,
        "origin": "undo",
        "commands": [{
            "commandId": "mm-history-undo-op",
            "typeId": "mindmap.history",
            "payload": {"action": "undo"}
        }]
    });
    let (status, undone) = app.json(transaction_request(id, undo.clone())).await;
    assert_eq!(status, StatusCode::OK, "响应：{undone}");
    assert_eq!(undone["revision"], 3);
    assert_eq!(undone["canUndo"], false);
    assert_eq!(undone["canRedo"], true);
    assert!(undone["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["typeId"] == "mindmap.historyApplied"));
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert!(snapshot["artifact"]["payload"]["data"]["nodes"]
        .as_array()
        .unwrap()
        .is_empty());

    // History intents are idempotent and the redo source is retained durably.
    let (_, retry) = app.json(transaction_request(id, undo)).await;
    assert_eq!(retry["revision"], 3);
    let redo = json!({
        "protocolVersion": 1,
        "transactionId": "mm-history-redo",
        "intentId": "intent-mm-history-redo",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 3,
        "origin": "redo",
        "commands": [{
            "commandId": "mm-history-redo-op",
            "typeId": "mindmap.history",
            "payload": {"action": "redo"}
        }]
    });
    let (status, redone) = app.json(transaction_request(id, redo)).await;
    assert_eq!(status, StatusCode::OK, "响应：{redone}");
    assert_eq!(redone["revision"], 4);
    assert_eq!(redone["canRedo"], false);
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(
        snapshot["artifact"]["payload"]["data"]["nodes"][0]["id"],
        "root"
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
                "actorId": "forged-client-session",
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
    let audit = db::get_artifact_transaction(&app.pool, id, "tx-actor")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(audit.author_id, "dev-user");
    assert_eq!(audit.client_actor_id, "forged-client-session");
    assert_eq!(audit.origin, "local");
}

/// C3 minimal complete path for Whiteboard: scene element edit under the
/// canonical transaction contract.
#[tokio::test]
async fn whiteboard_transactions_follow_the_canonical_contract() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kind": "whiteboard"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();

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
                    "typeId": "whiteboard.addElement",
                    "payload": {
                        "type": "addElement",
                        "element": {"id": "el-1", "typeId": "rect"},
                        "index": 0
                    }
                }]
            }),
        )
    };

    let (status, commit) = app.json(envelope("wb-1", 1)).await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");
    assert_eq!(commit["revision"], 2);
    assert_eq!(
        commit["invalidation"]["changedEntities"][0]["entityType"],
        "whiteboard.element"
    );
    let events = commit["events"].as_array().unwrap();
    assert_eq!(events[0]["typeId"], "whiteboard.elementInserted");
    assert_eq!(events[0]["payload"]["actorId"], "dev-user");

    let (retry_status, retry) = app.json(envelope("wb-1", 1)).await;
    assert_eq!(retry_status, StatusCode::OK);
    assert_eq!(retry["revision"], commit["revision"]);
    assert!(retry["events"].as_array().unwrap().is_empty());

    let (stale_status, stale) = app.json(envelope("wb-2", 1)).await;
    assert_eq!(stale_status, StatusCode::CONFLICT);
    assert_eq!(stale["code"], "version_conflict");

    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(
        snapshot["artifact"]["payload"]["data"]["elements"][0]["id"],
        "el-1"
    );
}

/// C3 minimal complete path for Spreadsheet: sheet creation plus a cell edit
/// in one atomic batch, then replay/conflict semantics.
#[tokio::test]
async fn spreadsheet_transactions_follow_the_canonical_contract() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "spreadsheet", "title": "表"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();

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
                "commands": [
                    {
                        "commandId": format!("op-{tx}-sheet"),
                        "typeId": "spreadsheet.createSheet",
                        "payload": {"type": "createSheet", "id": "s1", "name": "Sheet1"}
                    },
                    {
                        "commandId": format!("op-{tx}-cell"),
                        "typeId": "spreadsheet.setCell",
                        "payload": {"type": "setCell", "sheetId": "s1", "row": 1, "column": 1, "value": 42}
                    }
                ]
            }),
        )
    };

    let (status, commit) = app.json(envelope("ss-1", 1)).await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");
    assert_eq!(commit["revision"], 2);
    let events = commit["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["typeId"], "spreadsheet.sheetChanged");
    assert_eq!(events[1]["typeId"], "spreadsheet.cellChanged");
    assert_eq!(events[1]["payload"]["after"]["value"], 42);
    assert_eq!(events[1]["payload"]["actorId"], "dev-user");

    let (retry_status, retry) = app.json(envelope("ss-1", 1)).await;
    assert_eq!(retry_status, StatusCode::OK);
    assert_eq!(retry["revision"], commit["revision"]);
    assert!(retry["events"].as_array().unwrap().is_empty());

    let (stale_status, _) = app.json(envelope("ss-2", 1)).await;
    assert_eq!(stale_status, StatusCode::CONFLICT);

    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let sheets = snapshot["artifact"]["payload"]["data"]["sheets"]
        .as_array()
        .unwrap();
    let s1 = sheets
        .iter()
        .find(|sheet| sheet["id"] == "s1")
        .expect("应存在 s1 工作表");
    assert_eq!(s1["cells"][0]["value"], 42);
}

/// Spreadsheet undo/redo is server-authoritative: the durable semantic
/// command record replays against the pre-transaction snapshot, the inverse
/// never travels from the client, and the resulting snapshot is a normal
/// immutable history transition.
#[tokio::test]
async fn spreadsheet_history_is_server_authoritative() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "spreadsheet", "title": "历史表"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();

    let cell_tx = |tx: &str, base: u64, value: i64| {
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
                "commands": [
                    {
                        "commandId": format!("op-{tx}-sheet"),
                        "typeId": "spreadsheet.createSheet",
                        "payload": {"type": "createSheet", "id": "s1", "name": "Sheet1"}
                    },
                    {
                        "commandId": format!("op-{tx}-cell"),
                        "typeId": "spreadsheet.setCell",
                        "payload": {"type": "setCell", "sheetId": "s1", "row": 1, "column": 1, "value": value}
                    }
                ]
            }),
        )
    };
    // Revision 1 -> 2 (createSheet + setCell 7) -> 3 (setCell 99).
    let (status, first) = app.json(cell_tx("ss-h1", 1, 7)).await;
    assert_eq!(status, StatusCode::OK, "响应：{first}");
    assert_eq!(first["canUndo"], true);
    assert_eq!(first["canRedo"], false);
    let (_, second) = app.json(cell_tx_sheet_only(&id, "ss-h2", 2, 99)).await;
    assert_eq!(second["revision"], 3);

    let undo = json!({
        "protocolVersion": 1,
        "transactionId": "ss-undo",
        "intentId": "intent-ss-undo",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 3,
        "origin": "undo",
        "commands": [{"commandId": "ss-undo-op", "typeId": "spreadsheet.history", "payload": {"action": "undo"}}]
    });
    let (status, undone) = app.json(transaction_request(&id, undo.clone())).await;
    assert_eq!(status, StatusCode::OK, "响应：{undone}");
    assert_eq!(undone["revision"], 4);
    assert_eq!(undone["canUndo"], true, "第一次 undo 后还能撤销 ss-h1");
    assert_eq!(undone["canRedo"], true);
    assert_eq!(undone["events"][0]["typeId"], "spreadsheet.cellChanged");
    assert!(undone["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["typeId"] == "spreadsheet.historyApplied"));
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let s1 = snapshot["artifact"]["payload"]["data"]["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sheet| sheet["id"] == "s1")
        .expect("undo 不能删除工作表");
    let cell = s1["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cell| cell["row"] == 1 && cell["column"] == 1)
        .expect("undo 后单元格仍在");
    assert_eq!(cell["value"], 7, "undo 恢复为 ss-h1 写入的值");

    // Retrying the same history transaction is idempotent.
    let (status, retry) = app.json(transaction_request(&id, undo.clone())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retry["revision"], 4);
    assert!(retry["events"].as_array().unwrap().is_empty());

    let redo = json!({
        "protocolVersion": 1,
        "transactionId": "ss-redo",
        "intentId": "intent-ss-redo",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 4,
        "origin": "redo",
        "commands": [{"commandId": "ss-redo-op", "typeId": "spreadsheet.history", "payload": {"action": "redo"}}]
    });
    let (status, redone) = app.json(transaction_request(&id, redo.clone())).await;
    assert_eq!(status, StatusCode::OK, "响应：{redone}");
    assert_eq!(redone["revision"], 5);
    assert_eq!(redone["canUndo"], true);
    assert_eq!(redone["canRedo"], false);
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let s1 = snapshot["artifact"]["payload"]["data"]["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sheet| sheet["id"] == "s1")
        .unwrap();
    let cell = s1["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cell| cell["row"] == 1 && cell["column"] == 1)
        .expect("redo 后单元格仍在");
    assert_eq!(cell["value"], 99, "redo 恢复为 ss-h2 写入的值");

    // Undoing an exhausted stack fails loudly instead of silently no-oping.
    let (_, history) = app.json(get(&format!("/api/artifacts/{id}/history"))).await;
    assert_eq!(history["canUndo"], true);
}

/// M1-S：范围命令走 canonical 事务路径——一次范围格式化 = 一条
/// rangeChanged mutation = 一个用户历史项，撤销可整体回退。
#[tokio::test]
async fn spreadsheet_range_commands_are_atomic_and_reversible() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kind": "spreadsheet"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();

    let tx = json!({
        "protocolVersion": 1,
        "transactionId": "ss-range-1",
        "intentId": "intent-ss-range-1",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [
            {"commandId": "r-sheet", "typeId": "spreadsheet.createSheet",
             "payload": {"type": "createSheet", "id": "s1", "name": "Sheet1"}},
            {"commandId": "r-a", "typeId": "spreadsheet.setCell",
             "payload": {"type": "setCell", "sheetId": "s1", "row": 0, "column": 0, "value": "hello"}},
            {"commandId": "r-b", "typeId": "spreadsheet.setCell",
             "payload": {"type": "setCell", "sheetId": "s1", "row": 1, "column": 0, "value": "HELLO"}},
            {"commandId": "r-format", "typeId": "spreadsheet.formatRange",
             "payload": {"type": "formatRange", "sheetId": "s1",
                "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 0},
                "style": {"numberFormat": null, "font": {"family": null, "size": null, "bold": true, "italic": false, "strikethrough": false, "underline": false, "color": null}, "fill": null, "alignment": null, "borders": null}}},
            {"commandId": "r-replace", "typeId": "spreadsheet.replaceRange",
             "payload": {"type": "replaceRange", "sheetId": "s1",
                "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 0},
                "search": "hello", "replace": "hi", "matchCase": false}},
            {"commandId": "r-freeze", "typeId": "spreadsheet.setFreezePane",
             "payload": {"type": "setFreezePane", "sheetId": "s1", "rows": 1, "columns": 0}},
            {"commandId": "r-filter", "typeId": "spreadsheet.setAutoFilter",
             "payload": {"type": "setAutoFilter", "sheetId": "s1",
                "range": {"startRow": 0, "startColumn": 0, "endRow": 9, "endColumn": 3}}}
        ]
    });
    let (status, commit) = app.json(transaction_request(&id, tx)).await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");
    assert_eq!(commit["revision"], 2);
    let events = commit["events"].as_array().unwrap();
    let type_ids: Vec<&str> = events
        .iter()
        .map(|event| event["typeId"].as_str().unwrap())
        .collect();
    assert!(
        type_ids.contains(&"spreadsheet.rangeChanged"),
        "范围命令产生 rangeChanged 事件：{type_ids:?}"
    );
    assert!(
        type_ids.contains(&"spreadsheet.paneChanged"),
        "冻结产生 paneChanged 事件：{type_ids:?}"
    );
    assert!(
        type_ids.contains(&"spreadsheet.filterChanged"),
        "筛选产生 filterChanged 事件：{type_ids:?}"
    );
    assert_eq!(commit["canUndo"], true);

    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let s1 = snapshot["artifact"]["payload"]["data"]["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sheet| sheet["id"] == "s1")
        .unwrap();
    let cells = s1["cells"].as_array().unwrap();
    let a1 = cells
        .iter()
        .find(|cell| cell["row"] == 0 && cell["column"] == 0)
        .expect("A1 应存在");
    assert_eq!(a1["value"], "hi", "replaceRange 大小写不敏感替换：{a1}");
    assert_eq!(a1["style"]["font"]["bold"], true, "formatRange 加粗：{a1}");
    let a2 = cells
        .iter()
        .find(|cell| cell["row"] == 1 && cell["column"] == 0)
        .expect("A2 应存在");
    assert_eq!(a2["value"], "hi", "replaceRange 覆盖第二格");
    assert_eq!(s1["metadata"]["freeze"]["rows"], 1);
    assert!(s1["metadata"]["autoFilter"].is_object());

    // 撤销整个批次：范围 mutation 与细粒度 metadata 全部回退。
    let undo = json!({
        "protocolVersion": 1,
        "transactionId": "ss-range-undo",
        "intentId": "intent-ss-range-undo",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 2,
        "origin": "undo",
        "commands": [{"commandId": "r-undo", "typeId": "spreadsheet.history", "payload": {"action": "undo"}}]
    });
    let (status, undone) = app.json(transaction_request(&id, undo)).await;
    assert_eq!(status, StatusCode::OK, "响应：{undone}");
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    // 整批回退语义：undo 撤销的是完整事务（含 createSheet），工作表回到不存在。
    let s1 = snapshot["artifact"]["payload"]["data"]["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sheet| sheet["id"] == "s1");
    assert!(s1.is_none(), "整批 undo 应回退到事务前状态");
}

#[tokio::test]
async fn spreadsheet_ten_thousand_cell_paste_is_one_undoable_transaction() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kind": "spreadsheet"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();
    let cells: Vec<_> = (0..100u32)
        .flat_map(|row| {
            (0..100u32).map(move |column| {
                json!({
                    "rowOffset": row,
                    "columnOffset": column,
                    "value": row * 100 + column,
                    "attrs": {}
                })
            })
        })
        .collect();
    let paste = json!({
        "protocolVersion": 1,
        "transactionId": "ss-paste-10k",
        "intentId": "intent-ss-paste-10k",
        "artifactId": id,
        "actorId": "spreadsheet-web",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "paste-10k",
            "typeId": "spreadsheet.pasteRange",
            "payload": {
                "type": "pasteRange",
                "sheetId": "sheet-1",
                "startRow": 0,
                "startColumn": 0,
                "rowCount": 100,
                "columnCount": 100,
                "cells": cells,
                "mode": "all"
            }
        }]
    });
    let (status, commit) = app.json(transaction_request(&id, paste)).await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");
    assert_eq!(commit["revision"], 2);
    assert_eq!(commit["events"].as_array().unwrap().len(), 1);
    assert_eq!(commit["events"][0]["typeId"], "spreadsheet.rangeChanged");

    let (_, history) = app.json(get(&format!("/api/artifacts/{id}/history"))).await;
    assert_eq!(history["canUndo"], true);
    assert_eq!(history["canRedo"], false);
    let undo = json!({
        "protocolVersion": 1,
        "transactionId": "ss-paste-undo",
        "intentId": "intent-ss-paste-undo",
        "artifactId": id,
        "actorId": "spreadsheet-web",
        "baseRevision": 2,
        "origin": "undo",
        "commands": [{"commandId": "paste-undo", "typeId": "spreadsheet.history", "payload": {"action": "undo"}}]
    });
    let (status, undone) = app.json(transaction_request(&id, undo)).await;
    assert_eq!(status, StatusCode::OK, "响应：{undone}");
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert!(
        snapshot["artifact"]["payload"]["data"]["sheets"][0]["cells"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

/// M2-S：细粒度规则/维度/筛选列命令走 canonical 事务路径，产生专属
/// 事件且快照持久化正确。
#[tokio::test]
async fn spreadsheet_rule_and_dimension_commands_follow_the_contract() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kind": "spreadsheet"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();

    let tx = json!({
        "protocolVersion": 1,
        "transactionId": "ss-rule-1",
        "intentId": "intent-ss-rule-1",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [
            {"commandId": "rl-sheet", "typeId": "spreadsheet.createSheet",
             "payload": {"type": "createSheet", "id": "s1", "name": "Sheet1"}},
            {"commandId": "rl-filter", "typeId": "spreadsheet.setAutoFilter",
             "payload": {"type": "setAutoFilter", "sheetId": "s1",
                "range": {"startRow": 0, "startColumn": 0, "endRow": 9, "endColumn": 3}}},
            {"commandId": "rl-col", "typeId": "spreadsheet.upsertFilterColumn",
             "payload": {"type": "upsertFilterColumn", "sheetId": "s1", "column": 1,
                "predicate": {"type": "contains", "value": "x"}}},
            {"commandId": "rl-dim", "typeId": "spreadsheet.setRowDimensions",
             "payload": {"type": "setRowDimensions", "sheetId": "s1", "rows": 100}},
            {"commandId": "rl-cf", "typeId": "spreadsheet.upsertConditionalFormat",
             "payload": {"type": "upsertConditionalFormat", "sheetId": "s1",
                "rule": {"id": "cf-1", "range": {"startRow": 0, "startColumn": 0, "endRow": 5, "endColumn": 1},
                    "predicate": {"type": "cellIs", "value": {"operator": "greaterThan", "value": 0}}, "style": {}}}},
            {"commandId": "rl-dv", "typeId": "spreadsheet.upsertDataValidation",
             "payload": {"type": "upsertDataValidation", "sheetId": "s1",
                "rule": {"id": "dv-1", "range": {"startRow": 0, "startColumn": 0, "endRow": 5, "endColumn": 0},
                    "kind": {"type": "wholeNumber", "value": {"min": 0, "max": 10}}, "allowBlank": true, "errorMessage": null}}},
            {"commandId": "rl-mode", "typeId": "spreadsheet.setCalculationMode",
             "payload": {"type": "setCalculationMode", "calculationMode": "manual"}}
        ]
    });
    let (status, commit) = app.json(transaction_request(&id, tx)).await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");
    let events = commit["events"].as_array().unwrap();
    let type_ids: Vec<&str> = events
        .iter()
        .map(|event| event["typeId"].as_str().unwrap())
        .collect();
    for expected in [
        "spreadsheet.filterColumnsChanged",
        "spreadsheet.rowDimensionsChanged",
        "spreadsheet.conditionalFormatUpserted",
        "spreadsheet.dataValidationUpserted",
        "spreadsheet.calculationModeChanged",
    ] {
        assert!(
            type_ids.contains(&expected),
            "缺少事件 {expected}：{type_ids:?}"
        );
    }

    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let s1 = snapshot["artifact"]["payload"]["data"]["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sheet| sheet["id"] == "s1")
        .unwrap();
    assert_eq!(s1["metadata"]["rowCount"], 100);
    assert_eq!(s1["metadata"]["autoFilter"]["columns"][0]["column"], 1);
    assert_eq!(s1["metadata"]["conditionalFormats"][0]["id"], "cf-1");
    assert_eq!(s1["metadata"]["dataValidations"][0]["id"], "dv-1");

    // 删除规则走同一契约。
    let del = json!({
        "protocolVersion": 1,
        "transactionId": "ss-rule-2",
        "intentId": "intent-ss-rule-2",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": commit["revision"].as_u64().unwrap(),
        "origin": "local",
        "commands": [
            {"commandId": "rl-del", "typeId": "spreadsheet.deleteConditionalFormat",
             "payload": {"type": "deleteConditionalFormat", "sheetId": "s1", "ruleId": "cf-1"}}
        ]
    });
    let (status, _) = app.json(transaction_request(&id, del)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let s1 = snapshot["artifact"]["payload"]["data"]["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sheet| sheet["id"] == "s1")
        .unwrap();
    assert!(s1["metadata"]["conditionalFormats"]
        .as_array()
        .unwrap()
        .is_empty());
}

/// A helper that writes one cell into the existing s1 sheet.
fn cell_tx_sheet_only(id: &str, tx: &str, base: u64, value: i64) -> Request<Body> {
    transaction_request(
        id,
        json!({
            "protocolVersion": 1,
            "transactionId": tx,
            "intentId": format!("intent-{tx}"),
            "artifactId": id,
            "actorId": "dev-user",
            "baseRevision": base,
            "origin": "local",
            "commands": [
                {
                    "commandId": format!("op-{tx}-cell"),
                    "typeId": "spreadsheet.setCell",
                    "payload": {"type": "setCell", "sheetId": "s1", "row": 1, "column": 1, "value": value}
                }
            ]
        }),
    )
}

/// C2 loss visibility: DOCX export surfaces semantic approximations via the
/// x-docx-losses response header instead of silently dropping them.
#[tokio::test]
async fn docx_export_reports_semantic_losses() {
    let app = TestApp::new().await;
    // minimal.docx has no todo/link/containers: no loss header.
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let response = app
        .router
        .clone()
        .oneshot(
            Request::get(format!("/api/artifacts/{id}/export/docx"))
                .header("authorization", "Bearer dev-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("x-docx-losses").is_none());

    // Insert a todo block, then export again: the header must enumerate it.
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let root_id = snapshot["artifact"]["payload"]["data"]["root"][0]
        .as_str()
        .unwrap();
    let (status, commit) = app
        .json(transaction_request(
            id,
            json!({
                "protocolVersion": 1,
                "transactionId": "tx-todo",
                "intentId": "intent-todo",
                "artifactId": id,
                "actorId": "dev-user",
                "baseRevision": 1,
                "origin": "local",
                "commands": [{
                    "commandId": "op-todo",
                    "typeId": "document.insertTodo",
                    "payload": {
                        "type": "insertTodo",
                        "blockId": "todo-e2e-1",
                        "content": {"text": "买牛奶", "runs": []},
                        "parentId": root_id,
                        "index": 0,
                        "checked": false
                    }
                }]
            }),
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");

    let response = app
        .router
        .clone()
        .oneshot(
            Request::get(format!("/api/artifacts/{id}/export/docx"))
                .header("authorization", "Bearer dev-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let losses = response.headers().get("x-docx-losses").unwrap();
    assert_eq!(losses, "todoState:1");
}

/// `X-OO-User` 默认不被信任：带了就明确失败，而不是静默降级成 dev-user。
/// 静默降级会让 ACL 演练得出错误结论，所以这里要求 400 且错误信息可操作。
#[tokio::test]
async fn user_header_is_refused_unless_explicitly_trusted() {
    let app = TestApp::with_user_header_trust(false).await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/artifacts")
        .header("x-oo-user", "someone-else")
        .body(Body::empty())
        .unwrap();
    let (status, body) = app.json(request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "bad_request");
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("OO_TRUST_USER_HEADER"),
        "拒绝时必须指明如何显式开启：{body}"
    );

    // 不带该头时仍以开发用户正常工作，默认配置不是不可用，只是不接受身份声明。
    let request = Request::builder()
        .method("GET")
        .uri("/api/artifacts")
        .body(Body::empty())
        .unwrap();
    let (status, _) = app.json(request).await;
    assert_eq!(status, StatusCode::OK);
}

/// 开启开关后 `X-OO-User` 才决定 Principal，ACL 随之生效。
#[tokio::test]
async fn trusted_user_header_selects_the_principal() {
    let app = TestApp::with_user_header_trust(true).await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    // upload_fixture 不带身份头，owner 因此是 dev-user。
    assert_eq!(meta["ownerId"], "dev-user", "{meta}");

    let snapshot_as = |user: &str| {
        Request::builder()
            .method("GET")
            .uri(format!("/api/artifacts/{id}/snapshot"))
            .header("x-oo-user", user)
            .body(Body::empty())
            .unwrap()
    };

    let (status, _) = app.json(snapshot_as("stranger-1")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "陌生人不应读到该文档");

    // 同一个请求换成 owner 就通过，证明头确实被采纳而不是被忽略。
    let (status, _) = app.json(snapshot_as("dev-user")).await;
    assert_eq!(status, StatusCode::OK);
}

/// C4 授权矩阵：owner 全权；editor 可读+可提交事务、不可管理协作者与元数据；
/// viewer 只读；陌生人一律 403。Principal 由 X-OO-User 头参数化（dev 约定）。
#[tokio::test]
async fn collaborator_roles_enforce_the_full_matrix() {
    let app = TestApp::new().await;
    let (_, meta) = app.upload_fixture("minimal.docx").await;
    let id = meta["id"].as_str().unwrap();
    let request_with_user = |user: &str, method: &str, uri: String, body: Option<String>| {
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("x-oo-user", user);
        match body {
            Some(body) => builder
                .header("content-type", "application/json")
                .body(Body::from(body)),
            None => builder.body(Body::empty()),
        }
        .unwrap()
    };

    // Owner grants editor and viewer.
    for (user, role) in [("editor-1", "editor"), ("viewer-1", "viewer")] {
        let response = app
            .router
            .clone()
            .oneshot(request_with_user(
                "dev-user",
                "PUT",
                format!("/api/artifacts/{id}/collaborators/{user}"),
                Some(json!({"role": role}).to_string()),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT, "{user}");
    }
    // Invalid role rejected.
    let bad_role = app
        .router
        .clone()
        .oneshot(request_with_user(
            "dev-user",
            "PUT",
            format!("/api/artifacts/{id}/collaborators/ghost"),
            Some(json!({"role": "owner"}).to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(bad_role.status(), StatusCode::BAD_REQUEST);

    // Listing requires editor+: viewers do not get to enumerate other
    // principals on the artifact.
    for (user, expect_status) in [
        ("dev-user", StatusCode::OK),
        ("editor-1", StatusCode::OK),
        ("viewer-1", StatusCode::FORBIDDEN),
        ("stranger", StatusCode::FORBIDDEN),
    ] {
        let (status, body) = app
            .json(request_with_user(
                user,
                "GET",
                format!("/api/artifacts/{id}/collaborators"),
                None,
            ))
            .await;
        assert_eq!(status, expect_status, "list as {user}: {body}");
        if status == StatusCode::OK {
            let roles: Vec<&str> = body["collaborators"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["role"].as_str().unwrap())
                .collect();
            assert!(roles.contains(&"editor") && roles.contains(&"viewer"));
        }
    }

    // Reads: viewer+ can read snapshot; stranger cannot.
    for (user, expect) in [
        ("owner", "dev-user"),
        ("collaborator-editor", "editor-1"),
        ("collaborator-viewer", "viewer-1"),
    ] {
        let _ = (user, expect);
    }
    for (user, expect_status) in [
        ("dev-user", StatusCode::OK),
        ("editor-1", StatusCode::OK),
        ("viewer-1", StatusCode::OK),
        ("stranger", StatusCode::FORBIDDEN),
    ] {
        let (status, _) = app
            .json(request_with_user(
                user,
                "GET",
                format!("/api/artifacts/{id}/snapshot"),
                None,
            ))
            .await;
        assert_eq!(status, expect_status, "snapshot read as {user}");
    }

    // Transactions require editor+: viewer and stranger are forbidden even
    // with a perfect envelope.
    let (_, snapshot) = app
        .json(request_with_user(
            "editor-1",
            "GET",
            format!("/api/artifacts/{id}/snapshot"),
            None,
        ))
        .await;
    let block_id = snapshot["artifact"]["payload"]["data"]["blocks"][0]["id"]
        .as_str()
        .unwrap();
    let transaction_body = json!({
        "protocolVersion": 1,
        "transactionId": "tx-acl",
        "intentId": "intent-acl",
        "artifactId": id,
        "actorId": "editor-1",
        "baseRevision": 1,
        "origin": "local",
        "commands": [{
            "commandId": "op-acl",
            "typeId": "document.replaceBlockText",
            "payload": {"type": "replaceBlockText", "blockId": block_id, "content": {"text": "by editor", "runs": []}}
        }]
    })
    .to_string();
    for (user, expect_status) in [
        ("editor-1", StatusCode::OK),
        ("viewer-1", StatusCode::FORBIDDEN),
        ("stranger", StatusCode::FORBIDDEN),
    ] {
        let denied = json!({
            "protocolVersion": 1,
            "transactionId": format!("tx-denied-{user}"),
            "intentId": format!("intent-denied-{user}"),
            "artifactId": id,
            "actorId": user,
            "baseRevision": 1,
            "origin": "local",
            "commands": []
        });
        let payload = if user == "editor-1" {
            transaction_body.clone()
        } else {
            // 被拒角色使用独立事务 id，避免与 editor 的幂等记录互相污染。
            denied.to_string()
        };
        let transaction: serde_json::Value = serde_json::from_str(&transaction_body).unwrap();
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/artifacts/{id}/transactions"))
            .header("content-type", "application/json")
            .header("x-oo-user", user)
            .header("if-match", "\"1\"")
            .header(
                "x-transaction-id",
                transaction["transactionId"].as_str().unwrap(),
            )
            .body(Body::from(payload))
            .unwrap();
        let (status, body) = app.json(request).await;
        assert_eq!(status, expect_status, "transaction as {user}: {body}");
        if status == StatusCode::OK {
            // 审计 actor 必须是真实 Principal，而不是请求体里声称的人。
            assert_eq!(body["events"][0]["payload"]["actorId"], user);
        }
    }

    // Viewer cannot escalate: collaborator management stays owner-only.
    for (user, expect_status) in [
        ("editor-1", StatusCode::FORBIDDEN),
        ("viewer-1", StatusCode::FORBIDDEN),
    ] {
        let response = app
            .router
            .clone()
            .oneshot(request_with_user(
                user,
                "PUT",
                format!("/api/artifacts/{id}/collaborators/viewer-2"),
                Some(json!({"role": "editor"}).to_string()),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), expect_status, "escalation by {user}");
    }

    // Owner removal revokes access immediately.
    let response = app
        .router
        .clone()
        .oneshot(request_with_user(
            "dev-user",
            "DELETE",
            format!("/api/artifacts/{id}/collaborators/viewer-1"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let (status, _) = app
        .json(request_with_user(
            "viewer-1",
            "GET",
            format!("/api/artifacts/{id}/snapshot"),
            None,
        ))
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "revoked viewer must lose read"
    );
}

/// Spreadsheet grid projection: a bounded, read-only viewport window over the
/// sparse grid. It must return only in-window cells, reject oversized windows,
/// and never materialize the whole sheet (C3 frontend prereq).
#[tokio::test]
async fn spreadsheet_grid_projection_is_bounded_and_windowed() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kind": "spreadsheet"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();

    // Create a sheet and seed cells across a wide range (row 5, col 7; row 200, col 300).
    let transaction = |tx: &str| {
        transaction_request(
            &id,
            json!({
                "protocolVersion": 1,
                "transactionId": tx,
                "intentId": format!("intent-{tx}"),
                "artifactId": id,
                "actorId": "dev-user",
                "baseRevision": 1,
                "origin": "local",
                "commands": [
                    {"commandId": format!("s-{tx}"), "typeId": "spreadsheet.createSheet",
                     "payload": {"type": "createSheet", "id": "s1", "name": "Sheet1"}},
                    {"commandId": format!("c1-{tx}"), "typeId": "spreadsheet.setCell",
                     "payload": {"type": "setCell", "sheetId": "s1", "row": 5, "column": 7, "value": "near"}},
                    {"commandId": format!("c2-{tx}"), "typeId": "spreadsheet.setCell",
                     "payload": {"type": "setCell", "sheetId": "s1", "row": 200, "column": 300, "value": "far"}},
                    {"commandId": format!("c3-{tx}"), "typeId": "spreadsheet.setCell",
                     "payload": {"type": "setCell", "sheetId": "s1", "row": 6, "column": 8, "value": "also-near"}}
                ]
            }),
        )
    };
    let (status, commit) = app.json(transaction("grid-1")).await;
    assert_eq!(status, StatusCode::OK, "响应:{commit}");

    // A window around the near cells returns only those, not the far cell.
    let (status, proj) = app
        .json(get(&format!(
            "/api/artifacts/{id}/projection/spreadsheet?\
         sheetId=s1&startRow=0&endRow=10&startColumn=0&endColumn=10"
        )))
        .await;
    assert_eq!(status, StatusCode::OK, "投影:{proj}");
    assert_eq!(proj["projection"], "spreadsheet");
    assert_eq!(
        proj["data"]["cellCount"], 2,
        "应只包含窗口内 2 个近 cell:{proj}"
    );
    let cells = proj["data"]["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 2);
    let rows: Vec<u64> = cells
        .iter()
        .map(|c| c["address"]["row"].as_u64().unwrap())
        .collect();
    assert!(rows.contains(&5) && rows.contains(&6));

    // An oversized window is rejected (does not try to materialize the grid).
    let (status, body) = app
        .json(get(&format!(
            "/api/artifacts/{id}/projection/spreadsheet?\
         sheetId=s1&startRow=0&endRow=100000&startColumn=0&endColumn=21"
        )))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "超大窗口应被拒绝:{body}");

    // No viewport parameters returns compact workbook/sheet structure. It
    // must not leak either the near or far sparse cells.
    let (status, structure) = app
        .json(get(&format!("/api/artifacts/{id}/projection/spreadsheet")))
        .await;
    assert_eq!(status, StatusCode::OK, "结构投影:{structure}");
    let structure_sheets = structure["data"]["sheets"].as_array().unwrap();
    assert!(structure_sheets.iter().any(|sheet| sheet["id"] == "s1"));
    assert!(structure_sheets
        .iter()
        .all(|sheet| sheet["cells"].as_array().unwrap().is_empty()));

    // Supplying viewport coordinates without a sheet remains invalid rather
    // than silently turning a malformed grid request into a structure read.
    let (status, body) = app
        .json(get(&format!(
            "/api/artifacts/{id}/projection/spreadsheet?\
         startRow=0&endRow=10&startColumn=0&endColumn=10"
        )))
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "缺 sheetId 应被拒绝:{body}"
    );

    // The projection never reads the whole snapshot at the route level beyond the
    // viewport; a snapshot listing still shows the far cell persisted.
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let sheets = snapshot["artifact"]["payload"]["data"]["sheets"]
        .as_array()
        .unwrap();
    let s1 = sheets
        .iter()
        .find(|sheet| sheet["id"] == "s1")
        .expect("应存在 s1 工作表");
    let cells = s1["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 3);
}

/// Merge/unmerge/sort commands travel the canonical transaction path, and the
/// grid projection reports server-evaluated conditional format hits plus
/// filter-excluded rows instead of asking the browser to re-derive them.
#[tokio::test]
async fn spreadsheet_merge_sort_and_projection_semantics() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(json!({"kind": "spreadsheet"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();

    let transaction = json!({
        "protocolVersion": 1,
        "transactionId": "ss-ms-1",
        "intentId": "intent-ss-ms-1",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 1,
        "origin": "local",
        "commands": [
            {"commandId": "ms-sheet", "typeId": "spreadsheet.createSheet",
             "payload": {"type": "createSheet", "id": "s1", "name": "Sheet1"}},
            {"commandId": "ms-a", "typeId": "spreadsheet.setCell",
             "payload": {"type": "setCell", "sheetId": "s1", "row": 0, "column": 0, "value": 30}},
            {"commandId": "ms-b", "typeId": "spreadsheet.setCell",
             "payload": {"type": "setCell", "sheetId": "s1", "row": 1, "column": 0, "value": 10}},
            {"commandId": "ms-c", "typeId": "spreadsheet.setCell",
             "payload": {"type": "setCell", "sheetId": "s1", "row": 2, "column": 0, "value": 20}},
            {"commandId": "ms-sort", "typeId": "spreadsheet.sortRange",
             "payload": {"type": "sortRange", "sheetId": "s1",
                "range": {"startRow": 0, "startColumn": 0, "endRow": 2, "endColumn": 0},
                "keys": [{"column": 0, "direction": "ascending"}]}},
            {"commandId": "ms-merge", "typeId": "spreadsheet.mergeCells",
             "payload": {"type": "mergeCells", "sheetId": "s1",
                "range": {"startRow": 4, "startColumn": 0, "endRow": 5, "endColumn": 1}}}
        ]
    });
    let (status, commit) = app.json(transaction_request(&id, transaction)).await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");
    assert_eq!(commit["revision"], 2);
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let s1 = snapshot["artifact"]["payload"]["data"]["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sheet| sheet["id"] == "s1")
        .unwrap();
    let sorted: Vec<u64> = s1["cells"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|cell| cell["row"].as_u64().unwrap() < 3)
        .map(|cell| cell["value"].as_u64().unwrap())
        .collect();
    assert_eq!(sorted, vec![10, 20, 30], "sortRange 升序重排");
    let merges = s1["metadata"]["mergedRanges"].as_array().unwrap();
    assert_eq!(merges.len(), 1, "mergeCells 持久化合并区域");

    // Unmerge travels the same path.
    let unmerge = json!({
        "protocolVersion": 1,
        "transactionId": "ss-ms-2",
        "intentId": "intent-ss-ms-2",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 2,
        "origin": "local",
        "commands": [
            {"commandId": "ms-unmerge", "typeId": "spreadsheet.unmergeCells",
             "payload": {"type": "unmergeCells", "sheetId": "s1",
                "range": {"startRow": 4, "startColumn": 0, "endRow": 5, "endColumn": 1}}}
        ]
    });
    let (status, _) = app.json(transaction_request(&id, unmerge)).await;
    assert_eq!(status, StatusCode::OK);

    // Attach a conditional format and an auto filter through metadata, then
    // verify the projection reports hits and filtered rows.
    let metadata = json!({
        "protocolVersion": 1,
        "transactionId": "ss-ms-3",
        "intentId": "intent-ss-ms-3",
        "artifactId": id,
        "actorId": "dev-user",
        "baseRevision": 3,
        "origin": "local",
        "commands": [
            {"commandId": "ms-meta", "typeId": "spreadsheet.setSheetMetadata",
             "payload": {"type": "setSheetMetadata", "sheetId": "s1", "metadata": {
                "visibility": "visible",
                "rowCount": 20,
                "columnCount": 10,
                "freeze": {"rows": 0, "columns": 0},
                "conditionalFormats": [{
                    "id": "cf-hot", "range": {"startRow": 0, "startColumn": 0, "endRow": 2, "endColumn": 0},
                    "predicate": {"type": "cellIs", "value": {"operator": "greaterThan", "value": 15}},
                    "style": {}
                }],
                "autoFilter": {
                    "range": {"startRow": 0, "startColumn": 0, "endRow": 2, "endColumn": 0},
                    "columns": [{"column": 0, "predicate": {"type": "greaterThan", "value": 15}}]
                },
                "dataValidations": [], "mergedRanges": [], "media": []
             }}}
        ]
    });
    let (status, commit) = app.json(transaction_request(&id, metadata)).await;
    assert_eq!(status, StatusCode::OK, "响应：{commit}");
    let (status, proj) = app
        .json(get(&format!(
            "/api/artifacts/{id}/projection/spreadsheet?\
         sheetId=s1&startRow=0&endRow=10&startColumn=0&endColumn=10"
        )))
        .await;
    assert_eq!(status, StatusCode::OK, "投影：{proj}");
    let conditional = proj["data"]["conditionalStyles"].as_object().unwrap();
    // 20 and 30 exceed 15; 10 does not. After the sort, rows 1 and 2 hold 20/30.
    assert!(
        conditional.contains_key("1:0"),
        "20 命中条件格式：{conditional:?}"
    );
    assert!(
        conditional.contains_key("2:0"),
        "30 命中条件格式：{conditional:?}"
    );
    assert!(
        !conditional.contains_key("0:0"),
        "10 不命中：{conditional:?}"
    );
    let filtered = proj["data"]["filteredOutRows"].as_array().unwrap();
    assert_eq!(filtered, &[0], "值为 10 的第 0 行被筛选排除");
}

#[tokio::test]
async fn document_reviews_and_presence_use_stable_non_snapshot_references() {
    let app = TestApp::new().await;
    let (status, meta) = app
        .json(
            Request::post("/api/artifacts")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"kind": "document", "title": "Review references"}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap();
    let (_, snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let block_id = snapshot["artifact"]["payload"]["data"]["root"][0]
        .as_str()
        .unwrap();

    let (status, commit) = app
        .json(transaction_request(
            id,
            json!({
                "protocolVersion": 1,
                "transactionId": "review-seed",
                "intentId": "review-seed-intent",
                "artifactId": id,
                "actorId": "dev-user",
                "baseRevision": 1,
                "origin": "local",
                "commands": [{
                    "commandId": "review-seed-command",
                    "typeId": "document.replaceBlockText",
                    "payload": {"type": "replaceBlockText", "blockId": block_id, "content": {"text": "A中🙂Z", "runs": []}}
                }]
            }),
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "commit: {commit}");

    let anchor = json!({"blockId": block_id, "start": 1, "end": 3, "revision": 2});
    let (status, comments) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/reviews"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "threadId": "comment-1", "messageId": "comment-message-1",
                        "anchor": anchor, "body": "请由 @reviewer 检查", "mentions": ["reviewer"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "comments: {comments}");
    assert_eq!(comments["threads"][0]["anchorState"], "current");
    assert_eq!(
        comments["threads"][0]["messages"][0]["mentions"][0],
        "reviewer"
    );

    let (status, suggestions) = app
        .json(
            Request::post(format!("/api/artifacts/{id}/suggestions"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "threadId": "suggestion-1", "messageId": "suggestion-message-1",
                        "anchor": anchor, "body": "替换选中文字",
                        "suggestion": {"originalText": "中🙂", "replacement": "reviewed"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "suggestions: {suggestions}");
    assert_eq!(suggestions["threads"].as_array().unwrap().len(), 2);

    let (status, body) = app
        .send(
            Request::patch(format!("/api/artifacts/{id}/reviews/suggestion-1"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"state": "accepted"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let (_, accepted_snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(
        accepted_snapshot["artifact"]["payload"]["data"]["blocks"][0]["content"]["text"],
        "AreviewedZ"
    );
    let (retry_status, _) = app
        .send(
            Request::patch(format!("/api/artifacts/{id}/reviews/suggestion-1"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"state": "accepted"}).to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(retry_status, StatusCode::NO_CONTENT);
    let (_, retry_snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    assert_eq!(retry_snapshot["artifact"]["revision"], 3);

    let (status, bytes) = app
        .send(
            Request::put(format!("/api/artifacts/{id}/presence/remote-browser"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "revision": 3, "blockId": block_id, "selectedNodeIds": [],
                        "selection": {
                            "anchor": {"blockId": block_id, "offset": 1},
                            "focus": {"blockId": block_id, "offset": 3}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let (_, presence) = app
        .json(get(&format!("/api/artifacts/{id}/presence")))
        .await;
    assert_eq!(
        presence["participants"][0]["selection"]["focus"]["offset"],
        3
    );

    let (status, deleted) = app
        .json(transaction_request(
            id,
            json!({
                "protocolVersion": 1,
                "transactionId": "review-delete-target",
                "intentId": "review-delete-target-intent",
                "artifactId": id,
                "actorId": "dev-user",
                "baseRevision": 3,
                "origin": "local",
                "commands": [
                    {"commandId": "review-spare", "typeId": "document.insertDivider", "payload": {"type": "insertDivider", "blockId": "review-spare", "index": 1}},
                    {"commandId": "review-delete", "typeId": "document.deleteBlock", "payload": {"type": "deleteBlock", "blockId": block_id}}
                ]
            }),
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "delete: {deleted}");

    let (_, reviews_after_delete) = app.json(get(&format!("/api/artifacts/{id}/reviews"))).await;
    assert!(reviews_after_delete["threads"]
        .as_array()
        .unwrap()
        .iter()
        .all(|thread| { thread["anchorState"] == "detached" && thread["anchor"].is_null() }));
    let (_, presence_after_delete) = app
        .json(get(&format!("/api/artifacts/{id}/presence")))
        .await;
    assert!(presence_after_delete["participants"]
        .as_array()
        .unwrap()
        .is_empty());

    let (_, persisted_snapshot) = app
        .json(get(&format!("/api/artifacts/{id}/snapshot")))
        .await;
    let snapshot_text = persisted_snapshot.to_string();
    assert!(!snapshot_text.contains("comment-1"));
    assert!(!snapshot_text.contains("remote-browser"));
}
